//! Bounded Standard MIDI File format-1 Arrangement export from the canonical
//! musical timeline. Audio loops and SHR-only effect automation are reported,
//! never represented as portable MIDI.

use crate::config::ExternalMidiConfig;
use crate::sequencer::{AutomationTarget, PageTarget, Song, LANES_PER_PAGE};
use crate::timeline::{self, TimelinePlan};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MAX_EXPORT_EVENTS: usize = 1_048_576;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExportReport {
    pub tracks: usize,
    pub events: usize,
    pub ppqn: u16,
    pub tempo_changes: usize,
    pub meter_changes: usize,
    pub omitted_loop_slots: usize,
    pub omitted_effect_lanes: usize,
    pub omitted_setup_messages: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyzedExport {
    pub bytes: Vec<u8>,
    pub report: ExportReport,
    pub suggested_name: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PartKey {
    page: usize,
    channel: u8,
    name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Event {
    tick: u64,
    priority: u8,
    ordinal: u32,
    bytes: Vec<u8>,
}

pub fn analyze(song: &Song, config: &ExternalMidiConfig) -> Result<AnalyzedExport> {
    let plan = timeline::compile(song, config, 0, 0)?;
    let mut report = ExportReport {
        ppqn: plan.ppqn,
        tempo_changes: plan.tempos.len(),
        meter_changes: plan.meters.len(),
        omitted_loop_slots: song
            .order
            .iter()
            .filter_map(|number| song.patterns.get(number))
            .flat_map(|pattern| pattern.audio_loops.iter())
            .filter(|slot| slot.is_some())
            .count(),
        ..ExportReport::default()
    };
    let mut tracks = BTreeMap::<PartKey, Vec<Event>>::new();
    for event in &plan.midi {
        if event.bytes.is_empty() || event.bytes[0] >= 0xf0 {
            if !event.bytes.is_empty() && event.bytes[0] != 0xf8 {
                report.omitted_setup_messages += 1;
            }
            continue;
        }
        let channel = event.bytes[0] & 0x0f;
        let key = part_key(
            song,
            event.order,
            event.lane,
            event.target.as_ref(),
            channel,
        );
        tracks.entry(key).or_default().push(Event {
            tick: event.tick,
            priority: timeline::event_priority(&event.bytes),
            ordinal: event.ordinal,
            bytes: event.bytes.clone(),
        });
    }
    report.omitted_effect_lanes = plan
        .automation
        .iter()
        .filter(|segment| {
            matches!(
                segment.target,
                AutomationTarget::Effect { .. } | AutomationTarget::EffectBypass { .. }
            )
        })
        .map(|segment| (segment.pattern, segment.lane_id))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if tracks.len() + 1 > u16::MAX as usize {
        bail!("MIDI export has too many tracks");
    }
    let mut chunks = Vec::with_capacity(tracks.len() + 1);
    chunks.push(conductor_track(&plan));
    for (key, mut events) in tracks {
        events.sort_by_key(|event| (event.tick, event.priority, event.ordinal));
        chunks.push(part_track(&key, &events, plan.end_tick)?);
        report.events = report
            .events
            .checked_add(events.len())
            .context("MIDI export event count overflow")?;
    }
    if report.events > MAX_EXPORT_EVENTS {
        bail!("MIDI export exceeds {MAX_EXPORT_EVENTS} events");
    }
    report.tracks = chunks.len();
    let mut bytes = b"MThd".to_vec();
    bytes.extend_from_slice(&6u32.to_be_bytes());
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&(chunks.len() as u16).to_be_bytes());
    bytes.extend_from_slice(&plan.ppqn.to_be_bytes());
    for track in chunks {
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
        bytes.extend(track);
    }
    Ok(AnalyzedExport {
        bytes,
        report,
        suggested_name: format!("{}.mid", crate::sequencer::safe_name(&song.name)),
    })
}

fn part_key(
    song: &Song,
    order: usize,
    lane: Option<usize>,
    target: Option<&PageTarget>,
    channel: u8,
) -> PartKey {
    let pattern = song
        .order
        .get(order)
        .and_then(|number| song.patterns.get(number));
    let page = lane
        .map(|lane| lane / LANES_PER_PAGE)
        .or_else(|| {
            pattern?.pages.iter().position(|page| {
                Some(&page.target) == target
                    && page.columns.iter().any(|column| column.channel == channel)
            })
        })
        .unwrap_or(0);
    let name = pattern
        .and_then(|pattern| pattern.pages.get(page))
        .map_or_else(|| format!("Page {}", page + 1), |page| page.name.clone());
    PartKey {
        page,
        channel,
        name,
    }
}

fn conductor_track(plan: &TimelinePlan) -> Vec<u8> {
    let mut events = Vec::<Event>::new();
    let mut ordinal = 0u32;
    for tempo in &plan.tempos {
        let micros = ((60_000_000u64 * 100) / u64::from(tempo.tempo.hundredths()))
            .clamp(1, 0x00ff_ffff) as u32;
        events.push(Event {
            tick: tempo.tick,
            priority: 0,
            ordinal,
            bytes: vec![
                0xff,
                0x51,
                0x03,
                (micros >> 16) as u8,
                (micros >> 8) as u8,
                micros as u8,
            ],
        });
        ordinal += 1;
    }
    for meter in &plan.meters {
        events.push(Event {
            tick: meter.tick,
            priority: 1,
            ordinal,
            bytes: vec![
                0xff,
                0x58,
                0x04,
                meter.numerator,
                meter.denominator.trailing_zeros() as u8,
                24,
                8,
            ],
        });
        ordinal += 1;
    }
    events.sort_by_key(|event| (event.tick, event.priority, event.ordinal));
    encode_track("Conductor", &events, plan.end_tick)
}

fn part_track(key: &PartKey, events: &[Event], end_tick: u64) -> Result<Vec<u8>> {
    let name = format!("{} · ch {}", key.name, key.channel + 1);
    Ok(encode_track(&name, events, end_tick))
}

fn encode_track(name: &str, events: &[Event], end_tick: u64) -> Vec<u8> {
    let mut out = Vec::new();
    vlq(0, &mut out);
    out.extend_from_slice(&[0xff, 0x03]);
    let name = name.as_bytes();
    vlq(name.len().min(127) as u64, &mut out);
    out.extend_from_slice(&name[..name.len().min(127)]);
    let mut previous = 0u64;
    for event in events {
        write_delta(event.tick.saturating_sub(previous), &mut out);
        out.extend_from_slice(&event.bytes);
        previous = event.tick;
    }
    write_delta(end_tick.saturating_sub(previous), &mut out);
    out.extend_from_slice(&[0xff, 0x2f, 0]);
    out
}

fn write_delta(mut value: u64, out: &mut Vec<u8>) {
    const MAX_VLQ: u64 = 0x0fff_ffff;
    while value > MAX_VLQ {
        vlq(MAX_VLQ, out);
        out.extend_from_slice(&[0xff, 0x7f, 0]);
        value -= MAX_VLQ;
    }
    vlq(value, out);
}

fn vlq(mut value: u64, out: &mut Vec<u8>) {
    let mut bytes = [0u8; 4];
    let mut index = 3;
    bytes[index] = (value & 0x7f) as u8;
    while {
        value >>= 7;
        value != 0
    } {
        index -= 1;
        bytes[index] = ((value & 0x7f) as u8) | 0x80;
    }
    out.extend_from_slice(&bytes[index..]);
}

pub fn exports_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()))
                .join(".local/share")
        })
        .join("shsynth/exports")
}

pub fn next_path(base: &Path, suggested_name: &str) -> Result<PathBuf> {
    let stem = Path::new(suggested_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(crate::sequencer::safe_name)
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "arrangement".into());
    for suffix in 0..=9_999 {
        let name = if suffix == 0 {
            format!("{stem}.mid")
        } else {
            format!("{stem}-{suffix:03}.mid")
        };
        let path = base.join(name);
        if !path.exists() {
            return Ok(path);
        }
    }
    bail!("MIDI export names are exhausted")
}

pub fn save(base: &Path, analyzed: &AnalyzedExport) -> Result<PathBuf> {
    std::fs::create_dir_all(base)?;
    let path = next_path(base, &analyzed.suggested_name)?;
    crate::fsutil::atomic_write_noreplace(&path, &analyzed.bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_song() -> (Song, ExternalMidiConfig) {
        let mut config = crate::config::RuntimeConfig::default().external_midi;
        config.bank_select = crate::config::BankSelectMode::Cc0Cc32;
        config.program_changes = true;
        config.send_transport = false;
        let mut page = crate::sequencer::Page::new("BASS", 2, false, 9);
        page.target = PageTarget::Midi("DIN".into());
        for column in &mut page.columns {
            column.bank_msb = 3;
            column.bank_lsb = 4;
            column.program = 9;
        }
        let mut song = Song::new_with_pages(&config, vec![page]);
        let pattern = song.patterns.get_mut(&0).unwrap();
        pattern.rows.truncate(8);
        pattern.rows[0][0] = crate::sequencer::Cell {
            note: crate::sequencer::Note::On(36),
            velocity: Some(111),
            gate: Some(50),
            ..crate::sequencer::Cell::default()
        };
        pattern.rows[3][0].command =
            crate::sequencer::Command::Tempo(crate::tempo::Bpm::from_hundredths(9_000).unwrap());
        pattern.automation.push(crate::sequencer::AutomationLane {
            id: 1,
            target: AutomationTarget::MidiCc {
                page: 0,
                channel: 2,
                controller: 74,
            },
            curve: crate::sequencer::AutomationCurve::Linear,
            points: vec![
                crate::sequencer::AutomationPoint { tick: 0, value: 0 },
                crate::sequencer::AutomationPoint {
                    tick: crate::sequencer::AUTOMATION_TICKS_PER_ROW * 4,
                    value: u16::MAX,
                },
            ],
        });
        (song, config)
    }

    #[test]
    fn collision_free_export_path_never_overwrites() {
        let base = std::env::temp_dir().join(format!("shr-midi-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("song.mid"), b"old").unwrap();
        assert_eq!(
            next_path(&base, "song.mid").unwrap(),
            base.join("song-001.mid")
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn arrangement_export_is_format_one_with_conductor_named_part_and_cc() {
        let (song, config) = configured_song();
        let analyzed = analyze(&song, &config).unwrap();
        assert_eq!(&analyzed.bytes[..10], b"MThd\0\0\0\x06\0\x01");
        assert_eq!(
            u16::from_be_bytes([analyzed.bytes[10], analyzed.bytes[11]]),
            2
        );
        assert_eq!(analyzed.report.tracks, 2);
        assert!(analyzed.report.tempo_changes >= 2);
        assert_eq!(analyzed.report.meter_changes, 1);
        assert!(analyzed
            .bytes
            .windows(9)
            .any(|window| window == b"Conductor"));
        assert!(analyzed
            .bytes
            .windows(4)
            .any(|window| window == [0xff, 0x51, 3, 0x07]));
        assert!(analyzed
            .bytes
            .windows(4)
            .any(|window| window == [0xff, 0x58, 4, 4]));
        assert!(analyzed.bytes.windows(4).any(|window| window == b"BASS"));
        assert!(analyzed
            .bytes
            .windows(3)
            .any(|window| window == [0xb2, 74, 127]));

        let bank_msb = analyzed
            .bytes
            .windows(3)
            .position(|window| window == [0xb2, 0, 3])
            .unwrap();
        let bank_lsb = analyzed
            .bytes
            .windows(3)
            .position(|window| window == [0xb2, 32, 4])
            .unwrap();
        let program = analyzed
            .bytes
            .windows(2)
            .position(|window| window == [0xc2, 9])
            .unwrap();
        let note = analyzed
            .bytes
            .windows(3)
            .position(|window| window == [0x92, 36, 111])
            .unwrap();
        assert!(bank_msb < bank_lsb && bank_lsb < program && program < note);
    }

    #[test]
    fn canonical_ticks_preserve_exact_gate_and_changed_cc_values() {
        let (song, config) = configured_song();
        let plan = timeline::compile(&song, &config, 0, 0).unwrap();
        let on = plan
            .midi
            .iter()
            .find(|event| event.bytes == [0x92, 36, 111])
            .unwrap();
        let off = plan
            .midi
            .iter()
            .find(|event| {
                event
                    .bytes
                    .first()
                    .is_some_and(|status| status & 0xf0 == 0x80)
            })
            .unwrap();
        assert_eq!(
            off.tick - on.tick,
            u64::from(crate::sequencer::AUTOMATION_TICKS_PER_ROW) / 2
        );
        let cc = plan
            .midi
            .iter()
            .filter(|event| event.automation && event.bytes.get(1) == Some(&74))
            .map(|event| event.bytes[2])
            .collect::<Vec<_>>();
        assert_eq!(cc.first(), Some(&0));
        assert!(cc.contains(&127));
        assert_eq!(cc.last(), Some(&0));
        assert!(cc.windows(2).all(|values| values[0] != values[1]));
    }
}
