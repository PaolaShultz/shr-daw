//! Bounded Standard MIDI File parsing and tick-domain FT2 conversion.
//! This deliberately does not share the Ideas parser's legacy elapsed-time
//! behavior.
use crate::sequencer::{
    ColumnSetup, Command, Note, Page, PageTarget, Pattern, Song, LANES_PER_PAGE,
};
use crate::tempo::Bpm;
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MAX_MIDI_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_MIDI_TRACKS: usize = 64;
pub const MAX_MIDI_EVENTS: usize = 262_144;
pub const MAX_MIDI_TICK: u64 = 100_000_000;
const MAX_IMPORT_PAGES: usize = 64;
const MAX_IMPORT_PATTERNS: usize = 256;
const MAX_IMPORT_CELLS: usize = 1_048_576;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmfFormat {
    Format0,
    Format1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TempoEvent {
    pub tick: u64,
    pub tempo: Bpm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeSignature {
    pub tick: u64,
    pub numerator: u8,
    pub denominator: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeySignature {
    pub tick: u64,
    pub sharps_flats: i8,
    pub minor: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiEventKind {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
    Control { controller: u8, value: u8 },
    Program { program: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiEvent {
    pub tick: u64,
    pub channel: u8,
    pub ordinal: u32,
    pub kind: MidiEventKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MidiTrack {
    pub name: Option<String>,
    pub events: Vec<MidiEvent>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StrippedEvents {
    pub text: usize,
    pub key_signatures: usize,
    pub sysex: usize,
    pub aftertouch: usize,
    pub system_common: usize,
    pub realtime: usize,
    pub sequencer_metadata: usize,
    pub unsupported_metadata: usize,
    pub unsupported_cc: usize,
    pub pitch_bend: usize,
    pub later_bank_program: usize,
}

impl StrippedEvents {
    pub fn total(&self) -> usize {
        self.text
            + self.key_signatures
            + self.sysex
            + self.aftertouch
            + self.system_common
            + self.realtime
            + self.sequencer_metadata
            + self.unsupported_metadata
            + self.unsupported_cc
            + self.pitch_bend
            + self.later_bank_program
    }

    pub fn compact(&self) -> String {
        let mut groups = Vec::new();
        for (label, count) in [
            ("text", self.text),
            ("key", self.key_signatures),
            ("SysEx", self.sysex),
            ("pressure", self.aftertouch),
            ("system", self.system_common + self.realtime),
            (
                "metadata",
                self.sequencer_metadata + self.unsupported_metadata,
            ),
            ("CC", self.unsupported_cc),
            ("bend", self.pitch_bend),
            ("bank/program", self.later_bank_program),
        ] {
            if count > 0 {
                groups.push(format!("{label} {count}"));
            }
        }
        if groups.is_empty() {
            "none".into()
        } else {
            groups.join(", ")
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Smf {
    pub format: SmfFormat,
    pub ppqn: u16,
    pub tracks: Vec<MidiTrack>,
    pub tempos: Vec<TempoEvent>,
    pub time_signatures: Vec<TimeSignature>,
    pub key_signatures: Vec<KeySignature>,
    pub stripped: StrippedEvents,
    pub event_count: usize,
    pub maximum_tick: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportReport {
    pub source_format: SmfFormat,
    pub source_tracks: usize,
    pub ppqn: u16,
    pub parts: usize,
    pub pages: usize,
    pub patterns: usize,
    pub rows: usize,
    pub steps_per_beat: u8,
    pub tempos: Vec<TempoEvent>,
    pub source_meter: (u8, u16),
    pub project_meter: u8,
    pub meter_mapping: Option<String>,
    pub key_signature: Option<KeySignature>,
    pub note_ons: usize,
    pub maximum_polyphony: usize,
    pub exact_events: usize,
    pub quantized_events: usize,
    pub maximum_displacement_ticks: u64,
    pub stripped: StrippedEvents,
    pub unmatched_note_offs: usize,
    pub hanging_notes: usize,
    pub sustained_notes: usize,
}

impl ImportReport {
    pub fn warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Some(mapping) = &self.meter_mapping {
            warnings.push(mapping.clone());
        }
        if let Some(key) = self.key_signature {
            warnings.push(format!(
                "key {} reported; Project has no key field",
                key_label(key)
            ));
        }
        if self.quantized_events > 0 {
            warnings.push(format!(
                "{} event(s) quantized; max {} tick(s)",
                self.quantized_events, self.maximum_displacement_ticks
            ));
        }
        if self.hanging_notes > 0 || self.unmatched_note_offs > 0 {
            warnings.push(format!(
                "{} hanging / {} unmatched note-off(s)",
                self.hanging_notes, self.unmatched_note_offs
            ));
        }
        if self.stripped.total() > 0 {
            warnings.push(format!("stripped {}", self.stripped.compact()));
        }
        warnings
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedProject {
    pub song: Song,
    pub report: ImportReport,
}

pub fn discover(directory: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("mid") || extension.eq_ignore_ascii_case("midi")
                })
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| {
        left.file_name()
            .cmp(&right.file_name())
            .then_with(|| left.cmp(right))
    });
    files
}

pub fn import_path(path: &Path) -> Result<ImportedProject> {
    let bytes = read_regular_file(path)?;
    let parsed = parse(&bytes)?;
    let name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(crate::sequencer::safe_name)
        .unwrap_or_else(|| "midi-import".into());
    convert(&parsed, &name)
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>> {
    let link_metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    if !link_metadata.file_type().is_file() {
        bail!("MIDI import accepts regular files only");
    }
    if link_metadata.len() > MAX_MIDI_BYTES {
        bail!("MIDI file exceeds {MAX_MIDI_BYTES} bytes");
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Linux O_NOFOLLOW. SHR-DAW's supported target is Raspberry Pi OS.
        options.custom_flags(0x20_000);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("open regular MIDI file {}", path.display()))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        bail!("MIDI import accepts regular files only");
    }
    let capacity = usize::try_from(metadata.len()).context("MIDI file size overflow")?;
    let mut bytes = Vec::with_capacity(capacity);
    file.by_ref()
        .take(MAX_MIDI_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MIDI_BYTES {
        bail!("MIDI file exceeds {MAX_MIDI_BYTES} bytes");
    }
    Ok(bytes)
}

pub fn parse(bytes: &[u8]) -> Result<Smf> {
    if bytes.len() as u64 > MAX_MIDI_BYTES {
        bail!("MIDI file exceeds {MAX_MIDI_BYTES} bytes");
    }
    let mut cursor = Cursor::new(bytes);
    if cursor.take(4)? != b"MThd" {
        bail!("not a Standard MIDI File");
    }
    let header_length = cursor.u32()? as usize;
    if header_length < 6 {
        bail!("truncated MIDI header");
    }
    let header = cursor.take(header_length)?;
    let format_number = u16::from_be_bytes([header[0], header[1]]);
    let format = match format_number {
        0 => SmfFormat::Format0,
        1 => SmfFormat::Format1,
        2 => bail!("SMF format 2 is not supported"),
        value => bail!("unsupported SMF format {value}"),
    };
    let track_count = usize::from(u16::from_be_bytes([header[2], header[3]]));
    if track_count == 0 || track_count > MAX_MIDI_TRACKS {
        bail!("MIDI track count must be 1..={MAX_MIDI_TRACKS}");
    }
    if format == SmfFormat::Format0 && track_count != 1 {
        bail!("SMF format 0 must contain exactly one track");
    }
    let division = u16::from_be_bytes([header[4], header[5]]);
    if division & 0x8000 != 0 {
        bail!("SMPTE MIDI timing is not supported; PPQN is required");
    }
    if division == 0 {
        bail!("MIDI PPQN division must be greater than zero");
    }

    let mut tracks = Vec::with_capacity(track_count);
    let mut tempos = Vec::new();
    let mut time_signatures = Vec::new();
    let mut key_signatures = Vec::new();
    let mut stripped = StrippedEvents::default();
    let mut total_events = 0usize;
    let mut maximum_tick = 0u64;
    for track_index in 0..track_count {
        if cursor.take(4)? != b"MTrk" {
            bail!("missing MIDI track chunk {track_index}");
        }
        let length = usize::try_from(cursor.u32()?).context("MIDI track size overflow")?;
        let data = cursor.take(length)?;
        let track = parse_track(
            data,
            &mut tempos,
            &mut time_signatures,
            &mut key_signatures,
            &mut stripped,
            &mut total_events,
            &mut maximum_tick,
        )
        .map_err(|error| anyhow::anyhow!("MIDI track {}: {error}", track_index + 1))?;
        tracks.push(track);
    }
    if !cursor.remaining().is_empty() {
        bail!("unexpected data follows the declared MIDI tracks");
    }
    tempos.sort_by_key(|event| event.tick);
    time_signatures.sort_by_key(|event| event.tick);
    key_signatures.sort_by_key(|event| event.tick);
    Ok(Smf {
        format,
        ppqn: division,
        tracks,
        tempos,
        time_signatures,
        key_signatures,
        stripped,
        event_count: total_events,
        maximum_tick,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_track(
    data: &[u8],
    tempos: &mut Vec<TempoEvent>,
    time_signatures: &mut Vec<TimeSignature>,
    key_signatures: &mut Vec<KeySignature>,
    stripped: &mut StrippedEvents,
    total_events: &mut usize,
    maximum_tick: &mut u64,
) -> Result<MidiTrack> {
    let mut cursor = Cursor::new(data);
    let mut tick = 0u64;
    let mut running_status = None;
    let mut name = None;
    let mut events = Vec::new();
    let mut ordinal = 0u32;
    let mut ended = false;
    while !cursor.remaining().is_empty() {
        let delta = u64::from(cursor.vlq()?);
        tick = tick.checked_add(delta).context("MIDI tick overflow")?;
        if tick > MAX_MIDI_TICK {
            bail!("MIDI duration exceeds {MAX_MIDI_TICK} ticks");
        }
        *maximum_tick = (*maximum_tick).max(tick);
        *total_events = total_events
            .checked_add(1)
            .context("MIDI event count overflow")?;
        if *total_events > MAX_MIDI_EVENTS {
            bail!("MIDI exceeds {MAX_MIDI_EVENTS} events");
        }
        let first = cursor.peek().context("truncated MIDI event")?;
        let status = if first & 0x80 != 0 {
            cursor.byte()?
        } else {
            running_status.context("MIDI running status has no channel status")?
        };
        if (0x80..=0xef).contains(&status) {
            running_status = Some(status);
            let channel = status & 0x0f;
            let kind = status & 0xf0;
            let first_data = cursor.data_byte()?;
            let second_data = if matches!(kind, 0xc0 | 0xd0) {
                None
            } else {
                Some(cursor.data_byte()?)
            };
            let parsed = match kind {
                0x80 => Some(MidiEventKind::NoteOff { note: first_data }),
                0x90 if second_data == Some(0) => Some(MidiEventKind::NoteOff { note: first_data }),
                0x90 => Some(MidiEventKind::NoteOn {
                    note: first_data,
                    velocity: second_data.unwrap_or_default(),
                }),
                0xa0 | 0xd0 => {
                    stripped.aftertouch += 1;
                    None
                }
                0xb0 => Some(MidiEventKind::Control {
                    controller: first_data,
                    value: second_data.unwrap_or_default(),
                }),
                0xc0 => Some(MidiEventKind::Program {
                    program: first_data,
                }),
                0xe0 => {
                    stripped.pitch_bend += 1;
                    None
                }
                _ => bail!("invalid MIDI channel status {status:#04x}"),
            };
            if let Some(kind) = parsed {
                events.push(MidiEvent {
                    tick,
                    channel,
                    ordinal,
                    kind,
                });
            }
        } else if status == 0xff {
            running_status = None;
            let meta_type = cursor.byte()?;
            let length = usize::try_from(cursor.vlq()?).context("MIDI metadata size overflow")?;
            let payload = cursor.take(length)?;
            match meta_type {
                0x02 | 0x05 | 0x06 | 0x07 => stripped.text += 1,
                0x03 => {
                    if name.is_none() {
                        name = Some(safe_track_name(payload));
                    }
                }
                0x2f => {
                    if !payload.is_empty() {
                        bail!("end-of-track metadata must be empty");
                    }
                    ended = true;
                    if !cursor.remaining().is_empty() {
                        bail!("data follows end-of-track metadata");
                    }
                }
                0x51 => {
                    if payload.len() != 3 {
                        bail!("tempo metadata must contain three bytes");
                    }
                    let micros = u32::from_be_bytes([0, payload[0], payload[1], payload[2]]);
                    tempos.push(TempoEvent {
                        tick,
                        tempo: Bpm::from_micros_per_quarter(micros)?,
                    });
                }
                0x58 => {
                    if payload.len() != 4 || payload[1] > 15 {
                        bail!("invalid time-signature metadata");
                    }
                    time_signatures.push(TimeSignature {
                        tick,
                        numerator: payload[0],
                        denominator: 1u16 << payload[1],
                    });
                }
                0x59 => {
                    if payload.len() != 2 || (payload[0] as i8) < -7 || (payload[0] as i8) > 7 {
                        bail!("invalid key-signature metadata");
                    }
                    key_signatures.push(KeySignature {
                        tick,
                        sharps_flats: payload[0] as i8,
                        minor: payload[1] != 0,
                    });
                    stripped.key_signatures += 1;
                }
                0x54 => stripped.system_common += 1,
                0x7f => stripped.sequencer_metadata += 1,
                _ => stripped.unsupported_metadata += 1,
            }
        } else if matches!(status, 0xf0 | 0xf7) {
            running_status = None;
            let length = usize::try_from(cursor.vlq()?).context("MIDI SysEx size overflow")?;
            cursor.take(length)?;
            stripped.sysex += 1;
        } else if (0xf8..=0xfe).contains(&status) {
            // Realtime bytes do not cancel channel running status.
            stripped.realtime += 1;
        } else {
            running_status = None;
            let data_bytes = match status {
                0xf1 | 0xf3 => 1,
                0xf2 => 2,
                0xf4..=0xf6 => 0,
                _ => bail!("invalid MIDI system status {status:#04x}"),
            };
            for _ in 0..data_bytes {
                cursor.data_byte()?;
            }
            stripped.system_common += 1;
        }
        ordinal = ordinal
            .checked_add(1)
            .context("MIDI event ordinal overflow")?;
        if ended {
            break;
        }
    }
    if !ended {
        bail!("MIDI track is missing end-of-track metadata");
    }
    Ok(MidiTrack { name, events })
}

#[derive(Clone, Debug)]
struct NoteSpan {
    note: u8,
    velocity: u8,
    start: u64,
    end: u64,
    ordinal: u32,
}

#[derive(Clone, Copy, Debug)]
struct ActiveNote {
    note: u8,
    velocity: u8,
    start: u64,
    ordinal: u32,
}

#[derive(Clone, Debug)]
struct Part {
    track: usize,
    channel: u8,
    name: String,
    bank_msb: u8,
    bank_lsb: u8,
    program: u8,
    notes: Vec<NoteSpan>,
}

#[derive(Clone, Copy, Debug)]
struct PlacedNote {
    lane: usize,
    note: u8,
    velocity: u8,
    start: u64,
    end: u64,
}

pub fn convert(smf: &Smf, project_name: &str) -> Result<ImportedProject> {
    let (source_meter, project_meter, meter_mapping) = fixed_meter(smf)?;
    let mut stripped = smf.stripped.clone();
    let mut unmatched_note_offs = 0usize;
    let mut hanging_notes = 0usize;
    let mut sustained_notes = 0usize;
    let mut parts = pair_parts(
        smf,
        &mut stripped,
        &mut unmatched_note_offs,
        &mut hanging_notes,
        &mut sustained_notes,
    )?;
    if parts.is_empty() {
        bail!("MIDI file contains no note parts");
    }
    parts.sort_by_key(|part| (part.track, part.channel));
    let all_notes = parts
        .iter()
        .flat_map(|part| part.notes.iter())
        .collect::<Vec<_>>();
    let steps_per_beat = infer_steps_per_beat(&all_notes, smf.ppqn);
    let maximum_tick = all_notes
        .iter()
        .map(|note| note.end.max(note.start))
        .max()
        .unwrap_or(smf.maximum_tick);
    let total_rows = ceil_rows(maximum_tick, smf.ppqn, steps_per_beat).max(1);
    let bar_rows = usize::from(project_meter) * usize::from(steps_per_beat);
    let spans = pattern_spans(total_rows, bar_rows)?;

    let track_part_counts = parts.iter().fold(BTreeMap::new(), |mut counts, part| {
        *counts.entry(part.track).or_insert(0usize) += 1;
        counts
    });
    let mut pages = Vec::new();
    let mut placed = Vec::new();
    let mut maximum_polyphony = 0usize;
    for part in &parts {
        let mut free_until: Vec<[u64; LANES_PER_PAGE]> = Vec::new();
        let mut page_indices = Vec::new();
        let mut notes = part.notes.clone();
        notes.sort_by_key(|note| (note.start, note.note, note.ordinal));
        for note in notes {
            let simultaneous = free_until
                .iter()
                .flat_map(|lanes| lanes.iter())
                .filter(|free| **free > note.start)
                .count();
            maximum_polyphony = maximum_polyphony.max(simultaneous + 1);
            let location = free_until.iter().enumerate().find_map(|(page, lanes)| {
                lanes
                    .iter()
                    .position(|free| *free <= note.start)
                    .map(|lane| (page, lane))
            });
            let (part_page, lane) = if let Some(location) = location {
                location
            } else {
                if pages.len() >= MAX_IMPORT_PAGES {
                    bail!("MIDI import exceeds {MAX_IMPORT_PAGES} tracker pages");
                }
                let suffix = free_until.len() + 1;
                let multiple_channels =
                    track_part_counts.get(&part.track).copied().unwrap_or(1) > 1;
                let mut name = if multiple_channels {
                    format!("{} ch{}", part.name, part.channel + 1)
                } else {
                    part.name.clone()
                };
                if suffix > 1 {
                    name = format!("{name} #{suffix}");
                }
                let mut page = Page::new(
                    &safe_page_name(&name),
                    part.channel,
                    part.channel == 9,
                    part.program,
                );
                page.target = PageTarget::ConfiguredExternal;
                page.columns = [ColumnSetup {
                    channel: part.channel,
                    bank_msb: part.bank_msb,
                    bank_lsb: part.bank_lsb,
                    program: part.program,
                }; LANES_PER_PAGE];
                pages.push(page);
                page_indices.push(pages.len() - 1);
                free_until.push([0; LANES_PER_PAGE]);
                (free_until.len() - 1, 0)
            };
            free_until[part_page][lane] = note.end.max(note.start + 1);
            placed.push(PlacedNote {
                lane: page_indices[part_page] * LANES_PER_PAGE + lane,
                note: note.note,
                velocity: note.velocity,
                start: note.start,
                end: note.end.max(note.start + 1),
            });
        }
    }
    let cell_count = spans
        .iter()
        .try_fold(0usize, |total, rows| {
            rows.checked_mul(pages.len() * LANES_PER_PAGE)
                .and_then(|cells| total.checked_add(cells))
        })
        .context("MIDI import cell count overflow")?;
    if cell_count > MAX_IMPORT_CELLS {
        bail!("MIDI import exceeds {MAX_IMPORT_CELLS} tracker cells");
    }

    let tempos = normalized_tempos(smf)?;
    let mut patterns = BTreeMap::new();
    let mut order = Vec::new();
    let mut row_origin = 0usize;
    for (number, rows) in spans.iter().copied().enumerate() {
        let number = u16::try_from(number).context("MIDI Pattern number overflow")?;
        let tick_origin = row_to_tick(row_origin, smf.ppqn, steps_per_beat)?;
        let tempo = tempo_at(&tempos, tick_origin);
        patterns.insert(
            number,
            Pattern::new(rows, tempo, project_meter, pages.clone()),
        );
        order.push(number);
        row_origin = row_origin.checked_add(rows).context("MIDI row overflow")?;
    }
    let mut exact_events = 0usize;
    let mut quantized_events = 0usize;
    let mut maximum_displacement_ticks = 0u64;
    for note in &placed {
        let start = quantize_tick(note.start, smf.ppqn, steps_per_beat)?;
        record_quantization(
            start.displacement,
            &mut exact_events,
            &mut quantized_events,
            &mut maximum_displacement_ticks,
        );
        let (pattern_number, local_row) =
            locate_row(start.row, &spans).context("quantized note starts beyond Project")?;
        let cell = patterns
            .get_mut(&(pattern_number as u16))
            .and_then(|pattern| pattern.rows.get_mut(local_row))
            .and_then(|row| row.get_mut(note.lane))
            .context("allocated MIDI note cell is outside Project")?;
        if !matches!(cell.note, Note::Empty | Note::Off) {
            bail!("deterministic MIDI lane allocation collision");
        }
        cell.note = Note::On(note.note);
        cell.velocity = Some(note.velocity);

        let duration = note.end.saturating_sub(note.start);
        let row_ticks_numerator = u64::from(smf.ppqn);
        let duration_rows_numerator = duration
            .checked_mul(u64::from(steps_per_beat))
            .context("MIDI duration scaling overflow")?;
        if duration_rows_numerator < row_ticks_numerator {
            let percentage_numerator = duration_rows_numerator
                .checked_mul(100)
                .context("MIDI gate scaling overflow")?;
            if percentage_numerator % row_ticks_numerator == 0 {
                let gate = (percentage_numerator / row_ticks_numerator).clamp(1, 100) as u8;
                cell.gate = Some(gate);
                exact_events += 1;
            } else {
                let gate = ((percentage_numerator + row_ticks_numerator / 2) / row_ticks_numerator)
                    .clamp(1, 100) as u8;
                cell.gate = Some(gate);
                quantized_events += 1;
                let represented = u64::from(gate) * row_ticks_numerator;
                maximum_displacement_ticks = maximum_displacement_ticks.max(
                    represented
                        .abs_diff(percentage_numerator)
                        .div_ceil(100 * u64::from(steps_per_beat)),
                );
            }
        } else {
            // A full-row or longer note is owned by its later OFF, next note,
            // or the Project boundary. The scheduler recognises 100% as that
            // explicit-release marker instead of applying the inherited gate.
            cell.gate = Some(100);
            let end = quantize_tick(note.end, smf.ppqn, steps_per_beat)?;
            record_quantization(
                end.displacement,
                &mut exact_events,
                &mut quantized_events,
                &mut maximum_displacement_ticks,
            );
            if end.row <= total_rows {
                // A release exactly at Project end is stored on the final row
                // with a 100% gate marker, meaning the OFF occurs at that
                // row's end. This preserves the musical boundary without
                // adding a visible 161st row to a 160-row import.
                let terminal = end.row == total_rows;
                let stored_row = if terminal {
                    end.row.saturating_sub(1)
                } else {
                    end.row
                };
                if terminal && stored_row == start.row {
                    continue;
                }
                let (end_pattern, end_row) =
                    locate_row(stored_row, &spans).context("quantized note end beyond Project")?;
                let end_cell = patterns
                    .get_mut(&(end_pattern as u16))
                    .and_then(|pattern| pattern.rows.get_mut(end_row))
                    .and_then(|row| row.get_mut(note.lane))
                    .context("allocated MIDI note-off cell is outside Project")?;
                if end_cell.note == Note::Empty {
                    end_cell.note = Note::Off;
                    if terminal {
                        end_cell.gate = Some(100);
                    }
                }
            }
        }
    }
    place_tempo_commands(
        &mut patterns,
        &spans,
        &tempos,
        smf.ppqn,
        steps_per_beat,
        &mut exact_events,
        &mut quantized_events,
        &mut maximum_displacement_ticks,
    )?;
    let song = Song {
        name: crate::sequencer::safe_name(project_name),
        project_key: Default::default(),
        drum_kit: "electronic-house".into(),
        drum_tuning: Default::default(),
        steps_per_beat,
        gate_percent: 80,
        insert_rack: Default::default(),
        aux_routing: Default::default(),
        master_strip: Default::default(),
        order,
        patterns,
    };
    song.validate()?;
    let report = ImportReport {
        source_format: smf.format,
        source_tracks: smf.tracks.len(),
        ppqn: smf.ppqn,
        parts: parts.len(),
        pages: pages.len(),
        patterns: spans.len(),
        rows: total_rows,
        steps_per_beat,
        tempos,
        source_meter,
        project_meter,
        meter_mapping,
        key_signature: smf.key_signatures.first().copied(),
        note_ons: placed.len(),
        maximum_polyphony,
        exact_events,
        quantized_events,
        maximum_displacement_ticks,
        stripped,
        unmatched_note_offs,
        hanging_notes,
        sustained_notes,
    };
    Ok(ImportedProject { song, report })
}

fn pair_parts(
    smf: &Smf,
    stripped: &mut StrippedEvents,
    unmatched_note_offs: &mut usize,
    hanging_notes: &mut usize,
    sustained_notes: &mut usize,
) -> Result<Vec<Part>> {
    let mut parts = Vec::new();
    for (track_index, track) in smf.tracks.iter().enumerate() {
        let channels = track
            .events
            .iter()
            .filter_map(|event| {
                matches!(
                    event.kind,
                    MidiEventKind::NoteOn { .. } | MidiEventKind::NoteOff { .. }
                )
                .then_some(event.channel)
            })
            .collect::<BTreeSet<_>>();
        for channel in channels {
            let first_note_position = track
                .events
                .iter()
                .find(|event| {
                    event.channel == channel && matches!(event.kind, MidiEventKind::NoteOn { .. })
                })
                .map(|event| (event.tick, event.ordinal))
                .unwrap_or((0, 0));
            let mut bank_msb = 0;
            let mut bank_lsb = 0;
            let mut program = 0;
            let mut sustain = false;
            let mut active: BTreeMap<u8, VecDeque<ActiveNote>> = BTreeMap::new();
            let mut sustained = Vec::new();
            let mut notes = Vec::new();
            for event in track.events.iter().filter(|event| event.channel == channel) {
                match event.kind {
                    MidiEventKind::NoteOn { note, velocity } => {
                        active.entry(note).or_default().push_back(ActiveNote {
                            note,
                            velocity,
                            start: event.tick,
                            ordinal: event.ordinal,
                        });
                    }
                    MidiEventKind::NoteOff { note } => {
                        let released = active.get_mut(&note).and_then(VecDeque::pop_front);
                        if active.get(&note).is_some_and(VecDeque::is_empty) {
                            active.remove(&note);
                        }
                        if let Some(released) = released {
                            if sustain {
                                sustained.push(released);
                                *sustained_notes += 1;
                            } else {
                                finish_note(track_index, channel, released, event.tick, &mut notes);
                            }
                        } else {
                            *unmatched_note_offs += 1;
                        }
                    }
                    MidiEventKind::Control {
                        controller: 64,
                        value,
                    } => {
                        let next = value >= 64;
                        if sustain && !next {
                            for released in sustained.drain(..) {
                                finish_note(track_index, channel, released, event.tick, &mut notes);
                            }
                        }
                        sustain = next;
                    }
                    MidiEventKind::Control { controller, value }
                        if matches!(controller, 0 | 32) =>
                    {
                        if (event.tick, event.ordinal) < first_note_position {
                            if controller == 0 {
                                bank_msb = value;
                            } else {
                                bank_lsb = value;
                            }
                        } else {
                            stripped.later_bank_program += 1;
                        }
                    }
                    MidiEventKind::Control { .. } => stripped.unsupported_cc += 1,
                    MidiEventKind::Program { program: next } => {
                        if (event.tick, event.ordinal) < first_note_position {
                            program = next;
                        } else {
                            stripped.later_bank_program += 1;
                        }
                    }
                }
            }
            let final_tick = smf.maximum_tick.max(
                active
                    .values()
                    .flatten()
                    .chain(sustained.iter())
                    .map(|note| note.start + 1)
                    .max()
                    .unwrap_or(0),
            );
            for hanging in active.into_values().flatten().chain(sustained.into_iter()) {
                *hanging_notes += 1;
                finish_note(track_index, channel, hanging, final_tick, &mut notes);
            }
            if !notes.is_empty() {
                parts.push(Part {
                    track: track_index,
                    channel,
                    name: track
                        .name
                        .clone()
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| format!("Track {}", track_index + 1)),
                    bank_msb,
                    bank_lsb,
                    program,
                    notes,
                });
            }
        }
    }
    Ok(parts)
}

fn finish_note(
    _track: usize,
    _channel: u8,
    active: ActiveNote,
    end: u64,
    notes: &mut Vec<NoteSpan>,
) {
    notes.push(NoteSpan {
        note: active.note,
        velocity: active.velocity,
        start: active.start,
        end: end.max(active.start + 1),
        ordinal: active.ordinal,
    });
}

fn fixed_meter(smf: &Smf) -> Result<((u8, u16), u8, Option<String>)> {
    if smf.time_signatures.is_empty() {
        return Ok(((4, 4), 4, None));
    }
    let first = smf.time_signatures[0];
    if first.tick != 0 {
        bail!("changing meter is not supported; first signature begins after tick 0");
    }
    if smf.time_signatures.iter().any(|signature| {
        signature.numerator != first.numerator || signature.denominator != first.denominator
    }) {
        bail!("changing meter is not supported in MIDI import");
    }
    match (first.numerator, first.denominator) {
        (3, 4) => Ok(((3, 4), 3, None)),
        (4, 4) => Ok(((4, 4), 4, None)),
        (6, 8) => Ok((
            (6, 8),
            3,
            Some("6/8 shown on the compound 3/4 tracker grid".into()),
        )),
        meter => bail!(
            "MIDI meter {}/{} is not supported; use fixed 3/4, 4/4, or 6/8",
            meter.0,
            meter.1
        ),
    }
}

fn normalized_tempos(smf: &Smf) -> Result<Vec<TempoEvent>> {
    if smf.tempos.is_empty() {
        return Ok(vec![TempoEvent {
            tick: 0,
            tempo: Bpm::DEFAULT,
        }]);
    }
    let mut by_tick = BTreeMap::new();
    for event in &smf.tempos {
        by_tick.insert(event.tick, event.tempo);
    }
    by_tick.entry(0).or_insert(Bpm::DEFAULT);
    Ok(by_tick
        .into_iter()
        .map(|(tick, tempo)| TempoEvent { tick, tempo })
        .collect())
}

fn tempo_at(events: &[TempoEvent], tick: u64) -> Bpm {
    events
        .iter()
        .take_while(|event| event.tick <= tick)
        .last()
        .map_or(Bpm::DEFAULT, |event| event.tempo)
}

fn infer_steps_per_beat(notes: &[&NoteSpan], ppqn: u16) -> u8 {
    let exact = (1..=16).find(|steps| {
        notes
            .iter()
            .all(|note| note.start * *steps as u64 % u64::from(ppqn) == 0)
    });
    match exact {
        Some(2) => 4,
        Some(steps) => steps,
        None => 16,
    }
}

fn ceil_rows(tick: u64, ppqn: u16, steps: u8) -> usize {
    let numerator = tick.saturating_mul(u64::from(steps));
    usize::try_from(numerator.div_ceil(u64::from(ppqn))).unwrap_or(usize::MAX)
}

fn pattern_spans(total_rows: usize, bar_rows: usize) -> Result<Vec<usize>> {
    if total_rows == 0 {
        return Ok(vec![1]);
    }
    let full = 256usize
        .checked_sub(256 % bar_rows.max(1))
        .filter(|rows| *rows > 0)
        .unwrap_or(256);
    let mut remaining = total_rows;
    let mut spans = Vec::new();
    while remaining > 0 {
        if spans.len() >= MAX_IMPORT_PATTERNS {
            bail!("MIDI import exceeds {MAX_IMPORT_PATTERNS} Patterns");
        }
        let rows = remaining.min(full).min(256);
        spans.push(rows);
        remaining -= rows;
    }
    Ok(spans)
}

#[derive(Clone, Copy, Debug)]
struct Quantized {
    row: usize,
    displacement: u64,
}

fn quantize_tick(tick: u64, ppqn: u16, steps: u8) -> Result<Quantized> {
    let numerator = tick
        .checked_mul(u64::from(steps))
        .context("MIDI tick scaling overflow")?;
    let denominator = u64::from(ppqn);
    let row = (numerator + denominator / 2) / denominator;
    let represented_numerator = row
        .checked_mul(denominator)
        .context("MIDI represented tick overflow")?;
    let displacement_numerator = represented_numerator.abs_diff(numerator);
    Ok(Quantized {
        row: usize::try_from(row).context("MIDI row index overflow")?,
        displacement: displacement_numerator.div_ceil(u64::from(steps)),
    })
}

fn record_quantization(
    displacement: u64,
    exact: &mut usize,
    quantized: &mut usize,
    maximum: &mut u64,
) {
    if displacement == 0 {
        *exact += 1;
    } else {
        *quantized += 1;
        *maximum = (*maximum).max(displacement);
    }
}

fn locate_row(row: usize, spans: &[usize]) -> Option<(usize, usize)> {
    let mut origin = 0usize;
    for (pattern, rows) in spans.iter().copied().enumerate() {
        let end = origin.checked_add(rows)?;
        if row < end {
            return Some((pattern, row - origin));
        }
        origin = end;
    }
    None
}

fn row_to_tick(row: usize, ppqn: u16, steps: u8) -> Result<u64> {
    u64::try_from(row)
        .ok()
        .and_then(|row| row.checked_mul(u64::from(ppqn)))
        .map(|value| value / u64::from(steps))
        .context("MIDI Pattern tick origin overflow")
}

#[allow(clippy::too_many_arguments)]
fn place_tempo_commands(
    patterns: &mut BTreeMap<u16, Pattern>,
    spans: &[usize],
    tempos: &[TempoEvent],
    ppqn: u16,
    steps: u8,
    exact: &mut usize,
    quantized: &mut usize,
    maximum: &mut u64,
) -> Result<()> {
    for event in tempos.iter().filter(|event| event.tick > 0) {
        let position = quantize_tick(event.tick, ppqn, steps)?;
        record_quantization(position.displacement, exact, quantized, maximum);
        let Some((pattern_number, row)) = locate_row(position.row, spans) else {
            continue;
        };
        let pattern = patterns
            .get_mut(&(pattern_number as u16))
            .context("tempo command Pattern is missing")?;
        if row == 0 {
            pattern.tempo = event.tempo;
            continue;
        }
        let cell = pattern.rows[row]
            .iter_mut()
            .find(|cell| cell.command == Command::None)
            .context("tracker row has no cell available for a tempo command")?;
        cell.command = Command::Tempo(event.tempo);
    }
    Ok(())
}

fn safe_track_name(bytes: &[u8]) -> String {
    safe_page_name(&String::from_utf8_lossy(bytes))
}

fn safe_page_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .filter(|character| !character.is_control())
        .map(|character| if character == '|' { ' ' } else { character })
        .collect::<String>();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "MIDI".into()
    } else {
        cleaned.chars().take(48).collect()
    }
}

fn key_label(key: KeySignature) -> String {
    const MAJOR: [&str; 15] = [
        "Cb", "Gb", "Db", "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E", "B", "F#", "C#",
    ];
    const MINOR: [&str; 15] = [
        "Abm", "Ebm", "Bbm", "Fm", "Cm", "Gm", "Dm", "Am", "Em", "Bm", "F#m", "C#m", "G#m", "D#m",
        "A#m",
    ];
    let index = usize::try_from((i16::from(key.sharps_flats) + 7).clamp(0, 14)).unwrap_or(7);
    if key.minor {
        MINOR[index].into()
    } else {
        MAJOR[index].into()
    }
}

struct Cursor<'a> {
    remaining: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn remaining(&self) -> &'a [u8] {
        self.remaining
    }

    fn peek(&self) -> Option<u8> {
        self.remaining.first().copied()
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        if count > self.remaining.len() {
            bail!("truncated MIDI data");
        }
        let (taken, remaining) = self.remaining.split_at(count);
        self.remaining = remaining;
        Ok(taken)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn data_byte(&mut self) -> Result<u8> {
        let byte = self.byte()?;
        if byte & 0x80 != 0 {
            bail!("MIDI data byte has its status bit set");
        }
        Ok(byte)
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .context("truncated four-byte MIDI value")?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn vlq(&mut self) -> Result<u32> {
        let mut value = 0u32;
        for index in 0..4 {
            let byte = self.byte()?;
            value = value
                .checked_shl(7)
                .and_then(|value| value.checked_add(u32::from(byte & 0x7f)))
                .context("MIDI variable-length value overflow")?;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            if index == 3 {
                bail!("overlong MIDI variable-length value");
            }
        }
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(body: &[u8]) -> Vec<u8> {
        let mut bytes = b"MTrk".to_vec();
        bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
        bytes.extend_from_slice(body);
        bytes
    }

    fn smf(format: u16, tracks: &[Vec<u8>], division: u16) -> Vec<u8> {
        let mut bytes = b"MThd".to_vec();
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&format.to_be_bytes());
        bytes.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
        bytes.extend_from_slice(&division.to_be_bytes());
        for track in tracks {
            bytes.extend_from_slice(track);
        }
        bytes
    }

    #[test]
    fn parses_format_zero_running_status_and_velocity_zero_off() {
        let bytes = smf(
            0,
            &[track(&[
                0, 0x90, 60, 100, 0x10, 64, 90, 0x10, 60, 0, 0, 0xff, 0x2f, 0,
            ])],
            480,
        );
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.format, SmfFormat::Format0);
        assert_eq!(parsed.tracks[0].events.len(), 3);
        assert!(matches!(
            parsed.tracks[0].events[2].kind,
            MidiEventKind::NoteOff { note: 60 }
        ));
    }

    #[test]
    fn format_one_uses_120_default_and_decimal_tempo() {
        let conductor = track(&[0, 0xff, 0x51, 3, 0x09, 0x1c, 0x17, 0, 0xff, 0x2f, 0]);
        let notes = track(&[0, 0x90, 60, 100, 0x81, 0x70, 0x80, 60, 0, 0, 0xff, 0x2f, 0]);
        let parsed = parse(&smf(1, &[conductor, notes], 480)).unwrap();
        assert_eq!(parsed.tempos[0].tempo.to_string(), "100.50");

        let no_tempo = parse(&smf(0, &[track(&[0, 0xff, 0x2f, 0])], 480)).unwrap();
        assert_eq!(normalized_tempos(&no_tempo).unwrap()[0].tempo, Bpm::DEFAULT);
    }

    #[test]
    fn rejects_truncation_format_two_smpte_and_overlong_vlq() {
        assert!(parse(b"MThd").is_err());
        assert!(parse(&smf(0, &[track(&[0, 0x90, 60, 100])], 480))
            .unwrap_err()
            .to_string()
            .contains("end-of-track"));
        assert!(parse(&smf(2, &[track(&[0, 0xff, 0x2f, 0])], 480))
            .unwrap_err()
            .to_string()
            .contains("format 2"));
        assert!(parse(&smf(0, &[track(&[0, 0xff, 0x2f, 0])], 0xe728))
            .unwrap_err()
            .to_string()
            .contains("SMPTE"));
        assert!(parse(&smf(
            0,
            &[track(&[0x81, 0x80, 0x80, 0x80, 0, 0xff, 0x2f, 0])],
            480
        ))
        .is_err());
    }

    #[test]
    fn sustain_and_repeated_pitch_pair_fifo() {
        let bytes = smf(
            0,
            &[track(&[
                0, 0xb0, 64, 127, 0, 0x90, 60, 90, 10, 0x90, 60, 80, 10, 0x80, 60, 0, 10, 0x80, 60,
                0, 10, 0xb0, 64, 0, 0, 0xff, 0x2f, 0,
            ])],
            40,
        );
        let imported = convert(&parse(&bytes).unwrap(), "sustain").unwrap();
        assert_eq!(imported.report.note_ons, 2);
        assert_eq!(imported.report.sustained_notes, 2);
        assert_eq!(imported.report.unmatched_note_offs, 0);
        assert_eq!(imported.report.pages, 1);
    }

    #[test]
    fn fifth_simultaneous_voice_uses_a_deterministic_overflow_page() {
        let bytes = smf(
            0,
            &[track(&[
                0, 0x90, 60, 100, 0, 0x90, 61, 99, 0, 0x90, 62, 98, 0, 0x90, 63, 97, 0, 0x90, 64,
                96, 40, 0x80, 60, 0, 0, 0x80, 61, 0, 0, 0x80, 62, 0, 0, 0x80, 63, 0, 0, 0x80, 64,
                0, 0, 0xff, 0x2f, 0,
            ])],
            40,
        );
        let imported = convert(&parse(&bytes).unwrap(), "polyphony").unwrap();
        assert_eq!(imported.report.maximum_polyphony, 5);
        assert_eq!(imported.report.pages, 2);
        let first = imported.song.patterns.get(&0).unwrap();
        assert_eq!(first.rows[0][0].note, Note::On(60));
        assert_eq!(first.rows[0][4].note, Note::On(64));
    }

    #[test]
    fn quantized_grid_tempo_map_and_bar_boundary_split_are_reported() {
        let conductor = track(&[
            0, 0xff, 0x58, 4, 4, 2, 24, 8, 0, 0xff, 0x51, 3, 0x07, 0xa1, 0x20, 0x81, 0x00, 0xff,
            0x51, 3, 0x09, 0x27, 0xc0, 0, 0xff, 0x2f, 0,
        ]);
        let notes = track(&[1, 0x90, 60, 100, 0x92, 0x2f, 0x80, 60, 0, 0, 0xff, 0x2f, 0]);
        let imported = convert(&parse(&smf(1, &[conductor, notes], 17)).unwrap(), "long").unwrap();
        assert_eq!(imported.report.steps_per_beat, 16);
        assert!(imported.report.quantized_events > 0);
        assert!(imported.report.maximum_displacement_ticks > 0);
        assert!(imported.report.patterns > 1);
        assert_eq!(imported.report.tempos.len(), 2);
        assert!(imported
            .song
            .patterns
            .values()
            .any(|pattern| pattern.rows.len() % (4 * 16) == 0));
    }

    #[test]
    fn parser_enforces_the_file_size_bound_before_chunk_work() {
        let oversized = vec![0; MAX_MIDI_BYTES as usize + 1];
        assert!(parse(&oversized)
            .unwrap_err()
            .to_string()
            .contains("exceeds"));
    }

    #[test]
    fn six_eight_maps_to_compound_three_four() {
        let bytes = smf(
            0,
            &[track(&[
                0, 0xff, 0x58, 4, 6, 3, 24, 8, 0, 0x90, 60, 100, 0x83, 0x60, 0x80, 60, 0, 0, 0xff,
                0x2f, 0,
            ])],
            480,
        );
        let imported = convert(&parse(&bytes).unwrap(), "six-eight").unwrap();
        assert_eq!(imported.report.source_meter, (6, 8));
        assert_eq!(imported.report.project_meter, 3);
        assert!(imported.report.meter_mapping.is_some());
    }

    #[test]
    fn long_note_release_survives_a_pattern_boundary() {
        let notes = track(&[
            0x87, 0x7c, 0x90, 60, 100, // row 255
            8, 0x80, 60, 0, // row 257, in the next Pattern
            4, 0x90, 62, 90, 1, 0x80, 62, 0, 0, 0xff, 0x2f, 0,
        ]);
        let imported = convert(&parse(&smf(0, &[notes], 4)).unwrap(), "boundary").unwrap();
        assert_eq!(imported.report.patterns, 2);
        let attack = imported.song.patterns[&0].rows[255][0];
        let release = imported.song.patterns[&1].rows[1][0];
        assert_eq!(attack.note, Note::On(60));
        assert_eq!(attack.gate, Some(100));
        assert_eq!(release.note, Note::Off);

        let messages = crate::sequencer::schedule(
            &imported.song,
            &crate::config::RuntimeConfig::default().external_midi,
            0,
            0,
        )
        .unwrap();
        let release = messages
            .iter()
            .find(|message| message.bytes == [0x80, 60, 0])
            .unwrap();
        assert_eq!(release.at, std::time::Duration::from_millis(128_500));
    }

    #[test]
    fn changing_meter_is_refused_and_unmatched_notes_are_reported() {
        let conductor = track(&[
            0, 0xff, 0x58, 4, 4, 2, 24, 8, 4, 0xff, 0x58, 4, 3, 2, 24, 8, 0, 0xff, 0x2f, 0,
        ]);
        let notes = track(&[
            0, 0x80, 61, 0, // unmatched release
            0, 0x90, 60, 100, // hanging attack
            0, 0xff, 0x2f, 0,
        ]);
        let parsed = parse(&smf(1, &[conductor, notes.clone()], 4)).unwrap();
        assert!(convert(&parsed, "meter")
            .unwrap_err()
            .to_string()
            .contains("changing meter"));

        let imported = convert(&parse(&smf(0, &[notes], 4)).unwrap(), "notes").unwrap();
        assert_eq!(imported.report.unmatched_note_offs, 1);
        assert_eq!(imported.report.hanging_notes, 1);
    }

    #[test]
    fn unsupported_events_are_counted_instead_of_claimed_as_exact() {
        let bytes = smf(
            0,
            &[track(&[
                0, 0xff, 0x05, 1, b'x', // lyric
                0, 0xff, 0x59, 2, 0, 0, // key
                0, 0xff, 0x7f, 1, 1, // sequencer metadata
                0, 0xf0, 1, 0x7f, // SysEx
                0, 0xa0, 60, 1, // poly aftertouch
                0, 0xd0, 1, // channel aftertouch
                0, 0xe0, 0, 64, // pitch bend
                0, 0xb0, 1, 1, // unsupported CC
                0, 0xc0, 10, // initial program
                0, 0x90, 60, 100, 4, 0x80, 60, 0, // note
                0, 0xc0, 11, // later program
                0, 0xff, 0x2f, 0,
            ])],
            4,
        );
        let imported = convert(&parse(&bytes).unwrap(), "stripped").unwrap();
        assert_eq!(imported.report.stripped.text, 1);
        assert_eq!(imported.report.stripped.key_signatures, 1);
        assert_eq!(imported.report.stripped.sequencer_metadata, 1);
        assert_eq!(imported.report.stripped.sysex, 1);
        assert_eq!(imported.report.stripped.aftertouch, 2);
        assert_eq!(imported.report.stripped.pitch_bend, 1);
        assert_eq!(imported.report.stripped.unsupported_cc, 1);
        assert_eq!(imported.report.stripped.later_bank_program, 1);
        assert_eq!(imported.song.patterns[&0].pages[0].columns[0].program, 10);
    }

    #[cfg(unix)]
    #[test]
    fn path_import_refuses_symlinks() {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!("shr-midi-symlink-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let target = base.join("target.mid");
        fs::write(&target, smf(0, &[track(&[0, 0xff, 0x2f, 0])], 480)).unwrap();
        let link = base.join("link.mid");
        symlink(&target, &link).unwrap();
        assert!(import_path(&link).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn bundled_house_fixture_has_expected_musical_contract() {
        let parsed =
            parse(&read_regular_file(Path::new("demos/house-of-the-rising-sun.mid")).unwrap())
                .unwrap();
        assert_eq!(
            parsed
                .tracks
                .iter()
                .map(|track| track.name.as_deref())
                .collect::<Vec<_>>(),
            vec![
                Some("Conductor"),
                Some("Drums"),
                Some("Bass"),
                Some("Pad"),
                Some("Lead"),
                Some("Counter")
            ]
        );
        let mut paired_stripped = parsed.stripped.clone();
        let mut unmatched = 0;
        let mut hanging = 0;
        let mut sustained = 0;
        let parts = pair_parts(
            &parsed,
            &mut paired_stripped,
            &mut unmatched,
            &mut hanging,
            &mut sustained,
        )
        .unwrap();
        let mut expected_notes = parts
            .iter()
            .flat_map(|part| {
                part.notes
                    .iter()
                    .map(|note| (part.channel, note.note, note.velocity, note.start, note.end))
            })
            .collect::<Vec<_>>();
        expected_notes.sort_unstable();

        let imported = convert(&parsed, "house-of-the-rising-sun").unwrap();
        let report = &imported.report;
        assert_eq!(report.source_format, SmfFormat::Format1);
        assert_eq!(report.source_tracks, 6);
        assert_eq!(report.ppqn, 480);
        assert_eq!(
            report.tempos,
            vec![TempoEvent {
                tick: 0,
                tempo: "84".parse().unwrap()
            }]
        );
        assert_eq!(report.source_meter, (6, 8));
        assert_eq!(report.project_meter, 3);
        assert_eq!(report.rows, 160);
        assert_eq!(report.steps_per_beat, 4);
        assert_eq!(report.note_ons, 254);
        assert_eq!(report.maximum_polyphony, 3);
        assert_eq!(report.quantized_events, 0);
        assert_eq!(report.pages, 5);
        assert_eq!(report.stripped.unsupported_cc, 0);
        assert_eq!(report.stripped.sysex, 0);
        assert_eq!(report.stripped.aftertouch, 0);
        assert_eq!(report.stripped.pitch_bend, 0);
        assert_eq!(report.stripped.realtime, 0);
        assert_eq!(report.stripped.system_common, 0);
        assert_eq!(report.stripped.later_bank_program, 0);
        assert_eq!(report.sustained_notes, 0);
        assert_eq!(report.unmatched_note_offs, 0);
        assert_eq!(report.hanging_notes, 0);
        let first = imported.song.patterns.get(&0).unwrap();
        let channels = first
            .pages
            .iter()
            .map(|page| page.columns[0].channel)
            .collect::<Vec<_>>();
        let programs = first
            .pages
            .iter()
            .map(|page| page.columns[0].program)
            .collect::<Vec<_>>();
        assert_eq!(channels, vec![9, 3, 2, 0, 1]);
        assert_eq!(programs, vec![0, 32, 88, 40, 10]);
        assert!(first.pages[0].percussion);

        let scheduled = crate::sequencer::schedule(
            &imported.song,
            &crate::config::RuntimeConfig::default().external_midi,
            0,
            0,
        )
        .unwrap();
        let mut active: BTreeMap<usize, (u8, u8, u8, u64)> = BTreeMap::new();
        let mut actual_notes = Vec::new();
        for message in scheduled {
            if message.bytes.len() != 3 {
                continue;
            }
            let status = message.bytes[0];
            let channel = status & 0x0f;
            let note = message.bytes[1];
            let tick = (message.at.as_secs_f64() * 84.0 / 60.0 * 480.0).round() as u64;
            let Some(lane) = message.lane else {
                continue;
            };
            match status & 0xf0 {
                0x90 if message.bytes[2] > 0 => {
                    active.insert(lane, (channel, note, message.bytes[2], tick));
                }
                0x80 | 0x90 => {
                    // The scheduler may emit a harmless repeated lane-cleanup
                    // OFF after an earlier gate release. Only the first OFF
                    // owns the corresponding imported attack.
                    if let Some(started) = active.remove(&lane) {
                        assert_eq!((channel, note), (started.0, started.1));
                        actual_notes.push((channel, note, started.2, started.3, tick));
                    }
                }
                _ => {}
            }
        }
        assert!(active.is_empty());
        actual_notes.sort_unstable();
        assert_eq!(
            actual_notes, expected_notes,
            "every imported channel, pitch, velocity, start, and duration must survive"
        );
    }
}
