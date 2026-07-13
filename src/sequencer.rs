//! Hardware MIDI accompaniment sequencing. Song editing/storage and event
//! planning are independent from the owned software-synth lifecycle.
use crate::config::{BankSelectMode, ExternalMidiConfig};
use anyhow::{anyhow, bail, Context, Result};
use midir::{MidiOutput, MidiOutputConnection};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const SONG_VERSION: u8 = 2;
pub const LANES_PER_PAGE: usize = 4;
pub const PAGE_COUNT: usize = 2;
pub const TOTAL_LANES: usize = LANES_PER_PAGE * PAGE_COUNT;
#[cfg(test)]
const DEFAULT_GESTURE_SETTLE: Duration = Duration::from_millis(45);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Song {
    pub name: String,
    pub tempo: u16,
    pub steps_per_beat: u8,
    pub gate_percent: u8,
    pub order: Vec<u16>,
    pub pages: Vec<Page>,
    pub patterns: BTreeMap<u16, Pattern>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page {
    pub name: String,
    pub enabled: bool,
    pub channel: u8,
    pub bank_msb: u8,
    pub bank_lsb: u8,
    pub program: u8,
    pub velocity: u8,
    pub percussion: bool,
    pub lanes: Vec<Lane>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lane {
    pub name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pattern {
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Cell {
    pub note: Note,
    pub velocity: Option<u8>,
    pub program: Option<u8>,
    pub command: Command,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Note {
    #[default]
    Empty,
    On(u8),
    Off,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Command {
    #[default]
    None,
    Cut(u8),
    Delay(u8),
    Retrigger(u8),
    Tempo(u16),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GestureCommit {
    pub notes: Vec<(u8, u8)>,
    pub overflowed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct GestureCapture {
    held: BTreeMap<u8, u16>,
    collected: BTreeMap<u8, u8>,
    released_at: Option<Instant>,
    overflowed: bool,
}

impl GestureCapture {
    pub fn observe(&mut self, now: Instant, message: &[u8]) {
        if message.len() < 3 {
            return;
        }
        let kind = message[0] & 0xf0;
        let note = message[1];
        if kind == 0x90 && message[2] > 0 {
            *self.held.entry(note).or_default() += 1;
            if self.collected.len() < LANES_PER_PAGE || self.collected.contains_key(&note) {
                self.collected.entry(note).or_insert(message[2]);
            } else {
                self.overflowed = true;
            }
            self.released_at = None;
        } else if kind == 0x80 || (kind == 0x90 && message[2] == 0) {
            if let Some(count) = self.held.get_mut(&note) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.held.remove(&note);
                }
            }
            if self.held.is_empty() && !self.collected.is_empty() {
                self.released_at = Some(now);
            }
        }
    }

    pub fn finish(&mut self, now: Instant, settle: Duration) -> Option<GestureCommit> {
        let ready = self.held.is_empty()
            && self
                .released_at
                .is_some_and(|released| now.saturating_duration_since(released) >= settle);
        ready.then(|| {
            let commit = GestureCommit {
                notes: std::mem::take(&mut self.collected).into_iter().collect(),
                overflowed: std::mem::take(&mut self.overflowed),
            };
            self.released_at = None;
            commit
        })
    }

    pub fn cancel(&mut self) {
        self.held.clear();
        self.collected.clear();
        self.released_at = None;
        self.overflowed = false;
    }

    pub fn is_active(&self) -> bool {
        !self.collected.is_empty()
    }
}

impl Song {
    pub fn new(config: &ExternalMidiConfig) -> Self {
        let melody_channel = config.melody_channel;
        let drum_channel = config.percussion_channel.unwrap_or(1);
        let pages = vec![
            Page::new("MELODY", melody_channel, false, 0),
            Page::new(
                "DRUMS",
                drum_channel,
                true,
                config.percussion_program.unwrap_or(0),
            ),
        ];
        let mut patterns = BTreeMap::new();
        patterns.insert(0, Pattern::empty(config.default_pattern_rows, TOTAL_LANES));
        Self {
            name: "untitled".into(),
            tempo: config.default_tempo,
            steps_per_beat: config.steps_per_beat,
            gate_percent: config.gate_percent,
            order: vec![0],
            pages,
            patterns,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if !(20..=300).contains(&self.tempo) || !(1..=16).contains(&self.steps_per_beat) {
            bail!("song tempo/steps out of range");
        }
        if self.order.is_empty() || self.pages.len() != PAGE_COUNT {
            bail!("song needs an order and exactly two pages");
        }
        if self
            .pages
            .iter()
            .any(|page| page.lanes.len() != LANES_PER_PAGE)
        {
            bail!("each song page needs exactly four lanes");
        }
        if self
            .order
            .iter()
            .any(|number| !self.patterns.contains_key(number))
        {
            bail!("order references a missing pattern");
        }
        for pattern in self.patterns.values() {
            if pattern.rows.is_empty() || pattern.rows.len() > 256 {
                bail!("pattern must have 1..=256 rows");
            }
            if pattern.rows.iter().any(|row| row.len() != TOTAL_LANES) {
                bail!("pattern track count mismatch");
            }
        }
        Ok(())
    }
}

impl Page {
    fn new(name: &str, channel: u8, percussion: bool, program: u8) -> Self {
        Self {
            name: name.into(),
            enabled: true,
            channel,
            bank_msb: 0,
            bank_lsb: 0,
            program,
            velocity: 96,
            percussion,
            lanes: (1..=LANES_PER_PAGE)
                .map(|lane| Lane {
                    name: format!("L{lane}"),
                    enabled: true,
                })
                .collect(),
        }
    }
}

impl Pattern {
    pub fn empty(rows: usize, tracks: usize) -> Self {
        Self {
            rows: vec![vec![Cell::default(); tracks]; rows],
        }
    }
}

pub fn songs_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_else(|| ".".into())).join(".local/share")
        })
        .join("shsynth/songs")
}

pub fn safe_name(input: &str) -> String {
    let name = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if name.is_empty() {
        "untitled".into()
    } else {
        name.chars().take(64).collect()
    }
}

pub fn list(base: &Path) -> Vec<String> {
    let mut names = fs::read_dir(base)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Versioned line format. Unknown keys are retained only on disk: unsupported
/// or newer versions are refused, so they can never be destructively rewritten.
pub fn encode(song: &Song) -> Result<String> {
    song.validate()?;
    let mut out = format!(
        "SHSYNTH-SONG {SONG_VERSION}\nname={}\ntempo={}\nsteps={}\ngate={}\norder={}\n",
        escape(&song.name),
        song.tempo,
        song.steps_per_beat,
        song.gate_percent,
        song.order
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    for (page_index, page) in song.pages.iter().enumerate() {
        out.push_str(&format!(
            "page={page_index}|{}|{}|{}|{}|{}|{}|{}|{}\n",
            escape(&page.name),
            u8::from(page.enabled),
            page.channel + 1,
            page.bank_msb,
            page.bank_lsb,
            page.program,
            page.velocity,
            u8::from(page.percussion)
        ));
        for (lane_index, lane) in page.lanes.iter().enumerate() {
            out.push_str(&format!(
                "lane={page_index}|{lane_index}|{}|{}\n",
                escape(&lane.name),
                u8::from(lane.enabled)
            ));
        }
    }
    for (number, pattern) in &song.patterns {
        out.push_str(&format!("pattern={number}|{}\n", pattern.rows.len()));
        for (row_index, row) in pattern.rows.iter().enumerate() {
            for (track_index, cell) in row
                .iter()
                .enumerate()
                .filter(|(_, c)| **c != Cell::default())
            {
                out.push_str(&format!(
                    "cell={number}|{row_index}|{track_index}|{}|{}|{}|{}\n",
                    note_text(cell.note),
                    cell.velocity.map_or("-".into(), |v| v.to_string()),
                    cell.program.map_or("-".into(), |v| v.to_string()),
                    command_text(cell.command)
                ));
            }
        }
    }
    Ok(out)
}

pub fn decode(text: &str) -> Result<Song> {
    let mut lines = text.lines();
    let header = lines.next().context("empty song")?;
    let version = header
        .strip_prefix("SHSYNTH-SONG ")
        .context("not an SHSynth song")?
        .parse::<u8>()?;
    if version != 1 && version != SONG_VERSION {
        bail!("unsupported song version {version}; file was not changed");
    }
    let mut name = None;
    let mut tempo = None;
    let mut steps = None;
    let mut gate = Some(80);
    let mut order = None;
    let mut pages = BTreeMap::new();
    let mut lanes = Vec::new();
    let mut legacy_tracks = BTreeMap::new();
    let mut patterns: BTreeMap<u16, Pattern> = BTreeMap::new();
    let mut cells = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty() && !line.starts_with('#')) {
        let (key, value) = line.split_once('=').context("invalid song line")?;
        match key {
            "name" => name = Some(unescape(value)?),
            "tempo" => tempo = Some(value.parse()?),
            "steps" => steps = Some(value.parse()?),
            "gate" => gate = Some(value.parse()?),
            "order" => {
                order = Some(
                    value
                        .split(',')
                        .map(str::parse)
                        .collect::<std::result::Result<Vec<u16>, _>>()?,
                )
            }
            "track" if version == 1 => {
                let f = value.split('|').collect::<Vec<_>>();
                if f.len() != 9 {
                    bail!("invalid track");
                }
                legacy_tracks.insert(
                    f[0].parse::<usize>()?,
                    Page {
                        name: unescape(f[1])?,
                        enabled: f[2] == "1",
                        channel: one_based_channel(f[3])?,
                        bank_msb: midi_value(f[4])?,
                        bank_lsb: midi_value(f[5])?,
                        program: midi_value(f[6])?,
                        velocity: midi_value(f[7])?,
                        percussion: f[8] == "1",
                        lanes: Vec::new(),
                    },
                );
            }
            "page" if version == SONG_VERSION => {
                let f = value.split('|').collect::<Vec<_>>();
                if f.len() != 9 {
                    bail!("invalid page");
                }
                pages.insert(
                    f[0].parse::<usize>()?,
                    Page {
                        name: unescape(f[1])?,
                        enabled: f[2] == "1",
                        channel: one_based_channel(f[3])?,
                        bank_msb: midi_value(f[4])?,
                        bank_lsb: midi_value(f[5])?,
                        program: midi_value(f[6])?,
                        velocity: midi_value(f[7])?,
                        percussion: f[8] == "1",
                        lanes: Vec::new(),
                    },
                );
            }
            "lane" if version == SONG_VERSION => lanes.push(value.to_owned()),
            "pattern" => {
                let (number, rows) = value.split_once('|').context("invalid pattern")?;
                patterns.insert(number.parse()?, Pattern::empty(rows.parse()?, 0));
            }
            "cell" => cells.push(value.to_owned()),
            _ => bail!("unknown song field {key}; file was not changed"),
        }
    }
    if version == 1 && !legacy_tracks.keys().copied().eq(0..legacy_tracks.len()) {
        bail!("legacy tracks must be contiguous");
    }
    if version == SONG_VERSION && !pages.keys().copied().eq(0..PAGE_COUNT) {
        bail!("pages must be numbered 0 and 1");
    }
    let (mut pages, legacy_lane_map) = if version == 1 {
        convert_legacy_pages(&legacy_tracks)
    } else {
        (pages.into_values().collect::<Vec<_>>(), Vec::new())
    };
    if version == SONG_VERSION {
        for value in lanes {
            let f = value.split('|').collect::<Vec<_>>();
            if f.len() != 4 {
                bail!("invalid lane");
            }
            let page = pages
                .get_mut(f[0].parse::<usize>()?)
                .context("lane page missing")?;
            let index = f[1].parse::<usize>()?;
            if index != page.lanes.len() {
                bail!("lanes must be contiguous");
            }
            page.lanes.push(Lane {
                name: unescape(f[2])?,
                enabled: f[3] == "1",
            });
        }
    }
    for pattern in patterns.values_mut() {
        for row in &mut pattern.rows {
            row.resize(TOTAL_LANES, Cell::default());
        }
    }
    for value in cells {
        let f = value.split('|').collect::<Vec<_>>();
        if f.len() != 7 {
            bail!("invalid cell");
        }
        let pattern = patterns
            .get_mut(&f[0].parse()?)
            .context("cell pattern missing")?;
        let row_index = f[1].parse::<usize>()?;
        let source_index = f[2].parse::<usize>()?;
        let track_index = if version == 1 {
            *legacy_lane_map
                .get(source_index)
                .context("legacy cell track missing")?
        } else {
            source_index
        };
        let cell = pattern
            .rows
            .get_mut(row_index)
            .and_then(|r| r.get_mut(track_index))
            .context("cell outside pattern")?;
        *cell = Cell {
            note: parse_note(f[3])?,
            velocity: optional_midi(f[4])?,
            program: optional_midi(f[5])?,
            command: parse_command(f[6])?,
        };
    }
    let song = Song {
        name: name.context("missing name")?,
        tempo: tempo.context("missing tempo")?,
        steps_per_beat: steps.context("missing steps")?,
        gate_percent: gate.unwrap_or(80),
        order: order.context("missing order")?,
        pages,
        patterns,
    };
    song.validate()?;
    Ok(song)
}

fn convert_legacy_pages(tracks: &BTreeMap<usize, Page>) -> (Vec<Page>, Vec<usize>) {
    let mut melody = Page::new("MELODY", 0, false, 0);
    let mut drums = Page::new("DRUMS", 1, true, 9);
    let mut melody_lane = 0;
    let mut drum_lane = 0;
    let mut map = Vec::new();
    for track in tracks.values() {
        let (page, lane, offset) = if track.percussion {
            let lane = drum_lane.min(LANES_PER_PAGE - 1);
            drum_lane += 1;
            (&mut drums, lane, LANES_PER_PAGE)
        } else {
            let lane = melody_lane.min(LANES_PER_PAGE - 1);
            melody_lane += 1;
            (&mut melody, lane, 0)
        };
        if lane == 0 {
            page.program = track.program;
            page.bank_msb = track.bank_msb;
            page.bank_lsb = track.bank_lsb;
            page.velocity = track.velocity;
        }
        page.lanes[lane].name = track.name.clone();
        page.lanes[lane].enabled = track.enabled;
        map.push(offset + lane);
    }
    (vec![melody, drums], map)
}

pub fn save(base: &Path, song: &Song, overwrite: bool) -> Result<PathBuf> {
    fs::create_dir_all(base)?;
    let path = base.join(format!("{}.shsong", safe_name(&song.name)));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    if overwrite {
        options.create_new(false).create(true).truncate(true);
    }
    if path.exists() && !overwrite {
        bail!("song already exists; confirm overwrite explicitly");
    }
    if path.exists() && overwrite {
        let existing = fs::read_to_string(&path)?;
        let supported = existing
            .lines()
            .next()
            .and_then(|header| header.strip_prefix("SHSYNTH-SONG "))
            .and_then(|version| version.parse::<u8>().ok())
            .is_some_and(|version| version == 1 || version == SONG_VERSION);
        if !supported {
            bail!("refusing to overwrite unsupported/newer song file");
        }
    }
    let tmp = base.join(format!(
        ".{}.{}.tmp",
        safe_name(&song.name),
        std::process::id()
    ));
    if tmp.exists() {
        fs::remove_file(&tmp)?;
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    file.write_all(encode(song)?.as_bytes())?;
    file.sync_all()?;
    if path.exists() && !overwrite {
        let _ = fs::remove_file(&tmp);
        bail!("song already exists");
    }
    if overwrite {
        fs::rename(&tmp, &path)?;
    } else {
        rename_noreplace(&tmp, &path)?;
    }
    Ok(path)
}

fn rename_noreplace(from: &Path, to: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("publish song without replacement");
    }
    Ok(())
}

pub fn load(base: &Path, name: &str) -> Result<Song> {
    decode(&fs::read_to_string(
        base.join(format!("{}.shsong", safe_name(name))),
    )?)
}

pub fn delete(base: &Path, name: &str) -> Result<()> {
    let path = base.join(format!("{}.shsong", safe_name(name)));
    decode(&fs::read_to_string(&path)?)?;
    fs::remove_file(path)?;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledMessage {
    pub at: Duration,
    /// Empty for an internal transport-row marker. Row markers advance the
    /// UI and preserve the full pattern duration, but are never transmitted.
    pub bytes: Vec<u8>,
    pub order: usize,
    pub row: usize,
    pub lane: Option<usize>,
}

pub fn schedule(
    song: &Song,
    config: &ExternalMidiConfig,
    start_order: usize,
    start_row: usize,
) -> Result<Vec<ScheduledMessage>> {
    song.validate()?;
    for page in &song.pages {
        if !config.channels.contains(&page.channel) {
            bail!(
                "song uses MIDI channel {} outside the capability profile",
                page.channel + 1
            );
        }
        if page.percussion && config.percussion_channel != Some(page.channel) {
            bail!("song drums page is not on the configured percussion channel");
        }
    }
    let mut result = Vec::new();
    let mut at = Duration::ZERO;
    let mut tempo = song.tempo;
    let mut active: Vec<Option<u8>> = vec![None; TOTAL_LANES];
    let mut programmed = [false; PAGE_COUNT];
    for (order_index, pattern_number) in song.order.iter().enumerate().skip(start_order) {
        let pattern = &song.patterns[pattern_number];
        let first_row = if order_index == start_order {
            start_row.min(pattern.rows.len())
        } else {
            0
        };
        for (row_index, row) in pattern.rows.iter().enumerate().skip(first_row) {
            let row_duration =
                Duration::from_secs_f64(60.0 / f64::from(tempo) / f64::from(song.steps_per_beat));
            // A row is part of the transport even when it contains no MIDI.
            // Keep this marker ahead of messages at the same instant so the
            // play cursor moves before that row's notes are sent.
            push(&mut result, at, order_index, row_index, Vec::new());
            if config.send_transport {
                let clocks = (24 / usize::from(song.steps_per_beat)).max(1);
                for clock in 0..clocks {
                    push(
                        &mut result,
                        at + row_duration.mul_f64(clock as f64 / clocks as f64),
                        order_index,
                        row_index,
                        vec![0xf8],
                    );
                }
            }
            for (lane_index, cell) in row.iter().enumerate() {
                let page_index = lane_index / LANES_PER_PAGE;
                let page = &song.pages[page_index];
                let lane = &page.lanes[lane_index % LANES_PER_PAGE];
                if !page.enabled || !lane.enabled {
                    continue;
                }
                if let Command::Tempo(new_tempo) = cell.command {
                    tempo = new_tempo.clamp(20, 300);
                }
                let delay = match cell.command {
                    Command::Delay(tick) => row_duration.mul_f64(f64::from(tick.min(15)) / 16.0),
                    _ => Duration::ZERO,
                };
                let event_at = at + delay;
                match cell.note {
                    Note::On(note) => {
                        if cell.program.is_some() || !programmed[page_index] {
                            append_program(
                                &mut result,
                                event_at,
                                order_index,
                                row_index,
                                page,
                                cell.program.unwrap_or(page.program),
                                config,
                            );
                            programmed[page_index] = true;
                        }
                        if let Some(old) = active[lane_index].take() {
                            push_lane(
                                &mut result,
                                event_at,
                                order_index,
                                row_index,
                                vec![0x80 | page.channel, old, 0],
                                lane_index,
                            );
                        }
                        push_lane(
                            &mut result,
                            event_at,
                            order_index,
                            row_index,
                            vec![
                                0x90 | page.channel,
                                note,
                                cell.velocity.unwrap_or(page.velocity),
                            ],
                            lane_index,
                        );
                        active[lane_index] = Some(note);
                        let gate = row_duration.mul_f64(f64::from(song.gate_percent) / 100.0);
                        push_lane(
                            &mut result,
                            event_at + gate,
                            order_index,
                            row_index,
                            vec![0x80 | page.channel, note, 0],
                            lane_index,
                        );
                    }
                    Note::Off => {
                        if let Some(note) = active[lane_index].take() {
                            push_lane(
                                &mut result,
                                event_at,
                                order_index,
                                row_index,
                                vec![0x80 | page.channel, note, 0],
                                lane_index,
                            );
                        }
                    }
                    Note::Empty => {}
                }
                if let Command::Cut(tick) = cell.command {
                    if let Some(note) = active[lane_index].take() {
                        push_lane(
                            &mut result,
                            at + row_duration.mul_f64(f64::from(tick.min(15)) / 16.0),
                            order_index,
                            row_index,
                            vec![0x80 | page.channel, note, 0],
                            lane_index,
                        );
                    }
                }
                if let (Command::Retrigger(count), Note::On(note)) = (cell.command, cell.note) {
                    for n in 1..count.clamp(1, 8) {
                        push_lane(
                            &mut result,
                            event_at + row_duration.mul_f64(f64::from(n) / f64::from(count)),
                            order_index,
                            row_index,
                            vec![0x80 | page.channel, note, 0],
                            lane_index,
                        );
                        push_lane(
                            &mut result,
                            event_at + row_duration.mul_f64(f64::from(n) / f64::from(count)),
                            order_index,
                            row_index,
                            vec![
                                0x90 | page.channel,
                                note,
                                cell.velocity.unwrap_or(page.velocity),
                            ],
                            lane_index,
                        );
                    }
                }
            }
            at += row_duration;
        }
    }
    for (lane_index, note) in active.into_iter().enumerate() {
        if let Some(note) = note {
            let page = &song.pages[lane_index / LANES_PER_PAGE];
            push_lane(
                &mut result,
                at,
                song.order.len().saturating_sub(1),
                0,
                vec![0x80 | page.channel, note, 0],
                lane_index,
            );
        }
    }
    // Do not loop as soon as the last note's gate closes: the final rest rows
    // are musically significant. This boundary marker holds the transport to
    // the exact end of the scheduled pattern/order span.
    if let Some((order, pattern_number)) = song.order.iter().enumerate().next_back() {
        let row = song.patterns[pattern_number].rows.len().saturating_sub(1);
        push(&mut result, at, order, row, Vec::new());
    }
    result.sort_by_key(|message| message.at);
    Ok(result)
}

fn append_program(
    out: &mut Vec<ScheduledMessage>,
    at: Duration,
    order: usize,
    row: usize,
    page: &Page,
    program: u8,
    config: &ExternalMidiConfig,
) {
    match config.bank_select {
        BankSelectMode::Off => {}
        BankSelectMode::Cc0 => push(
            out,
            at,
            order,
            row,
            vec![0xb0 | page.channel, 0, page.bank_msb],
        ),
        BankSelectMode::Cc0Cc32 => {
            push(
                out,
                at,
                order,
                row,
                vec![0xb0 | page.channel, 0, page.bank_msb],
            );
            push(
                out,
                at,
                order,
                row,
                vec![0xb0 | page.channel, 32, page.bank_lsb],
            );
        }
    }
    if config.program_changes {
        push(out, at, order, row, vec![0xc0 | page.channel, program]);
    }
}
fn push(out: &mut Vec<ScheduledMessage>, at: Duration, order: usize, row: usize, bytes: Vec<u8>) {
    out.push(ScheduledMessage {
        at,
        bytes,
        order,
        row,
        lane: None,
    });
}

fn push_lane(
    out: &mut Vec<ScheduledMessage>,
    at: Duration,
    order: usize,
    row: usize,
    bytes: Vec<u8>,
    lane: usize,
) {
    out.push(ScheduledMessage {
        at,
        bytes,
        order,
        row,
        lane: Some(lane),
    });
}

#[cfg(test)]
fn message_channel(bytes: &[u8]) -> Option<u8> {
    let status = *bytes.first()?;
    (0x80..=0xef).contains(&status).then_some(status & 0x0f)
}

pub fn panic_messages(config: &ExternalMidiConfig) -> Vec<Vec<u8>> {
    let channels = config.channels.iter().copied().collect::<BTreeSet<_>>();
    channels
        .into_iter()
        .flat_map(|ch| {
            [
                vec![0xb0 | ch, 64, 0],
                vec![0xb0 | ch, 123, 0],
                vec![0xb0 | ch, 120, 0],
            ]
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct SequencerStatus {
    pub available: bool,
    pub playing: bool,
    pub order: usize,
    pub row: usize,
    pub error: Option<String>,
    pub generation: u64,
}
enum Transport {
    Play(Song, usize, usize),
    Stop,
    Mute(usize, bool),
    Thru(Vec<u8>),
    CancelThru(u8),
    Tempo(u16),
    Shutdown,
}

#[derive(Clone)]
pub struct LiveInput {
    tx: mpsc::Sender<Transport>,
}

impl LiveInput {
    pub fn send(&self, message: &[u8]) {
        let _ = self.tx.send(Transport::Thru(message.to_vec()));
    }

    pub fn cancel(&self, channel: u8) {
        let _ = self.tx.send(Transport::CancelThru(channel));
    }
}

pub struct Sequencer {
    tx: mpsc::Sender<Transport>,
    status: Arc<Mutex<SequencerStatus>>,
    thread: Option<thread::JoinHandle<()>>,
    config: ExternalMidiConfig,
}
impl Sequencer {
    pub fn start(config: &ExternalMidiConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(SequencerStatus::default()));
        let thread_status = Arc::clone(&status);
        let cfg = config.clone();
        let handle = thread::Builder::new()
            .name("shsynth-sequencer".into())
            .spawn(move || run_transport(rx, thread_status, cfg))
            .ok();
        Self {
            tx,
            status,
            thread: handle,
            config: config.clone(),
        }
    }
    pub fn play(&self, song: &Song, order: usize, row: usize) {
        if let Ok(mut status) = self.status.lock() {
            status.playing = true;
            status.order = order;
            status.row = row;
            status.generation = status.generation.wrapping_add(1);
        }
        let _ = self.tx.send(Transport::Play(song.clone(), order, row));
    }
    pub fn live_input(&self) -> LiveInput {
        LiveInput {
            tx: self.tx.clone(),
        }
    }
    pub fn stop(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.playing = false;
        }
        let _ = self.tx.send(Transport::Stop);
    }
    pub fn mute(&self, track: usize, muted: bool) {
        let _ = self.tx.send(Transport::Mute(track, muted));
    }
    pub fn mute_page(&self, page: usize, muted: bool) {
        for lane in 0..LANES_PER_PAGE {
            let _ = self
                .tx
                .send(Transport::Mute(page * LANES_PER_PAGE + lane, muted));
        }
    }
    pub fn tempo(&self, bpm: u16) {
        let _ = self.tx.send(Transport::Tempo(bpm.clamp(20, 300)));
    }
    pub fn thru(&self, message: &[u8]) {
        if self.config.live_thru {
            let _ = self.tx.send(Transport::Thru(message.to_vec()));
        }
    }
    pub fn status(&self) -> SequencerStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn unavailable_label(&self) -> String {
        self.status()
            .error
            .unwrap_or_else(|| "Casio MIDI unavailable".into())
    }
}
impl Drop for Sequencer {
    fn drop(&mut self) {
        let _ = self.tx.send(Transport::Shutdown);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn run_transport(
    rx: mpsc::Receiver<Transport>,
    status: Arc<Mutex<SequencerStatus>>,
    config: ExternalMidiConfig,
) {
    let mut output = connect_output(&config).map_err(|e| e.to_string());
    if output.is_ok() {
        isolate_external_route(&config);
    }
    if let Ok(mut s) = status.lock() {
        s.available = output.is_ok();
        s.error = output.as_ref().err().cloned();
    }
    let mut messages = Vec::new();
    let mut index = 0;
    let mut started = Instant::now();
    let mut muted = BTreeSet::new();
    let mut lane_channels = Vec::new();
    let mut active_notes: BTreeMap<usize, BTreeSet<u8>> = BTreeMap::new();
    let mut thru_notes: BTreeMap<u8, BTreeSet<u8>> = BTreeMap::new();
    let mut transport_tempo = config.default_tempo;
    loop {
        let timeout = messages
            .get(index)
            .map(|m: &ScheduledMessage| (started + m.at).saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_millis(50))
            .min(Duration::from_millis(50));
        match rx.recv_timeout(timeout) {
            Ok(Transport::Play(song, order, row)) => {
                isolate_external_route(&config);
                send_panic(&mut output, &config);
                lane_channels = song
                    .pages
                    .iter()
                    .flat_map(|page| std::iter::repeat(page.channel).take(LANES_PER_PAGE))
                    .collect();
                match schedule(&song, &config, order, row) {
                    Ok(planned) => messages = planned,
                    Err(error) => {
                        messages.clear();
                        if let Ok(mut s) = status.lock() {
                            s.playing = false;
                            s.error = Some(error.to_string());
                        }
                        continue;
                    }
                }
                index = 0;
                started = Instant::now();
                transport_tempo = song.tempo;
                muted.clear();
                active_notes.clear();
                if config.send_transport {
                    let _ = send(&mut output, &[0xfa]);
                }
                if let Ok(mut s) = status.lock() {
                    s.playing = true;
                    s.order = order;
                    s.row = row;
                }
            }
            Ok(Transport::Stop) => {
                messages.clear();
                index = 0;
                send_panic(&mut output, &config);
                active_notes.clear();
                if config.send_transport {
                    let _ = send(&mut output, &[0xfc]);
                }
                if let Ok(mut s) = status.lock() {
                    s.playing = false;
                }
            }
            Ok(Transport::Mute(lane, value)) => {
                if let Some(channel) = lane_channels.get(lane).copied() {
                    if value {
                        muted.insert(lane);
                        if let Some(notes) = active_notes.remove(&lane) {
                            for note in notes {
                                let _ = send(&mut output, &[0x80 | channel, note, 0]);
                            }
                        }
                    } else {
                        muted.remove(&lane);
                    }
                }
            }
            Ok(Transport::Thru(message)) => {
                if let Err(error) = send(&mut output, &message) {
                    if let Ok(mut s) = status.lock() {
                        s.available = false;
                        s.error = Some(error);
                    }
                } else if let [status, note, velocity, ..] = message.as_slice() {
                    let channel = status & 0x0f;
                    match status & 0xf0 {
                        0x90 if *velocity > 0 => {
                            thru_notes.entry(channel).or_default().insert(*note);
                        }
                        0x80 | 0x90 => {
                            if let Some(notes) = thru_notes.get_mut(&channel) {
                                notes.remove(note);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Transport::CancelThru(channel)) => {
                if let Some(notes) = thru_notes.remove(&channel) {
                    for note in notes {
                        let _ = send(&mut output, &[0x80 | channel, note, 0]);
                    }
                }
            }
            Ok(Transport::Tempo(bpm)) => {
                let elapsed = started.elapsed();
                rescale_schedule(&mut messages, index, elapsed, transport_tempo, bpm);
                transport_tempo = bpm;
            }
            Ok(Transport::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                send_panic(&mut output, &config);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        while let Some(message) = messages
            .get(index)
            .filter(|m| started + m.at <= Instant::now())
        {
            let muted_message = message.lane.is_some_and(|lane| muted.contains(&lane));
            let send_error = if message.bytes.is_empty() || muted_message {
                None
            } else {
                send(&mut output, &message.bytes).err()
            };
            if !muted_message {
                if let (Some(lane), [status, note, ..]) = (message.lane, message.bytes.as_slice()) {
                    match status & 0xf0 {
                        0x90 if message.bytes.get(2).copied().unwrap_or(0) > 0 => {
                            active_notes.entry(lane).or_default().insert(*note);
                        }
                        0x80 | 0x90 => {
                            if let Some(notes) = active_notes.get_mut(&lane) {
                                notes.remove(note);
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Some(error) = send_error {
                messages.clear();
                index = 0;
                if let Ok(mut s) = status.lock() {
                    s.available = false;
                    s.playing = false;
                    s.error = Some(error);
                }
                continue;
            }
            if let Ok(mut s) = status.lock() {
                s.order = message.order;
                s.row = message.row;
            }
            index += 1;
        }
        if !messages.is_empty() && index == messages.len() {
            send_panic(&mut output, &config);
            active_notes.clear();
            index = 0;
            started = Instant::now();
        }
    }
}

fn rescale_schedule(
    messages: &mut [ScheduledMessage],
    index: usize,
    elapsed: Duration,
    old_tempo: u16,
    new_tempo: u16,
) {
    let scale = f64::from(old_tempo) / f64::from(new_tempo);
    for message in messages.iter_mut().skip(index) {
        let remaining = message.at.saturating_sub(elapsed);
        message.at = elapsed + remaining.mul_f64(scale);
    }
}
fn isolate_external_route(config: &ExternalMidiConfig) {
    crate::engine::retain_midi_destination(&config.client_name, &config.output_match);
}
fn connect_output(config: &ExternalMidiConfig) -> Result<MidiOutputConnection> {
    if !config.enabled {
        bail!("Casio MIDI disabled (tracker remains offline)");
    }
    let output = MidiOutput::new(&config.client_name)?;
    let port = output
        .ports()
        .into_iter()
        .find(|p| {
            output
                .port_name(p)
                .map(|n| {
                    n.to_lowercase()
                        .contains(&config.output_match.to_lowercase())
                })
                .unwrap_or(false)
        })
        .context("Casio MIDI unavailable")?;
    output
        .connect(&port, "SHSynth accompaniment")
        .map_err(|e| anyhow!(e.to_string()))
}
fn send(
    output: &mut std::result::Result<MidiOutputConnection, String>,
    bytes: &[u8],
) -> std::result::Result<(), String> {
    match output {
        Ok(output) => output.send(bytes).map_err(|error| error.to_string()),
        Err(error) => Err(error.clone()),
    }
}
fn send_panic(
    output: &mut std::result::Result<MidiOutputConnection, String>,
    config: &ExternalMidiConfig,
) {
    for message in panic_messages(config) {
        let _ = send(output, &message);
    }
}

pub fn diagnostic(config: &ExternalMidiConfig) -> Result<String> {
    let output = MidiOutput::new(&config.client_name)?;
    let ports = output
        .ports()
        .iter()
        .filter_map(|p| output.port_name(p).ok())
        .collect::<Vec<_>>();
    let matches = ports
        .iter()
        .filter(|name| {
            name.to_lowercase()
                .contains(&config.output_match.to_lowercase())
        })
        .cloned()
        .collect::<Vec<_>>();
    let page = Page {
        name: "dry-run".into(),
        enabled: true,
        channel: config.channels[0],
        bank_msb: 0,
        bank_lsb: 0,
        program: 0,
        velocity: 64,
        percussion: false,
        lanes: (1..=LANES_PER_PAGE)
            .map(|lane| Lane {
                name: format!("L{lane}"),
                enabled: true,
            })
            .collect(),
    };
    let mut dry = Vec::new();
    append_program(&mut dry, Duration::ZERO, 0, 0, &page, 0, config);
    push(
        &mut dry,
        Duration::ZERO,
        0,
        0,
        vec![0x90 | page.channel, 60, 64],
    );
    push(
        &mut dry,
        Duration::from_millis(250),
        0,
        0,
        vec![0x80 | page.channel, 60, 0],
    );
    if let Some(channel) = config.percussion_channel {
        if config.program_changes {
            if let Some(program) = config.percussion_program {
                push(
                    &mut dry,
                    Duration::ZERO,
                    0,
                    0,
                    vec![0xc0 | channel, program],
                );
            }
        }
        push(&mut dry, Duration::ZERO, 0, 0, vec![0x90 | channel, 36, 96]);
        push(
            &mut dry,
            Duration::from_millis(125),
            0,
            0,
            vec![0x80 | channel, 36, 0],
        );
    }
    let messages = dry
        .iter()
        .map(|m| format!("{:?} @ {}ms", m.bytes, m.at.as_millis()))
        .chain(
            panic_messages(config)
                .iter()
                .map(|m| format!("{m:?} panic")),
        )
        .collect::<Vec<_>>()
        .join("\n  ");
    Ok(format!("profile: {}\nenabled: {}\nconfigured match: {:?}\nmatching ports: {}\navailable MIDI outputs:\n  {}\nchannels: {}\npercussion: {}; percussion program: {}; input map: {} -> [{}]\nbank: {:?}; program: {}; clock/start/stop: {}; live thru: {}\ndry run (NOT transmitted):\n  {}\n",
        config.profile, config.enabled, config.output_match, if matches.is_empty() { "none".into() } else { matches.join(", ") }, if ports.is_empty() { "none".into() } else { ports.join("\n  ") },
        config.channels.iter().map(|c| (c+1).to_string()).collect::<Vec<_>>().join(","), config.percussion_channel.map(|c| (c+1).to_string()).unwrap_or_else(|| "off".into()), config.percussion_program.map(|p| p.to_string()).unwrap_or_else(|| "unchanged".into()), config.percussion_input_base, config.percussion_notes.iter().map(u8::to_string).collect::<Vec<_>>().join(","), config.bank_select, config.program_changes, config.send_transport, config.live_thru, messages))
}

fn escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('|', "%7C")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
}
fn unescape(value: &str) -> Result<String> {
    Ok(value
        .replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%7C", "|")
        .replace("%25", "%"))
}
fn one_based_channel(v: &str) -> Result<u8> {
    let n = v.parse::<u8>()?;
    if !(1..=16).contains(&n) {
        bail!("channel out of range");
    }
    Ok(n - 1)
}
fn midi_value(v: &str) -> Result<u8> {
    let n = v.parse::<u8>()?;
    if n > 127 {
        bail!("MIDI value out of range");
    }
    Ok(n)
}
fn optional_midi(v: &str) -> Result<Option<u8>> {
    if v == "-" {
        Ok(None)
    } else {
        midi_value(v).map(Some)
    }
}
fn note_text(n: Note) -> String {
    match n {
        Note::Empty => "---".into(),
        Note::Off => "OFF".into(),
        Note::On(n) => n.to_string(),
    }
}
fn parse_note(v: &str) -> Result<Note> {
    match v {
        "---" => Ok(Note::Empty),
        "OFF" => Ok(Note::Off),
        _ => midi_value(v).map(Note::On),
    }
}
fn command_text(c: Command) -> String {
    match c {
        Command::None => "-".into(),
        Command::Cut(v) => format!("C{v}"),
        Command::Delay(v) => format!("D{v}"),
        Command::Retrigger(v) => format!("R{v}"),
        Command::Tempo(v) => format!("T{v}"),
    }
}
fn parse_command(v: &str) -> Result<Command> {
    if v == "-" {
        return Ok(Command::None);
    }
    match &v[..1] {
        "C" => Ok(Command::Cut(v[1..].parse()?)),
        "D" => Ok(Command::Delay(v[1..].parse()?)),
        "R" => Ok(Command::Retrigger(v[1..].parse()?)),
        "T" => Ok(Command::Tempo(v[1..].parse()?)),
        _ => bail!("unknown command"),
    }
}

pub fn note_name(note: Note) -> String {
    match note {
        Note::Empty => "---".into(),
        Note::Off => "OFF".into(),
        Note::On(n) => {
            const N: [&str; 12] = [
                "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
            ];
            format!("{}{}", N[usize::from(n % 12)], i16::from(n) / 12 - 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> ExternalMidiConfig {
        let mut c = crate::config::RuntimeConfig::default().external_midi;
        c.program_changes = true;
        c.bank_select = BankSelectMode::Cc0Cc32;
        c
    }
    #[test]
    fn serialization_round_trip_and_old_gate_default() {
        let mut s = Song::new(&config());
        s.name = "a|b".into();
        s.patterns.get_mut(&0).unwrap().rows[0][0].note = Note::On(60);
        let text = encode(&s).unwrap();
        assert_eq!(decode(&text).unwrap(), s);
        assert_eq!(
            decode(&text.replace("gate=80\n", "")).unwrap().gate_percent,
            80
        );
    }
    #[test]
    fn atomic_save_refuses_overwrite() {
        let base = env::temp_dir().join(format!("shsong-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let s = Song::new(&config());
        save(&base, &s, false).unwrap();
        assert!(save(&base, &s, false).is_err());
        assert!(save(&base, &s, true).is_ok());
        assert!(!base.join(".untitled.tmp").exists());
        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn bank_and_program_precede_note_and_notes_end() {
        let c = config();
        let mut s = Song::new(&c);
        let cell = &mut s.patterns.get_mut(&0).unwrap().rows[0][0];
        cell.program = Some(7);
        cell.note = Note::On(60);
        let scheduled = schedule(&s, &c, 0, 0).unwrap();
        let m = scheduled
            .iter()
            .filter(|message| !message.bytes.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(&m[0].bytes[..2], &[0xb0, 0]);
        assert_eq!(&m[1].bytes[..2], &[0xb0, 32]);
        assert_eq!(m[2].bytes[0] & 0xf0, 0xc0);
        assert_eq!(m[3].bytes[0] & 0xf0, 0x90);
        assert!(m.iter().any(|x| x.bytes[0] & 0xf0 == 0x80));
    }
    #[test]
    fn row_timing_pattern_transition_and_tempo() {
        let c = config();
        let mut s = Song::new(&c);
        s.patterns.insert(1, Pattern::empty(64, TOTAL_LANES));
        s.order.push(1);
        s.patterns.get_mut(&0).unwrap().rows[1][0] = Cell {
            note: Note::On(61),
            command: Command::Tempo(60),
            ..Cell::default()
        };
        s.patterns.get_mut(&1).unwrap().rows[0][0].note = Note::On(62);
        let m = schedule(&s, &c, 0, 0).unwrap();
        let notes = m
            .iter()
            .filter(|x| x.bytes.first().is_some_and(|status| status & 0xf0 == 0x90))
            .collect::<Vec<_>>();
        assert_eq!(notes[0].at, Duration::from_millis(125));
        assert_eq!(notes[1].order, 1);
    }
    #[test]
    fn live_tempo_change_rescales_remaining_schedule_monotonically() {
        let c = config();
        let mut song = Song::new(&c);
        song.patterns.insert(0, Pattern::empty(4, TOTAL_LANES));
        let mut messages = schedule(&song, &c, 0, 0).unwrap();
        rescale_schedule(&mut messages, 1, Duration::from_millis(100), 120, 60);
        let times = messages
            .iter()
            .skip(1)
            .map(|message| message.at)
            .collect::<Vec<_>>();
        assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(times.contains(&Duration::from_millis(150)));
        assert_eq!(times.last(), Some(&Duration::from_millis(900)));
    }
    #[test]
    fn panic_covers_every_channel_with_sound_off() {
        let c = config();
        let p = panic_messages(&c);
        for ch in c.channels {
            assert!(p.contains(&vec![0xb0 | ch, 120, 0]));
            assert!(p.contains(&vec![0xb0 | ch, 123, 0]));
        }
    }
    #[test]
    fn installed_profile_has_four_lane_drum_page_on_channel_two() {
        let c = config();
        let mut song = Song::new(&c);
        assert_eq!(song.pages[1].channel, 1);
        assert!(song.pages[1].percussion);
        song.patterns.get_mut(&0).unwrap().rows[0][4].note = Note::On(36);
        assert!(schedule(&song, &c, 0, 0).unwrap().iter().any(|message| {
            message.bytes.first() == Some(&0x91) && message.bytes.get(1) == Some(&36)
        }));
    }
    #[test]
    fn mt240_profile_uses_channel_two_and_selects_percussion_first() {
        let mut c = config();
        c.channels = vec![0, 1];
        c.melody_channel = 0;
        c.percussion_channel = Some(1);
        c.percussion_program = Some(9);
        c.max_tracks = 2;
        c.bank_select = BankSelectMode::Off;
        let mut song = Song::new(&c);
        assert_eq!(song.pages[1].channel, 1);
        assert_eq!(song.pages[1].program, 9);
        assert!(song.pages[1].percussion);
        song.patterns.get_mut(&0).unwrap().rows[0][4].note = Note::On(36);
        let midi = schedule(&song, &c, 0, 0)
            .unwrap()
            .into_iter()
            .filter(|message| !message.bytes.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(midi[0].bytes, [0xc1, 9]);
        assert_eq!(midi[1].bytes, [0x91, 36, 96]);
    }
    #[test]
    fn disabled_track_never_schedules_notes() {
        let c = config();
        let mut s = Song::new(&c);
        s.pages[0].lanes[0].enabled = false;
        s.patterns.get_mut(&0).unwrap().rows[0][0].note = Note::On(60);
        assert!(schedule(&s, &c, 0, 0)
            .unwrap()
            .iter()
            .all(|message| message.bytes.is_empty()));
    }
    #[test]
    fn empty_rows_advance_at_row_timing_and_hold_the_loop_boundary() {
        let c = config();
        let mut s = Song::new(&c);
        s.patterns.insert(0, Pattern::empty(4, TOTAL_LANES));
        let m = schedule(&s, &c, 0, 0).unwrap();
        let ticks = m
            .iter()
            .filter(|message| message.bytes.is_empty())
            .map(|message| (message.at, message.row))
            .collect::<Vec<_>>();
        assert_eq!(
            ticks,
            vec![
                (Duration::ZERO, 0),
                (Duration::from_millis(125), 1),
                (Duration::from_millis(250), 2),
                (Duration::from_millis(375), 3),
                (Duration::from_millis(500), 3),
            ]
        );
        assert_eq!(m.last().unwrap().at, Duration::from_millis(500));
    }
    #[test]
    fn system_realtime_messages_do_not_have_a_mute_channel() {
        assert_eq!(message_channel(&[]), None);
        assert_eq!(message_channel(&[0xf8]), None);
        assert_eq!(message_channel(&[0x99, 36, 100]), Some(9));
    }
    #[test]
    fn both_four_lane_pages_schedule_together_on_shared_page_channels() {
        let mut c = config();
        c.bank_select = BankSelectMode::Off;
        let mut song = Song::new(&c);
        let row = &mut song.patterns.get_mut(&0).unwrap().rows[0];
        for (lane, note) in [60, 64, 67, 71].into_iter().enumerate() {
            row[lane] = Cell {
                note: Note::On(note),
                velocity: Some(80 + lane as u8),
                ..Cell::default()
            };
        }
        for (lane, note) in [36, 38, 40, 41].into_iter().enumerate() {
            row[LANES_PER_PAGE + lane] = Cell {
                note: Note::On(note),
                velocity: Some(100 + lane as u8),
                ..Cell::default()
            };
        }
        let messages = schedule(&song, &c, 0, 0).unwrap();
        let note_ons = messages
            .iter()
            .filter(|message| {
                message
                    .bytes
                    .first()
                    .is_some_and(|status| status & 0xf0 == 0x90)
            })
            .collect::<Vec<_>>();
        assert_eq!(note_ons.iter().filter(|m| m.bytes[0] == 0x90).count(), 4);
        assert_eq!(note_ons.iter().filter(|m| m.bytes[0] == 0x91).count(), 4);
        assert!(note_ons.iter().all(|message| message.at == Duration::ZERO));
        assert_eq!(
            note_ons.iter().map(|m| m.bytes[2]).collect::<Vec<_>>(),
            [80, 81, 82, 83, 100, 101, 102, 103]
        );
        let program = messages.iter().position(|m| m.bytes == [0xc1, 9]).unwrap();
        let first_drum = messages
            .iter()
            .position(|m| m.bytes.first() == Some(&0x91))
            .unwrap();
        assert!(program < first_drum);
    }

    #[test]
    fn shared_channel_lanes_keep_independent_note_off_identity() {
        let c = config();
        let mut song = Song::new(&c);
        let row = &mut song.patterns.get_mut(&0).unwrap().rows[0];
        row[0].note = Note::On(60);
        row[1].note = Note::On(64);
        let messages = schedule(&song, &c, 0, 0).unwrap();
        assert!(messages
            .iter()
            .any(|m| m.lane == Some(0) && m.bytes == [0x80, 60, 0]));
        assert!(messages
            .iter()
            .any(|m| m.lane == Some(1) && m.bytes == [0x80, 64, 0]));
        assert!(!messages
            .iter()
            .any(|m| m.lane == Some(0) && m.bytes == [0x80, 64, 0]));
    }

    #[test]
    fn gesture_waits_sorts_preserves_velocity_and_accepts_staggered_notes() {
        let start = Instant::now();
        let mut gesture = GestureCapture::default();
        gesture.observe(start, &[0x90, 67, 91]);
        gesture.observe(start + Duration::from_millis(5), &[0x80, 67, 0]);
        assert_eq!(
            gesture.finish(start + Duration::from_millis(30), DEFAULT_GESTURE_SETTLE),
            None
        );
        gesture.observe(start + Duration::from_millis(35), &[0x90, 60, 73]);
        gesture.observe(start + Duration::from_millis(40), &[0x90, 64, 82]);
        gesture.observe(start + Duration::from_millis(45), &[0x90, 60, 0]);
        gesture.observe(start + Duration::from_millis(50), &[0x80, 64, 0]);
        let commit = gesture
            .finish(start + Duration::from_millis(100), DEFAULT_GESTURE_SETTLE)
            .unwrap();
        assert_eq!(commit.notes, [(60, 73), (64, 82), (67, 91)]);
        assert!(!commit.overflowed);
    }

    #[test]
    fn gesture_repeated_notes_and_fifth_note_are_deterministic() {
        let start = Instant::now();
        let mut gesture = GestureCapture::default();
        for (offset, note) in [60, 60, 62, 64, 65, 67].into_iter().enumerate() {
            gesture.observe(
                start + Duration::from_millis(offset as u64),
                &[0x90, note, 90 + offset as u8],
            );
        }
        for note in [60, 60, 62, 64, 65, 67] {
            gesture.observe(start + Duration::from_millis(10), &[0x90, note, 0]);
        }
        let commit = gesture
            .finish(start + Duration::from_millis(60), DEFAULT_GESTURE_SETTLE)
            .unwrap();
        assert_eq!(commit.notes.len(), 4);
        assert_eq!(commit.notes[0], (60, 90));
        assert!(commit.overflowed);
    }

    #[test]
    fn version_one_songs_convert_without_touching_source_roles() {
        let legacy = "SHSYNTH-SONG 1\nname=old\ntempo=120\nsteps=4\ngate=80\norder=0\ntrack=0|T1|1|1|0|0|0|96|0\ntrack=1|DRUM|1|2|0|0|9|96|1\npattern=0|1\ncell=0|0|0|60|90|-|-\ncell=0|0|1|36|110|-|-\n";
        let song = decode(legacy).unwrap();
        assert_eq!(song.patterns[&0].rows[0][0].note, Note::On(60));
        assert_eq!(song.patterns[&0].rows[0][4].note, Note::On(36));
        assert_eq!(song.pages[0].channel, 0);
        assert_eq!(song.pages[1].channel, 1);
        assert_eq!(song.pages[1].program, 9);
        assert!(encode(&song).unwrap().starts_with("SHSYNTH-SONG 2\n"));
    }

    #[test]
    fn overwrite_refuses_newer_or_unknown_song_files() {
        let base = env::temp_dir().join(format!("shsong-newer-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("untitled.shsong");
        fs::write(&path, "SHSYNTH-SONG 99\nfuture=data\n").unwrap();
        assert!(save(&base, &Song::new(&config()), true).is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "SHSYNTH-SONG 99\nfuture=data\n"
        );
        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn song_delete_requires_a_supported_file() {
        let base = env::temp_dir().join(format!("shsong-delete-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let song = Song::new(&config());
        let path = save(&base, &song, false).unwrap();
        delete(&base, &song.name).unwrap();
        assert!(!path.exists());
        fs::write(&path, "SHSYNTH-SONG 99\nfuture=data\n").unwrap();
        assert!(delete(&base, &song.name).is_err());
        assert!(path.exists());
        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn dry_run_is_non_transmitting_and_descriptive() {
        let mut c = config();
        c.enabled = false;
        let d = diagnostic(&c).unwrap();
        assert!(d.contains("NOT transmitted"));
        assert!(d.contains("profile:"));
    }
    #[test]
    fn disabled_or_missing_destination_is_an_offline_error_only() {
        let mut c = config();
        c.enabled = false;
        assert!(connect_output(&c)
            .err()
            .expect("disabled output must stay offline")
            .to_string()
            .contains("disabled"));
        let song = Song::new(&c);
        assert!(schedule(&song, &c, 0, 0).is_ok());
    }
}
