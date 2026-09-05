//! JACK-synchronized FT2 WAV loops. Decode/import work happens before JACK
//! activation; the process callback only reads immutable PCM and writes two
//! bounded output buffers.

use crate::config::{ControllerClockConfig, LoopPlayerConfig};
use crate::dsp::{AtomicMeter, MeterAccumulator, MeterSnapshot, StereoFrame, MAX_METER_WINDOW};
use crate::jack::{Client as JackClient, Port as JackPort, PortDirection, PortGetBuffer};
use crate::sequencer::LoopSettings;
use crate::tempo::Bpm;
use alsa::seq::{Addr, EvQueueControl, Event, EventType, PortCap, PortType, Seq};
use alsa::Direction;
use anyhow::{bail, Context, Result};
use libc::{c_int, c_uint, c_void};
use midir::MidiOutput;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const BEAT_UNITS: f64 = 1_000_000.0;
const ANALYSIS_HOP: usize = 1024;
const LOOP_OUTPUT_PORT_NAMES: [&str; 2] = ["output_l", "output_r"];

pub(crate) fn configured_output_ports(config: &LoopPlayerConfig) -> [String; 2] {
    [
        format!("{}:{}", config.client_name, LOOP_OUTPUT_PORT_NAMES[0]),
        format!("{}:{}", config.client_name, LOOP_OUTPUT_PORT_NAMES[1]),
    ]
}
const MAX_LOOP_CALLBACK_FRAMES: usize = MAX_METER_WINDOW;
// Decoding is deliberately bounded because the whole loop stays resident in
// memory for the lock-free JACK callback. Six million stereo frames use about
// 46 MiB and cover 125 seconds at 48 kHz.
const MAX_DECODED_LOOP_FRAMES: u32 = 6_000_000;
pub const LOOP_SLOTS: usize = 4;
const FILTER_DEADBAND: f32 = 0.04;
const CONTROL_SMOOTH_SECONDS: f32 = 0.015;

#[derive(Debug)]
pub struct TransportClock {
    playing: AtomicBool,
    controller_owned: AtomicBool,
    generation: AtomicU64,
    loop_generation: AtomicU64,
    origin_beat: AtomicU64,
    bpm_x100: AtomicU64,
    controller_tx: Option<mpsc::Sender<ControllerClockCommand>>,
    controller_thread: Mutex<Option<thread::JoinHandle<()>>>,
}

impl Default for TransportClock {
    fn default() -> Self {
        Self::new(
            &ControllerClockConfig {
                enabled: false,
                client_name: String::new(),
                output_match: String::new(),
            },
            Bpm::DEFAULT,
        )
    }
}

impl TransportClock {
    pub fn new(config: &ControllerClockConfig, initial_bpm: Bpm) -> Self {
        Self::new_with_external_owner(config, initial_bpm, false)
    }

    pub fn new_with_external_owner(
        config: &ControllerClockConfig,
        initial_bpm: Bpm,
        external_owner: bool,
    ) -> Self {
        let (controller_tx, controller_thread) = if config.enabled {
            let (tx, rx) = mpsc::channel();
            if external_owner {
                let _ = tx.send(ControllerClockCommand::Suspend);
            }
            let output = AlsaControllerClockOutput::new(config.clone());
            let handle = thread::Builder::new()
                .name("shsynth-controller-clock".into())
                .spawn(move || run_controller_clock(rx, Box::new(output), initial_bpm.as_f64()))
                .ok();
            match handle {
                Some(handle) => (Some(tx), Some(handle)),
                None => (None, None),
            }
        } else {
            (None, None)
        };
        Self {
            playing: AtomicBool::new(false),
            controller_owned: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            loop_generation: AtomicU64::new(u64::MAX),
            origin_beat: AtomicU64::new(0),
            bpm_x100: AtomicU64::new(u64::from(initial_bpm.hundredths())),
            controller_tx,
            controller_thread: Mutex::new(controller_thread),
        }
    }

    pub fn play(&self, origin_beat: f64, bpm: Bpm) {
        self.publish_play(origin_beat, bpm);
        self.controller_owned.store(true, Ordering::Release);
        if let Some(tx) = &self.controller_tx {
            let _ = tx.send(ControllerClockCommand::Start(bpm.as_f64()));
        }
    }

    /// Follow an incoming transport without also publishing SHR's controller
    /// clock output. This is the mutual-exclusion boundary that prevents a
    /// second clock owner or feedback loop.
    pub fn play_external(&self, origin_beat: f64, bpm: Bpm) {
        self.publish_play(origin_beat, bpm);
        self.suspend_controller_output();
    }

    #[cfg(test)]
    pub(crate) fn has_controller_output(&self) -> bool {
        self.controller_tx.is_some()
    }

    pub fn suspend_controller_output(&self) {
        self.controller_owned.store(false, Ordering::Release);
        if let Some(tx) = &self.controller_tx {
            let _ = tx.send(ControllerClockCommand::Suspend);
        }
    }

    fn publish_play(&self, origin_beat: f64, bpm: Bpm) {
        self.origin_beat.store(
            (origin_beat.max(0.0) * BEAT_UNITS) as u64,
            Ordering::Release,
        );
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.bpm_x100
            .store(u64::from(bpm.hundredths()), Ordering::Release);
        self.playing.store(true, Ordering::Release);
    }

    pub fn stop(&self) {
        if self.playing.swap(false, Ordering::AcqRel) {
            if self.controller_owned.swap(false, Ordering::AcqRel) {
                if let Some(tx) = &self.controller_tx {
                    let _ = tx.send(ControllerClockCommand::Stop);
                }
            }
        }
    }

    /// Authorize the fixed loop renderers for the current transport
    /// generation. A Pattern boundary increments `generation`, making every
    /// outgoing renderer silent until the incoming Pattern has been prepared
    /// and armed outside the callback.
    pub fn arm_loops(&self) {
        self.loop_generation
            .store(self.generation.load(Ordering::Acquire), Ordering::Release);
    }

    pub fn tempo(&self, bpm: Bpm) {
        self.bpm_x100
            .store(u64::from(bpm.hundredths()), Ordering::Release);
        if self.controller_owned.load(Ordering::Acquire) {
            if let Some(tx) = &self.controller_tx {
                let _ = tx.send(ControllerClockCommand::Tempo(bpm.as_f64()));
            }
        }
    }

    /// Reposition the loop at a repeated Project boundary without emitting a
    /// second MIDI Start. Controller transport remains one continuous run.
    pub fn restart_cycle(&self, origin_beat: f64) {
        self.origin_beat.store(
            (origin_beat.max(0.0) * BEAT_UNITS) as u64,
            Ordering::Release,
        );
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl Drop for TransportClock {
    fn drop(&mut self) {
        self.stop();
        if let Some(tx) = &self.controller_tx {
            let _ = tx.send(ControllerClockCommand::Shutdown);
        }
        if let Ok(mut handle) = self.controller_thread.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ControllerClockCommand {
    Start(f64),
    Tempo(f64),
    Stop,
    Suspend,
    Shutdown,
}

#[derive(Clone, Copy, Debug)]
enum ControllerClockMessage {
    TimingClock,
    Start,
    Stop,
}

impl ControllerClockMessage {
    #[cfg(test)]
    const fn bytes(self) -> &'static [u8] {
        match self {
            Self::TimingClock => &[0xf8],
            Self::Start => &[0xfa],
            Self::Stop => &[0xfc],
        }
    }
}

trait ControllerClockOutput: Send {
    fn send(&mut self, message: ControllerClockMessage) -> std::result::Result<(), String>;
}

struct AlsaControllerClockOutput {
    config: ControllerClockConfig,
    connection: Option<AlsaDirectClockConnection>,
}

struct AlsaDirectClockConnection {
    sequencer: Seq,
    source_port: i32,
    destination: Addr,
}

impl AlsaControllerClockOutput {
    fn new(config: ControllerClockConfig) -> Self {
        Self {
            config,
            connection: None,
        }
    }

    fn connect(&mut self) -> std::result::Result<(), String> {
        let output =
            MidiOutput::new(&self.config.client_name).map_err(|error| error.to_string())?;
        let ports = output.ports();
        let names = ports
            .iter()
            .map(|port| output.port_name(port).unwrap_or_default())
            .collect::<Vec<_>>();
        let index = matching_controller_output_index(&names, &self.config.output_match)?;
        let destination = alsa_address_from_midir_name(&names[index])?;
        drop(output);

        let sequencer =
            Seq::open(None, Some(Direction::Playback), false).map_err(|error| error.to_string())?;
        let client_name = CString::new(self.config.client_name.as_str())
            .map_err(|_| "controller clock client name contains a NUL byte".to_owned())?;
        sequencer
            .set_client_name(&client_name)
            .map_err(|error| error.to_string())?;
        let destination_info = sequencer
            .get_any_port_info(destination)
            .map_err(|error| error.to_string())?;
        if !destination_info.get_capability().contains(PortCap::WRITE) {
            return Err(format!(
                "controller clock destination {}:{} is not writable",
                destination.client, destination.port
            ));
        }
        let port_name = CString::new("SHR-DAW controller clock only").expect("static port name");
        let source_port = sequencer
            .create_simple_port(
                &port_name,
                controller_clock_source_capabilities(),
                PortType::MIDI_GENERIC | PortType::APPLICATION,
            )
            .map_err(|error| error.to_string())?;
        self.connection = Some(AlsaDirectClockConnection {
            sequencer,
            source_port,
            destination,
        });
        Ok(())
    }
}

fn controller_clock_source_capabilities() -> PortCap {
    PortCap::READ | PortCap::NO_EXPORT
}

fn alsa_address_from_midir_name(name: &str) -> std::result::Result<Addr, String> {
    let address = name
        .rsplit_once(' ')
        .map(|(_, address)| address)
        .ok_or_else(|| format!("ALSA output {name:?} has no client:port address"))?;
    address
        .parse::<Addr>()
        .map_err(|_| format!("ALSA output {name:?} has an invalid client:port address"))
}

pub(crate) fn matching_controller_output_index(
    names: &[String],
    wanted: &str,
) -> std::result::Result<usize, String> {
    if wanted.trim().is_empty() || wanted.trim() != wanted {
        return Err("controller clock output must be one exact ALSA port name".into());
    }
    crate::midi_endpoint::matching_index(names, wanted, "controller clock output")
        .map_err(|error| error.to_string())
}

pub fn controller_clock_outputs(client_name: &str) -> Result<Vec<String>> {
    let output = MidiOutput::new(client_name)?;
    let mut names = output
        .ports()
        .iter()
        .filter_map(|port| output.port_name(port).ok())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

impl ControllerClockOutput for AlsaControllerClockOutput {
    fn send(&mut self, message: ControllerClockMessage) -> std::result::Result<(), String> {
        if self.connection.is_none() {
            self.connect()?;
        }
        let connection = self
            .connection
            .as_mut()
            .expect("controller clock connection was established");
        let event_type = match message {
            ControllerClockMessage::TimingClock => EventType::Clock,
            ControllerClockMessage::Start => EventType::Start,
            ControllerClockMessage::Stop => EventType::Stop,
        };
        let mut event = Event::new(
            event_type,
            &EvQueueControl {
                queue: 0,
                value: (),
            },
        );
        event.set_source(connection.source_port);
        event.set_dest(connection.destination);
        event.set_direct();
        let result = connection
            .sequencer
            .event_output_direct(&mut event)
            .map(|_| ())
            .map_err(|error| error.to_string());
        if result.is_err() {
            self.connection = None;
        }
        result
    }
}

#[derive(Clone, Copy, Debug)]
struct ControllerClockPhase {
    interval_seconds: f64,
    next_tick_seconds: f64,
}

impl ControllerClockPhase {
    fn start(now: Duration, bpm: f64) -> Self {
        Self {
            interval_seconds: controller_clock_interval_seconds(bpm),
            next_tick_seconds: now.as_secs_f64(),
        }
    }

    fn tempo(&mut self, now: Duration, bpm: f64) {
        let now = now.as_secs_f64();
        let new_interval = controller_clock_interval_seconds(bpm);
        let remaining = (self.next_tick_seconds - now).max(0.0);
        let phase_remaining = (remaining / self.interval_seconds).clamp(0.0, 1.0);
        self.interval_seconds = new_interval;
        self.next_tick_seconds = now + new_interval * phase_remaining;
    }

    /// Return at most one due pulse. If scheduling was delayed, advance to
    /// the first future phase instead of sending a catch-up burst.
    fn take_due(&mut self, now: Duration) -> bool {
        let now = now.as_secs_f64();
        // `Duration` has nanosecond resolution while phase is retained as a
        // fractional second to avoid cumulative rounding. Treat conversion
        // to the nearest nanosecond as the same deadline.
        if now + 0.000_000_001 < self.next_tick_seconds {
            return false;
        }
        self.next_tick_seconds += self.interval_seconds;
        if self.next_tick_seconds <= now {
            let skipped = ((now - self.next_tick_seconds) / self.interval_seconds).floor() + 1.0;
            self.next_tick_seconds += skipped * self.interval_seconds;
        }
        true
    }

    fn next_tick(&self) -> Duration {
        Duration::from_secs_f64(self.next_tick_seconds)
    }
}

#[cfg(test)]
fn controller_clock_interval(bpm: f64) -> Duration {
    Duration::from_secs_f64(controller_clock_interval_seconds(bpm))
}

fn controller_clock_interval_seconds(bpm: f64) -> f64 {
    60.0 / bpm.clamp(20.0, 300.0) / 24.0
}

fn run_controller_clock(
    receiver: mpsc::Receiver<ControllerClockCommand>,
    mut output: Box<dyn ControllerClockOutput>,
    initial_bpm: f64,
) {
    let origin = Instant::now();
    let elapsed = || origin.elapsed();
    let mut phase = ControllerClockPhase::start(elapsed(), initial_bpm);
    let mut output_available = true;
    let mut clock_sent = false;
    let mut transport_running = false;
    let mut suspended = false;
    loop {
        let timeout = if suspended {
            Duration::from_millis(50)
        } else {
            phase.next_tick().saturating_sub(elapsed())
        };
        match receiver.recv_timeout(timeout) {
            Ok(ControllerClockCommand::Start(bpm)) => {
                suspended = false;
                phase.tempo(elapsed(), bpm);
                if transport_running && output_available {
                    let _ = output.send(ControllerClockMessage::Stop);
                }
                if !clock_sent {
                    output_available = output.send(ControllerClockMessage::TimingClock).is_ok();
                    clock_sent = output_available;
                    let _ = phase.take_due(elapsed());
                }
                if output_available {
                    output_available = output.send(ControllerClockMessage::Start).is_ok();
                }
                transport_running = true;
            }
            Ok(ControllerClockCommand::Tempo(bpm)) => {
                phase.tempo(elapsed(), bpm);
            }
            Ok(ControllerClockCommand::Stop) => {
                if transport_running && output_available {
                    let _ = output.send(ControllerClockMessage::Stop);
                }
                transport_running = false;
            }
            Ok(ControllerClockCommand::Suspend) => {
                if transport_running && output_available {
                    let _ = output.send(ControllerClockMessage::Stop);
                }
                transport_running = false;
                suspended = true;
                clock_sent = false;
            }
            Ok(ControllerClockCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if transport_running && output_available {
                    let _ = output.send(ControllerClockMessage::Stop);
                }
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !suspended && phase.take_due(elapsed()) && output_available {
                    output_available = output.send(ControllerClockMessage::TimingClock).is_ok();
                    clock_sent = output_available;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct DecodedLoop {
    pub samples: Vec<[f32; 2]>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl DecodedLoop {
    pub fn open(path: &Path) -> Result<Self> {
        let mut reader =
            hound::WavReader::open(path).with_context(|| format!("open WAV {}", path.display()))?;
        let spec = reader.spec();
        if !matches!(spec.channels, 1 | 2) {
            bail!(
                "WAV must be mono or stereo (found {} channels)",
                spec.channels
            );
        }
        if !(8_000..=384_000).contains(&spec.sample_rate) {
            bail!("unsupported WAV sample rate {} Hz", spec.sample_rate);
        }
        let frames = checked_loop_frames(reader.duration())?;
        let mut samples = Vec::with_capacity(frames);
        match spec.sample_format {
            hound::SampleFormat::Float => {
                let mut raw = reader.samples::<f32>();
                while let Some(left) = raw.next() {
                    let left = checked_float_sample(left)?;
                    let right = if spec.channels == 1 {
                        left
                    } else {
                        checked_float_sample(raw.next().context("incomplete stereo WAV frame")?)?
                    };
                    samples.push([left, right]);
                }
            }
            hound::SampleFormat::Int => {
                let bits = u32::from(spec.bits_per_sample);
                if bits == 0 || bits > 32 {
                    bail!("unsupported WAV integer depth {}", spec.bits_per_sample);
                }
                let divisor = 2_f32.powi(bits.saturating_sub(1) as i32);
                let mut raw = reader.samples::<i32>();
                while let Some(left) = raw.next() {
                    let left = left.context("malformed integer WAV sample")? as f32 / divisor;
                    let right = if spec.channels == 1 {
                        left
                    } else {
                        raw.next()
                            .context("incomplete stereo WAV frame")?
                            .context("malformed integer WAV sample")? as f32
                            / divisor
                    };
                    samples.push([left, right]);
                }
            }
        }
        if samples.is_empty() || samples.len() != frames {
            bail!("WAV has no complete audio frames");
        }
        Ok(Self {
            samples,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
        })
    }

    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.samples.len() as f64 / f64::from(self.sample_rate))
    }
}

fn checked_loop_frames(frames: u32) -> Result<usize> {
    if frames > MAX_DECODED_LOOP_FRAMES {
        bail!("WAV has {frames} frames; the safe loop limit is {MAX_DECODED_LOOP_FRAMES} frames");
    }
    Ok(frames as usize)
}

fn checked_float_sample(sample: hound::Result<f32>) -> Result<f32> {
    let sample = sample.context("malformed float WAV sample")?;
    if !sample.is_finite() {
        bail!("WAV contains a non-finite float sample");
    }
    Ok(sample.clamp(-1.0, 1.0))
}

pub fn loops_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()))
                .join(".local/share")
        })
        .join("shsynth/loops")
}

pub fn list_wavs(directory: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryEntry {
    pub file: String,
    pub current: bool,
    pub saved_references: usize,
}

pub fn library_entries(
    directory: &Path,
    current: Option<&LoopSettings>,
    projects: &Path,
) -> Result<Vec<LibraryEntry>> {
    let mut references = std::collections::BTreeMap::<String, usize>::new();
    for name in crate::sequencer::list(projects) {
        let song = crate::sequencer::load(projects, &name)
            .with_context(|| format!("inspect saved Project {name}"))?;
        for settings in song
            .patterns
            .values()
            .flat_map(|pattern| pattern.audio_loops.iter().flatten())
        {
            *references.entry(settings.file.clone()).or_default() += 1;
        }
    }
    Ok(list_wavs(directory)
        .into_iter()
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .map(|file| LibraryEntry {
            current: current.is_some_and(|settings| settings.file == file),
            saved_references: references.get(&file).copied().unwrap_or(0),
            file,
        })
        .collect())
}

pub fn import(source: &Path, destination: &Path) -> Result<(PathBuf, DecodedLoop)> {
    let decoded = DecodedLoop::open(source)?;
    fs::create_dir_all(destination)?;
    let original = source
        .file_name()
        .and_then(|name| name.to_str())
        .context("WAV filename is not valid UTF-8")?;
    let stem = Path::new(original)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("loop");
    let safe = crate::sequencer::safe_name(stem);
    for suffix in 1..=9999 {
        let target = if suffix == 1 {
            destination.join(format!("{safe}.wav"))
        } else {
            destination.join(format!("{safe}-{suffix}.wav"))
        };
        let mut output = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(output) => output,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("create {}", target.display()))
            }
        };
        let result = (|| -> Result<()> {
            let mut input = File::open(source)?;
            io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            Ok(())
        })();
        if let Err(error) = result {
            drop(output);
            let _ = fs::remove_file(&target);
            return Err(error)
                .with_context(|| format!("copy private loop to {}", target.display()));
        }
        drop(output);
        fs::File::open(destination)?.sync_all()?;
        return Ok((target, decoded));
    }
    bail!("too many imported loops named {safe}")
}

#[cfg(test)]
pub fn bpm_candidates(measured: f64) -> [f64; 3] {
    [measured / 2.0, measured, measured * 2.0]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopAlignment {
    pub source_bpm: f64,
    pub length_beats: u32,
    pub bars: u32,
    pub transient_detected: bool,
}

pub fn analyze_alignment(decoded: &DecodedLoop, pattern_bpm: Bpm, meter: u8) -> LoopAlignment {
    let duration = decoded.duration().as_secs_f64().max(0.001);
    let meter = u32::from(meter.clamp(1, 16));
    let estimated = estimate_bpm(decoded);
    let measured_bpm = estimated.unwrap_or_else(|| pattern_bpm.as_f64());
    let measured_beats = (duration * measured_bpm / 60.0).round().max(1.0) as u32;
    let bars = ((measured_beats as f64 / f64::from(meter)).round() as u32).max(1);
    let length_beats = bars.saturating_mul(meter).max(1);
    LoopAlignment {
        source_bpm: (f64::from(length_beats) * 60.0 / duration).clamp(20.0, 300.0),
        length_beats,
        bars,
        transient_detected: estimated.is_some(),
    }
}

fn estimate_bpm(decoded: &DecodedLoop) -> Option<f64> {
    if decoded.samples.len() < ANALYSIS_HOP * 4 {
        return None;
    }
    let envelope = onset_envelope(decoded);
    let energy = envelope.iter().sum::<f64>();
    if energy <= f64::EPSILON {
        return None;
    }
    let windows_per_second = f64::from(decoded.sample_rate) / ANALYSIS_HOP as f64;
    let mut best = None;
    for bpm in (60..=200).rev() {
        let lag = (windows_per_second * 60.0 / f64::from(bpm)).round() as usize;
        if lag == 0 || lag >= envelope.len() {
            continue;
        }
        let score = envelope
            .iter()
            .skip(lag)
            .zip(envelope.iter())
            .map(|(a, b)| a * b)
            .sum::<f64>()
            / (envelope.len() - lag) as f64;
        if score > best.map_or(0.0, |(_, score)| score) {
            best = Some((f64::from(bpm), score));
        }
    }
    best.filter(|(_, score)| *score > 1.0e-8)
        .map(|(bpm, _)| bpm)
}

fn onset_envelope(decoded: &DecodedLoop) -> Vec<f64> {
    let mut previous = 0.0;
    decoded
        .samples
        .chunks(ANALYSIS_HOP)
        .map(|chunk| {
            let energy = chunk
                .iter()
                .map(|sample| {
                    let mono = (f64::from(sample[0]) + f64::from(sample[1])) * 0.5;
                    mono * mono
                })
                .sum::<f64>()
                / chunk.len().max(1) as f64;
            let onset = (energy - previous).max(0.0);
            previous = energy;
            onset
        })
        .collect()
}

pub fn beat_to_frame(beat: f64, bpm: f64, sample_rate: u32) -> usize {
    (beat.max(0.0) * 60.0 / bpm.max(0.01) * f64::from(sample_rate)).round() as usize
}

pub fn bar_to_beat(bars: u32, meter: u8) -> u32 {
    bars.saturating_mul(u32::from(meter.clamp(1, 16)))
}

pub fn fade_frames(sample_rate: u32, slice_frames: usize) -> usize {
    ((f64::from(sample_rate) * 0.005).round() as usize)
        .max(1)
        .min(slice_frames.saturating_div(4).max(1))
}

pub fn render_sample(
    samples: &[[f32; 2]],
    region_start: usize,
    region_len: usize,
    phase: f64,
    fade: usize,
) -> [f32; 2] {
    if region_len == 0 || samples.is_empty() {
        return [0.0; 2];
    }
    let relative = (phase - region_start as f64).rem_euclid(region_len as f64);
    let positioned = region_start as f64 + relative;
    let index = positioned.floor() as usize;
    let next = if index + 1 < region_start + region_len {
        index + 1
    } else {
        region_start
    };
    let fraction = (positioned - index as f64) as f32;
    let a = samples.get(index).copied().unwrap_or([0.0; 2]);
    let b = samples.get(next).copied().unwrap_or(a);
    let edge = relative.min(region_len as f64 - relative);
    let envelope = (edge / fade.max(1) as f64).clamp(0.0, 1.0) as f32;
    [
        (a[0] + (b[0] - a[0]) * fraction) * envelope,
        (a[1] + (b[1] - a[1]) * fraction) * envelope,
    ]
}

#[cfg(test)]
pub fn song_position_beats(song: &crate::sequencer::Song, order: usize, row: usize) -> f64 {
    let prior_rows = song
        .order
        .iter()
        .take(order)
        .filter_map(|number| song.patterns.get(number))
        .map(|pattern| pattern.rows.len())
        .sum::<usize>();
    (prior_rows + row) as f64 / f64::from(song.steps_per_beat)
}

pub fn loop_phase_from_song(origin_beat: f64, offset_beats: i32, loop_beats: f64) -> f64 {
    if loop_beats > 0.0 {
        (origin_beat - f64::from(offset_beats)).rem_euclid(loop_beats) / loop_beats
    } else {
        0.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct LoopStatus {
    pub loaded: bool,
    pub playing: bool,
    pub file: Option<String>,
    pub source_rate: u32,
    pub source_channels: u16,
    pub duration: Duration,
    pub elapsed: Duration,
    pub error: Option<String>,
    pub running: bool,
    pub muted: bool,
    pub queued: Option<LoopCommand>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopCommand {
    Launch,
    Stop,
}

#[derive(Clone)]
struct PreparedLoop {
    samples: Arc<Vec<[f32; 2]>>,
    source_rate: u32,
    interpreted_bpm: f64,
    region_start: usize,
    region_len: usize,
    offset_beats: i32,
    meter: u8,
}

struct SlotControl {
    running: AtomicBool,
    muted: AtomicBool,
    tempo_fault: AtomicBool,
    pattern_generation: AtomicU64,
    queued: AtomicU8,
    level_x1000: AtomicU32,
    filter_bits: AtomicU32,
}

impl SlotControl {
    fn new(settings: Option<&LoopSettings>) -> Self {
        Self {
            running: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            tempo_fault: AtomicBool::new(false),
            pattern_generation: AtomicU64::new(u64::MAX),
            queued: AtomicU8::new(0),
            level_x1000: AtomicU32::new(
                settings.map_or(1000, |settings| u32::from(settings.level_x1000)),
            ),
            filter_bits: AtomicU32::new(
                settings
                    .map_or(0.0, |settings| f32::from(settings.filter_x1000) / 1000.0)
                    .to_bits(),
            ),
        }
    }

    fn queued(&self) -> Option<LoopCommand> {
        match self.queued.load(Ordering::Acquire) {
            1 => Some(LoopCommand::Launch),
            2 => Some(LoopCommand::Stop),
            _ => None,
        }
    }

    fn queue(&self, command: Option<LoopCommand>) {
        self.queued.store(
            match command {
                None => 0,
                Some(LoopCommand::Launch) => 1,
                Some(LoopCommand::Stop) => 2,
            },
            Ordering::Release,
        );
    }
}

struct LoopSlot {
    prepared: Option<PreparedLoop>,
    status: LoopStatus,
    position: Arc<AtomicU64>,
    meter: Arc<AtomicMeter>,
    control: Arc<SlotControl>,
}

impl Default for LoopSlot {
    fn default() -> Self {
        Self {
            prepared: None,
            status: LoopStatus::default(),
            position: Arc::new(AtomicU64::new(0)),
            meter: Arc::new(AtomicMeter::default()),
            control: Arc::new(SlotControl::new(None)),
        }
    }
}

fn prepare_loop_slot(
    decoded: DecodedLoop,
    settings: &LoopSettings,
    pattern_bpm: f64,
    meter: u8,
) -> Result<LoopSlot> {
    if decoded.samples.is_empty()
        || !(8_000..=384_000).contains(&decoded.sample_rate)
        || !matches!(decoded.channels, 1 | 2)
        || decoded
            .samples
            .iter()
            .flatten()
            .any(|sample| !sample.is_finite())
    {
        bail!("invalid decoded WAV loop");
    }
    if !(2_000..=30_000).contains(&settings.source_bpm_x100)
        || settings.length_beats == 0
        || !(-16_384..=16_384).contains(&settings.offset_beats)
        || settings.level_x1000 > 1_500
        || !(-1_000..=1_000).contains(&settings.filter_x1000)
    {
        bail!("invalid private loop settings");
    }
    let source_rate = decoded.sample_rate;
    let source_channels = decoded.channels;
    let interpreted = settings.interpreted_bpm();
    require_compatible_tempo(interpreted, pattern_bpm)?;
    let start = beat_to_frame(f64::from(settings.start_beat), interpreted, source_rate)
        .min(decoded.samples.len().saturating_sub(1));
    let requested = beat_to_frame(f64::from(settings.length_beats), interpreted, source_rate);
    let length = requested
        .max(1)
        .min(decoded.samples.len().saturating_sub(start));
    Ok(LoopSlot {
        prepared: Some(PreparedLoop {
            samples: Arc::new(decoded.samples),
            source_rate,
            interpreted_bpm: interpreted,
            region_start: start,
            region_len: length,
            offset_beats: settings.offset_beats,
            meter: meter.clamp(1, 16),
        }),
        status: LoopStatus {
            file: Some(settings.file.clone()),
            source_rate,
            source_channels,
            duration: Duration::from_secs_f64(length as f64 / f64::from(source_rate)),
            ..LoopStatus::default()
        },
        position: Arc::new(AtomicU64::new(0)),
        meter: Arc::new(AtomicMeter::default()),
        control: Arc::new(SlotControl::new(Some(settings))),
    })
}

/// Fully validate and prepare a loop candidate without publishing runtime
/// state. This is used when editing a Pattern that does not currently own the
/// sounding loop.
pub fn validate_prepared_loop(
    decoded: DecodedLoop,
    settings: &LoopSettings,
    pattern_bpm: Bpm,
    meter: u8,
) -> Result<()> {
    drop(prepare_loop_slot(
        decoded,
        settings,
        pattern_bpm.as_f64(),
        meter,
    )?);
    Ok(())
}

pub struct LoopPlayer {
    config: LoopPlayerConfig,
    clock: Arc<TransportClock>,
    active: Option<Active>,
    slots: [LoopSlot; LOOP_SLOTS],
    meter: Arc<AtomicMeter>,
    preview: bool,
}

struct Active {
    jack: JackClient,
    callback: Box<CallbackData>,
    client_state: Arc<LoopClientState>,
    publication: Arc<RendererPublication>,
    sample_rate: u32,
}

impl LoopPlayer {
    pub fn new(config: &LoopPlayerConfig, clock: Arc<TransportClock>) -> Self {
        Self {
            config: config.clone(),
            clock,
            active: None,
            slots: std::array::from_fn(|_| LoopSlot::default()),
            meter: Arc::new(AtomicMeter::default()),
            preview: false,
        }
    }

    pub fn load(&mut self, decoded: DecodedLoop, settings: &LoopSettings) -> Result<()> {
        self.load_slot(0, decoded, settings, settings.interpreted_bpm(), 4)
    }

    pub fn load_slot(
        &mut self,
        slot: usize,
        decoded: DecodedLoop,
        settings: &LoopSettings,
        pattern_bpm: f64,
        meter: u8,
    ) -> Result<()> {
        let slot = checked_slot(slot)?;
        let mut next = prepare_loop_slot(decoded, settings, pattern_bpm, meter)?;
        next.position = Arc::clone(&self.slots[slot].position);
        next.meter = Arc::clone(&self.slots[slot].meter);
        let previous = std::mem::replace(&mut self.slots[slot], next);
        self.preview = false;
        if let Err(error) = self.rebuild_backend() {
            self.slots[slot] = previous;
            let restoration = self.rebuild_backend();
            self.slots[slot].status.error = Some(error.to_string());
            return Err(match restoration {
                Ok(()) => error,
                Err(restore) => {
                    anyhow::anyhow!("{error:#} · healthy slots restore failed: {restore:#}")
                }
            });
        }
        self.slots[slot].status.loaded = true;
        let generation = self.clock.generation.load(Ordering::Acquire);
        if self.clock.loop_generation.load(Ordering::Acquire) == generation {
            self.slots[slot]
                .control
                .pattern_generation
                .store(generation, Ordering::Release);
        }
        Ok(())
    }

    /// Replace all four Pattern-owned slots as one bounded backend
    /// transaction. WAV decoding has already completed on the caller thread;
    /// the callback still sees exactly four fixed renderers and one stereo
    /// sum.
    pub fn replace_pattern_slots(
        &mut self,
        mut inputs: [Option<(DecodedLoop, LoopSettings)>; LOOP_SLOTS],
        mut faults: [Option<(String, String)>; LOOP_SLOTS],
        pattern_bpm: f64,
        meter: u8,
    ) -> Result<[bool; LOOP_SLOTS]> {
        let mut next = std::array::from_fn(|_| LoopSlot::default());
        let mut failed = [false; LOOP_SLOTS];
        for slot in 0..LOOP_SLOTS {
            if let Some((decoded, settings)) = inputs[slot].take() {
                match prepare_loop_slot(decoded, &settings, pattern_bpm, meter) {
                    Ok(prepared) => next[slot] = prepared,
                    Err(error) => {
                        next[slot].status.file = Some(settings.file);
                        next[slot].status.error = Some(error.to_string());
                        failed[slot] = true;
                    }
                }
            } else if let Some((file, error)) = faults[slot].take() {
                next[slot].status.file = Some(file);
                next[slot].status.error = Some(error);
                failed[slot] = true;
            }
            next[slot].position = Arc::clone(&self.slots[slot].position);
            next[slot].meter = Arc::clone(&self.slots[slot].meter);
        }
        let previous = std::mem::replace(&mut self.slots, next);
        self.preview = false;
        if let Err(error) = self.rebuild_backend_isolated(&mut failed) {
            self.slots = previous;
            let restoration = self.rebuild_backend();
            return Err(match restoration {
                Ok(()) => error,
                Err(restore) => {
                    anyhow::anyhow!("{error:#} · previous Pattern restore failed: {restore:#}")
                }
            });
        }
        for slot in &mut self.slots {
            slot.status.loaded = slot.prepared.is_some();
        }
        Ok(failed)
    }

    pub fn status(&self) -> LoopStatus {
        self.slot_status(0)
    }

    pub fn backend_active(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.client_state.active.load(Ordering::Acquire))
    }

    pub fn slot_status(&self, slot: usize) -> LoopStatus {
        if let Some(active) = self.active.as_ref() {
            active.publication.reclaim_retired();
        }
        let Ok(slot) = checked_slot(slot) else {
            return LoopStatus {
                error: Some("loop slot must be 1..=4".into()),
                ..LoopStatus::default()
            };
        };
        let source = &self.slots[slot];
        let mut status = source.status.clone();
        let client_active = self
            .active
            .as_ref()
            .is_some_and(|active| active.client_state.active.load(Ordering::Acquire));
        status.running = source.control.running.load(Ordering::Acquire);
        status.muted = source.control.muted.load(Ordering::Acquire);
        status.queued = source.control.queued();
        if source.control.tempo_fault.load(Ordering::Acquire) {
            status.error = Some("loop tempo no longer matches Pattern tempo".into());
        }
        status.playing = status.loaded
            && status.running
            && !status.muted
            && self.clock.playing.load(Ordering::Acquire)
            && (client_active || self.preview);
        if status.loaded && !client_active && !self.preview {
            status
                .error
                .get_or_insert_with(|| "JACK loop client inactive".into());
        }
        if status.source_rate > 0 {
            status.elapsed = Duration::from_secs_f64(
                source.position.load(Ordering::Acquire) as f64 / f64::from(status.source_rate),
            );
        }
        status
    }

    pub fn meter_snapshot(&self) -> Option<MeterSnapshot> {
        let active = self.active.as_ref()?;
        (active.client_state.active.load(Ordering::Acquire)
            && self.clock.playing.load(Ordering::Acquire))
        .then(|| self.meter.load())
    }

    pub fn queue_slot(&self, slot: usize, command: LoopCommand) -> Result<()> {
        let slot = checked_slot(slot)?;
        if !self.slots[slot].status.loaded {
            bail!("loop slot {} is empty", slot + 1);
        }
        self.slots[slot].control.queue(Some(command));
        Ok(())
    }

    pub fn cancel_slot_queue(&self, slot: usize) -> Result<bool> {
        let slot = checked_slot(slot)?;
        let queued = self.slots[slot].control.queued().is_some();
        self.slots[slot].control.queue(None);
        Ok(queued)
    }

    pub fn command_slot_immediate(&self, slot: usize, command: LoopCommand) -> Result<()> {
        let slot = checked_slot(slot)?;
        if !self.slots[slot].status.loaded {
            bail!("loop slot {} is empty", slot + 1);
        }
        self.slots[slot]
            .control
            .running
            .store(command == LoopCommand::Launch, Ordering::Release);
        self.slots[slot].control.queue(None);
        if command == LoopCommand::Stop {
            self.slots[slot].meter.publish(MeterSnapshot::default());
        }
        Ok(())
    }

    pub fn set_slot_muted(&self, slot: usize, muted: bool) -> Result<()> {
        let slot = checked_slot(slot)?;
        self.slots[slot]
            .control
            .muted
            .store(muted, Ordering::Release);
        if muted {
            self.slots[slot].meter.publish(MeterSnapshot::default());
        }
        Ok(())
    }

    pub fn set_slot_level(&self, slot: usize, level_x1000: u16) -> Result<()> {
        let slot = checked_slot(slot)?;
        self.slots[slot]
            .control
            .level_x1000
            .store(u32::from(level_x1000.min(1_500)), Ordering::Release);
        Ok(())
    }

    pub fn set_slot_filter(&self, slot: usize, filter_x1000: i16) -> Result<()> {
        let slot = checked_slot(slot)?;
        let value = f32::from(filter_x1000.clamp(-1_000, 1_000)) / 1000.0;
        self.slots[slot]
            .control
            .filter_bits
            .store(value.to_bits(), Ordering::Release);
        Ok(())
    }

    pub fn arm_pattern(&self) {
        let generation = self.clock.generation.load(Ordering::Acquire);
        for slot in &self.slots {
            slot.control
                .pattern_generation
                .store(generation, Ordering::Release);
        }
        self.clock.arm_loops();
    }

    #[doc(hidden)]
    pub(crate) fn set_preview_status(&mut self, status: LoopStatus) {
        self.set_slot_preview_status(0, status);
    }

    #[doc(hidden)]
    pub(crate) fn set_slot_preview_status(&mut self, slot: usize, status: LoopStatus) {
        let slot = slot.min(LOOP_SLOTS - 1);
        let running = status.running || status.playing;
        let muted = status.muted;
        let queued = status.queued;
        if status.source_rate > 0 {
            self.slots[slot].position.store(
                (status.elapsed.as_secs_f64() * f64::from(status.source_rate)).round() as u64,
                Ordering::Release,
            );
        }
        self.slots[slot].status = status;
        self.slots[slot]
            .control
            .running
            .store(running, Ordering::Release);
        self.slots[slot]
            .control
            .muted
            .store(muted, Ordering::Release);
        self.slots[slot].control.queue(queued);
        self.preview = true;
        if running {
            self.clock.play(0.0, Bpm::DEFAULT);
            self.arm_pattern();
        }
    }

    #[cfg(test)]
    pub(crate) fn slot_control_values(&self, slot: usize) -> (u16, i16) {
        let source = &self.slots[slot.min(LOOP_SLOTS - 1)].control;
        (
            source.level_x1000.load(Ordering::Acquire) as u16,
            (f32::from_bits(source.filter_bits.load(Ordering::Acquire)) * 1000.0).round() as i16,
        )
    }

    pub fn stop(&self) {
        self.clock.stop();
        for slot in &self.slots {
            slot.control.running.store(false, Ordering::Release);
            slot.control.queue(None);
            slot.meter.publish(MeterSnapshot::default());
        }
        self.clear_meter();
    }

    pub(crate) fn owned_output_ports(&self) -> Option<[String; 2]> {
        self.active
            .as_ref()
            .filter(|active| active.client_state.active.load(Ordering::Acquire))
            .map(|_| configured_output_ports(&self.config))
    }

    #[cfg(test)]
    pub fn unload(&mut self) {
        self.unload_slot(0);
    }

    pub fn unload_slot(&mut self, slot: usize) {
        let Ok(slot) = checked_slot(slot) else {
            return;
        };
        let empty = LoopSlot {
            position: Arc::clone(&self.slots[slot].position),
            meter: Arc::clone(&self.slots[slot].meter),
            ..LoopSlot::default()
        };
        self.slots[slot] = empty;
        let _ = self.rebuild_backend();
        self.preview = false;
    }

    fn rebuild_backend(&mut self) -> Result<()> {
        self.rebuild_backend_mode(None)
    }

    fn rebuild_backend_isolated(&mut self, failed: &mut [bool; LOOP_SLOTS]) -> Result<()> {
        self.rebuild_backend_mode(Some(failed))
    }

    fn rebuild_backend_mode(&mut self, mut failed: Option<&mut [bool; LOOP_SLOTS]>) -> Result<()> {
        if let Some(active) = self.active.as_ref() {
            active.publication.reclaim_retired();
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| !active.client_state.active.load(Ordering::Acquire))
        {
            self.stop_backend();
        }
        self.clear_meter();
        let active_rate = self
            .active
            .as_ref()
            .filter(|active| active.client_state.active.load(Ordering::Acquire))
            .map(|active| active.sample_rate);
        let mut jack =
            if active_rate.is_none() && self.slots.iter().any(|slot| slot.prepared.is_some()) {
                Some(JackClient::open(&self.config.client_name)?)
            } else {
                None
            };
        let jack_rate = active_rate
            .or_else(|| jack.as_ref().map(JackClient::sample_rate))
            .unwrap_or(0);
        for slot in 0..LOOP_SLOTS {
            let Some(source_rate) = self.slots[slot]
                .prepared
                .as_ref()
                .map(|prepared| prepared.source_rate)
            else {
                continue;
            };
            if let Err(error) = require_native_rate(source_rate, jack_rate) {
                if let Some(failed) = failed.as_deref_mut() {
                    self.slots[slot].prepared = None;
                    self.slots[slot].status.error = Some(error.to_string());
                    failed[slot] = true;
                } else {
                    return Err(error);
                }
            }
        }

        let renderers = self.build_renderers()?;
        if active_rate.is_some() {
            let active = self.active.as_ref().expect("active Loop backend");
            active
                .publication
                .publish(RendererSet { slots: renderers })?;
            return Ok(());
        }

        self.stop_backend();
        if self.slots.iter().all(|slot| slot.prepared.is_none()) {
            return Ok(());
        }
        let mut jack = jack.take().context("missing prepared JACK Loop client")?;
        let left = jack.register_audio_port(LOOP_OUTPUT_PORT_NAMES[0], PortDirection::Output)?;
        let right = jack.register_audio_port(LOOP_OUTPUT_PORT_NAMES[1], PortDirection::Output)?;
        let client_state = Arc::new(LoopClientState {
            active: AtomicBool::new(true),
            published_meter: Arc::clone(&self.meter),
            slot_meters: std::array::from_fn(|slot| Arc::clone(&self.slots[slot].meter)),
        });
        let publication = Arc::new(RendererPublication::default());
        let active_renderers = Box::into_raw(Box::new(RendererSet { slots: renderers }));
        let mut callback = Box::new(CallbackData {
            left,
            right,
            port_get_buffer: jack.port_get_buffer(),
            renderer: LoopMixerRenderer {
                active: active_renderers,
                publication: Arc::clone(&publication),
                meter: MeterAccumulator::new(MAX_LOOP_CALLBACK_FRAMES)?,
                client_state: Arc::clone(&client_state),
            },
        });
        // SAFETY: `callback` stays boxed until after JACK is deactivated.
        unsafe {
            jack.set_process_callback(
                process_callback,
                ((&mut *callback) as *mut CallbackData).cast(),
            )?;
            jack.set_shutdown_callback(
                shutdown_callback,
                Arc::as_ptr(&callback.renderer.client_state)
                    .cast_mut()
                    .cast(),
            );
        }
        activate_and_connect(&mut jack, &self.config.outputs, left, right)?;
        self.active = Some(Active {
            jack,
            callback,
            client_state,
            publication,
            sample_rate: jack_rate,
        });
        Ok(())
    }

    fn build_renderers(&self) -> Result<[Option<LoopRenderer>; LOOP_SLOTS]> {
        let mut renderers = std::array::from_fn(|_| None);
        for (slot, renderer) in renderers.iter_mut().enumerate() {
            if let Some(prepared) = self.slots[slot].prepared.as_ref() {
                *renderer = Some(LoopRenderer::new(
                    prepared,
                    Arc::clone(&self.clock),
                    Arc::clone(&self.slots[slot].position),
                    Arc::clone(&self.slots[slot].meter),
                    Arc::clone(&self.slots[slot].control),
                )?);
            }
        }
        Ok(renderers)
    }

    fn stop_backend(&mut self) {
        if let Some(mut active) = self.active.take() {
            active.jack.deactivate();
            active.publication.clear_quiescent();
            // Keep the callback allocation alive until JACK is inactive.
            drop(active.callback);
        }
        self.clear_meter();
    }

    fn clear_meter(&self) {
        self.meter.publish(MeterSnapshot::default());
    }
}

impl Drop for LoopPlayer {
    fn drop(&mut self) {
        self.stop_backend();
    }
}

struct CallbackData {
    left: *mut JackPort,
    right: *mut JackPort,
    port_get_buffer: PortGetBuffer,
    renderer: LoopMixerRenderer,
}

const PUBLICATION_IDLE: u8 = 0;
const PUBLICATION_PENDING: u8 = 1;
const PUBLICATION_SWAPPING: u8 = 2;
const PUBLICATION_RETIRED: u8 = 3;
const PUBLICATION_RECLAIMING: u8 = 4;

struct RendererSet {
    slots: [Option<LoopRenderer>; LOOP_SLOTS],
}

/// One fixed pending renderer set crosses into the callback. The callback only
/// swaps raw pointers and retires the previous fixed set; allocation and
/// destruction both remain on the owner thread.
struct RendererPublication {
    state: AtomicU8,
    pending: AtomicPtr<RendererSet>,
    retired: AtomicPtr<RendererSet>,
}

impl Default for RendererPublication {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(PUBLICATION_IDLE),
            pending: AtomicPtr::new(std::ptr::null_mut()),
            retired: AtomicPtr::new(std::ptr::null_mut()),
        }
    }
}

impl RendererPublication {
    fn publish(&self, next: RendererSet) -> Result<()> {
        self.reclaim_retired();
        if self.state.load(Ordering::Acquire) != PUBLICATION_IDLE {
            bail!("incoming Pattern Loop Mix preparation is late");
        }
        let next = Box::into_raw(Box::new(next));
        self.pending.store(next, Ordering::Release);
        if self
            .state
            .compare_exchange(
                PUBLICATION_IDLE,
                PUBLICATION_PENDING,
                Ordering::Release,
                Ordering::Acquire,
            )
            .is_err()
        {
            let next = self.pending.swap(std::ptr::null_mut(), Ordering::AcqRel);
            if !next.is_null() {
                // SAFETY: publication did not transfer ownership to the
                // callback because the state transition failed.
                unsafe { drop(Box::from_raw(next)) };
            }
            bail!("incoming Pattern Loop Mix publication is busy");
        }
        Ok(())
    }

    #[inline]
    fn adopt_pending(&self, active: &mut *mut RendererSet) {
        if self
            .state
            .compare_exchange(
                PUBLICATION_PENDING,
                PUBLICATION_SWAPPING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return;
        }
        let next = self.pending.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if next.is_null() {
            self.state.store(PUBLICATION_IDLE, Ordering::Release);
            return;
        }
        let previous = std::mem::replace(active, next);
        self.retired.store(previous, Ordering::Release);
        self.state.store(PUBLICATION_RETIRED, Ordering::Release);
    }

    fn reclaim_retired(&self) {
        if self
            .state
            .compare_exchange(
                PUBLICATION_RETIRED,
                PUBLICATION_RECLAIMING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return;
        }
        let retired = self.retired.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !retired.is_null() {
            // SAFETY: the callback published this pointer only after switching
            // away from it, and RECLAIMING prevents another adoption.
            unsafe { drop(Box::from_raw(retired)) };
        }
        self.state.store(PUBLICATION_IDLE, Ordering::Release);
    }

    fn clear_quiescent(&self) {
        let pending = self.pending.swap(std::ptr::null_mut(), Ordering::AcqRel);
        let retired = self.retired.swap(std::ptr::null_mut(), Ordering::AcqRel);
        for pointer in [pending, retired] {
            if !pointer.is_null() {
                // SAFETY: JACK has been deactivated before this cleanup, so no
                // callback can own either publication pointer.
                unsafe { drop(Box::from_raw(pointer)) };
            }
        }
        self.state.store(PUBLICATION_IDLE, Ordering::Release);
    }
}

struct LoopMixerRenderer {
    active: *mut RendererSet,
    publication: Arc<RendererPublication>,
    meter: MeterAccumulator,
    client_state: Arc<LoopClientState>,
}

impl LoopMixerRenderer {
    #[inline]
    fn slots(&self) -> &[Option<LoopRenderer>; LOOP_SLOTS] {
        // SAFETY: `active` is created from a Box, remains owned by this
        // renderer, and changes only in the callback before this borrow.
        unsafe { &(*self.active).slots }
    }

    #[inline]
    fn slots_mut(&mut self) -> &mut [Option<LoopRenderer>; LOOP_SLOTS] {
        // SAFETY: the JACK callback is the only caller while active. Tests own
        // the renderer exclusively.
        unsafe { &mut (*self.active).slots }
    }

    #[inline]
    fn adopt_pending(&mut self) {
        self.publication.adopt_pending(&mut self.active);
    }
}

impl Drop for LoopMixerRenderer {
    fn drop(&mut self) {
        self.publication.clear_quiescent();
        if !self.active.is_null() {
            // SAFETY: callback deactivation precedes CallbackData destruction,
            // and `active` is the one remaining owned RendererSet.
            unsafe { drop(Box::from_raw(self.active)) };
            self.active = std::ptr::null_mut();
        }
    }
}

struct LoopRenderer {
    samples: Arc<Vec<[f32; 2]>>,
    source_rate: u32,
    interpreted_bpm: f64,
    region_start: usize,
    region_len: usize,
    offset_beats: i32,
    meter_beats: u8,
    fade: usize,
    phase: f64,
    seen_generation: u64,
    clock: Arc<TransportClock>,
    position: Arc<AtomicU64>,
    published_meter: Arc<AtomicMeter>,
    meter: MeterAccumulator,
    control: Arc<SlotControl>,
    level: f32,
    filter: f32,
    lowpass: [f32; 2],
    highpass_low: [f32; 2],
    transport_frames: u64,
}

struct LoopClientState {
    active: AtomicBool,
    published_meter: Arc<AtomicMeter>,
    slot_meters: [Arc<AtomicMeter>; LOOP_SLOTS],
}

impl LoopRenderer {
    fn new(
        prepared: &PreparedLoop,
        clock: Arc<TransportClock>,
        position: Arc<AtomicU64>,
        published_meter: Arc<AtomicMeter>,
        control: Arc<SlotControl>,
    ) -> Result<Self> {
        Ok(Self {
            samples: Arc::clone(&prepared.samples),
            source_rate: prepared.source_rate,
            interpreted_bpm: prepared.interpreted_bpm,
            region_start: prepared.region_start,
            region_len: prepared.region_len,
            offset_beats: prepared.offset_beats,
            meter_beats: prepared.meter,
            fade: fade_frames(prepared.source_rate, prepared.region_len),
            phase: prepared.region_start as f64,
            seen_generation: u64::MAX,
            clock,
            position,
            published_meter,
            meter: MeterAccumulator::new(MAX_LOOP_CALLBACK_FRAMES)?,
            control,
            level: 1.0,
            filter: 0.0,
            lowpass: [0.0; 2],
            highpass_low: [0.0; 2],
            transport_frames: 0,
        })
    }
}

fn activate_and_connect(
    jack: &mut JackClient,
    outputs: &[String],
    left: *mut JackPort,
    right: *mut JackPort,
) -> Result<()> {
    let destinations = loop_destinations(outputs)?;
    jack.activate().context("activate JACK loop player")?;
    for (port, destination) in [(left, destinations[0]), (right, destinations[1])] {
        if let Err(error) = jack.connect_port_to_external(port, destination) {
            jack.deactivate();
            return Err(error)
                .with_context(|| format!("connect JACK loop output to {destination}"));
        }
    }
    Ok(())
}

fn loop_destinations(outputs: &[String]) -> Result<[&str; 2]> {
    let [left, right] = outputs else {
        bail!("loop.output requires exactly two JACK destination ports");
    };
    Ok([left, right])
}

unsafe extern "C" fn process_callback(frames: c_uint, argument: *mut c_void) -> c_int {
    let data = unsafe { &mut *(argument.cast::<CallbackData>()) };
    let left = unsafe { (data.port_get_buffer)(data.left, frames) }.cast::<f32>();
    let right = unsafe { (data.port_get_buffer)(data.right, frames) }.cast::<f32>();
    if left.is_null() || right.is_null() {
        clear_mixer_meter(&mut data.renderer);
        return 0;
    }
    let left = unsafe { std::slice::from_raw_parts_mut(left, frames as usize) };
    let right = unsafe { std::slice::from_raw_parts_mut(right, frames as usize) };
    render_mixer_output(&mut data.renderer, left, right);
    0
}

unsafe extern "C" fn shutdown_callback(argument: *mut c_void) {
    let state = unsafe { &*(argument.cast::<LoopClientState>()) };
    state.active.store(false, Ordering::Release);
    state.published_meter.publish(MeterSnapshot::default());
    for meter in &state.slot_meters {
        meter.publish(MeterSnapshot::default());
    }
}

#[inline]
fn clear_mixer_meter(data: &mut LoopMixerRenderer) {
    data.meter.reset();
    for slot in data.slots_mut().iter_mut().flatten() {
        slot.published_meter.publish(MeterSnapshot::default());
    }
    data.client_state
        .published_meter
        .publish(MeterSnapshot::default());
}

#[inline]
fn render_mixer_output(data: &mut LoopMixerRenderer, left: &mut [f32], right: &mut [f32]) {
    data.adopt_pending();
    left.fill(0.0);
    right.fill(0.0);
    if left.len() != right.len()
        || left.len() > MAX_LOOP_CALLBACK_FRAMES
        || !data.client_state.active.load(Ordering::Acquire)
    {
        clear_mixer_meter(data);
        return;
    }
    if data
        .slots()
        .iter()
        .flatten()
        .all(|slot| !slot.clock.playing.load(Ordering::Acquire))
    {
        clear_mixer_meter(data);
        return;
    }
    for slot in data.slots_mut().iter_mut().flatten() {
        render_slot(slot, left, right);
    }
    for (left_out, right_out) in left.iter_mut().zip(right.iter_mut()) {
        let frame = data.meter.process(StereoFrame::new(*left_out, *right_out));
        *left_out = frame.left;
        *right_out = frame.right;
    }
    data.client_state
        .published_meter
        .publish(data.meter.snapshot_and_clear_peak());
}

#[inline]
fn render_slot(data: &mut LoopRenderer, left: &mut [f32], right: &mut [f32]) {
    let generation = data.clock.generation.load(Ordering::Acquire);
    if generation != data.seen_generation {
        data.seen_generation = generation;
        data.transport_frames = 0;
        data.lowpass = [0.0; 2];
        data.highpass_low = [0.0; 2];
        let origin = data.clock.origin_beat.load(Ordering::Acquire) as f64 / BEAT_UNITS;
        let loop_beats =
            data.region_len as f64 * data.interpreted_bpm / (60.0 * f64::from(data.source_rate));
        let beat_phase = loop_phase_from_song(origin, data.offset_beats, loop_beats);
        data.phase = data.region_start as f64 + beat_phase * data.region_len as f64;
        apply_queued_command(data);
    }
    if data.clock.loop_generation.load(Ordering::Acquire) != generation {
        data.meter.reset();
        data.published_meter.publish(MeterSnapshot::default());
        return;
    }
    if data.control.pattern_generation.load(Ordering::Acquire) != generation {
        data.meter.reset();
        data.published_meter.publish(MeterSnapshot::default());
        return;
    }
    let target_level = data.control.level_x1000.load(Ordering::Acquire) as f32 / 1000.0;
    let target_filter =
        f32::from_bits(data.control.filter_bits.load(Ordering::Acquire)).clamp(-1.0, 1.0);
    let smooth = smoothing_coefficient(data.source_rate);
    let end = (data.region_start + data.region_len) as f64;
    let bpm = data.clock.bpm_x100.load(Ordering::Acquire) as f64 / 100.0;
    if (data.interpreted_bpm - bpm).abs() > 0.01 {
        data.control.tempo_fault.store(true, Ordering::Release);
        data.control.running.store(false, Ordering::Release);
        data.meter.reset();
        data.published_meter.publish(MeterSnapshot::default());
        return;
    }
    data.control.tempo_fault.store(false, Ordering::Release);
    let origin = data.clock.origin_beat.load(Ordering::Acquire) as f64 / BEAT_UNITS;
    data.meter.reset();
    for (left_out, right_out) in left.iter_mut().zip(right.iter_mut()) {
        let beat =
            origin + data.transport_frames as f64 * bpm / (60.0 * f64::from(data.source_rate));
        let previous_beat = origin
            + data.transport_frames.saturating_sub(1) as f64 * bpm
                / (60.0 * f64::from(data.source_rate));
        if data.transport_frames == 0
            || (beat / f64::from(data.meter_beats)).floor()
                != (previous_beat / f64::from(data.meter_beats)).floor()
        {
            apply_queued_command(data);
        }
        data.transport_frames = data.transport_frames.saturating_add(1);
        data.level += (target_level - data.level) * smooth;
        data.filter += (target_filter - data.filter) * smooth;
        while data.phase >= end {
            data.phase -= data.region_len as f64;
        }
        let mut sample = render_sample(
            data.samples.as_slice(),
            data.region_start,
            data.region_len,
            data.phase,
            data.fade,
        );
        if !data.control.running.load(Ordering::Acquire)
            || data.control.muted.load(Ordering::Acquire)
        {
            sample = [0.0; 2];
        } else {
            sample = dj_filter(data, sample);
            sample[0] *= data.level;
            sample[1] *= data.level;
        }
        let measured = data.meter.process(StereoFrame::new(sample[0], sample[1]));
        *left_out += measured.left;
        *right_out += measured.right;
        data.phase += 1.0;
    }
    data.published_meter
        .publish(data.meter.snapshot_and_clear_peak());
    data.position.store(
        (data.phase - data.region_start as f64).max(0.0) as u64,
        Ordering::Release,
    );
}

#[inline]
fn apply_queued_command(data: &LoopRenderer) {
    let command = data.control.queued.swap(0, Ordering::AcqRel);
    if command != 0 {
        data.control.running.store(command == 1, Ordering::Release);
    }
}

#[inline]
fn smoothing_coefficient(sample_rate: u32) -> f32 {
    1.0 - (-1.0 / (CONTROL_SMOOTH_SECONDS * sample_rate.max(1) as f32)).exp()
}

#[inline]
fn dj_filter(data: &mut LoopRenderer, input: [f32; 2]) -> [f32; 2] {
    if data.filter.abs() <= FILTER_DEADBAND {
        return input;
    }
    let normalized =
        ((data.filter.abs() - FILTER_DEADBAND) / (1.0 - FILTER_DEADBAND)).clamp(0.0, 1.0);
    let cutoff = if data.filter < 0.0 {
        18_000.0 * (200.0_f32 / 18_000.0).powf(normalized)
    } else {
        20.0 * (5_000.0_f32 / 20.0).powf(normalized)
    };
    let alpha = 1.0
        - (-std::f32::consts::TAU * cutoff / data.source_rate.max(1) as f32)
            .exp()
            .clamp(0.0, 1.0);
    let mut output = [0.0; 2];
    for channel in 0..2 {
        if data.filter < 0.0 {
            data.lowpass[channel] += alpha * (input[channel] - data.lowpass[channel]);
            output[channel] = data.lowpass[channel];
        } else {
            data.highpass_low[channel] += alpha * (input[channel] - data.highpass_low[channel]);
            output[channel] = input[channel] - data.highpass_low[channel];
        }
        if !output[channel].is_finite() {
            output[channel] = 0.0;
            data.lowpass[channel] = 0.0;
            data.highpass_low[channel] = 0.0;
        }
    }
    output
}

fn checked_slot(slot: usize) -> Result<usize> {
    if slot >= LOOP_SLOTS {
        bail!("loop slot must be 1..=4");
    }
    Ok(slot)
}

fn require_compatible_tempo(interpreted_bpm: f64, pattern_bpm: f64) -> Result<()> {
    if (interpreted_bpm - pattern_bpm).abs() > 0.01 {
        bail!(
            "loop is {:.2} BPM but Project is {:.2} BPM; time-stretching is not available",
            interpreted_bpm,
            pattern_bpm
        );
    }
    Ok(())
}

fn require_native_rate(source_rate: u32, jack_rate: u32) -> Result<()> {
    if source_rate != jack_rate {
        bail!(
            "WAV is {source_rate} Hz but JACK is {jack_rate} Hz; restart JACK at {source_rate} Hz for native loop playback"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::allocation_test::assert_no_allocations;
    use crate::sequencer::Song;
    use hound::{SampleFormat, WavSpec, WavWriter};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("shr-loop-{name}-{}-{nanos}", std::process::id()))
    }

    fn quarter_note(bpm: f64) -> Duration {
        Duration::from_secs_f64(60.0 / bpm)
    }

    #[test]
    fn controller_clock_is_exactly_twenty_four_ppqn_at_representative_tempos() {
        for bpm in [20.0, 60.0, 120.0, 173.0, 300.0] {
            let mut phase = ControllerClockPhase::start(Duration::ZERO, bpm);
            let end = quarter_note(bpm);
            let mut pulses = Vec::new();
            while phase.next_tick() < end {
                let at = phase.next_tick();
                assert!(phase.take_due(at));
                pulses.push(at);
            }
            assert_eq!(pulses.len(), 24, "wrong PPQN at {bpm} BPM");
            for pair in pulses.windows(2) {
                let actual = pair[1] - pair[0];
                assert!(actual.abs_diff(controller_clock_interval(bpm)) <= Duration::from_nanos(1));
            }
        }
    }

    #[test]
    fn controller_clock_tempo_change_keeps_phase_and_never_catches_up_in_a_burst() {
        let mut phase = ControllerClockPhase::start(Duration::ZERO, 120.0);
        assert!(phase.take_due(Duration::ZERO));
        let old_next = phase.next_tick();
        let change = old_next / 2;
        phase.tempo(change, 60.0);
        let expected = change + controller_clock_interval(60.0) / 2;
        assert!(phase.next_tick().abs_diff(expected) <= Duration::from_nanos(2));
        let deadline = phase.next_tick();
        assert!(!phase.take_due(deadline - Duration::from_nanos(5)));
        assert!(phase.take_due(deadline));
        assert!(!phase.take_due(deadline));

        let delayed = deadline + Duration::from_secs(2);
        assert!(phase.take_due(delayed));
        assert!(!phase.take_due(delayed));
        assert!(phase.next_tick() > delayed);
    }

    #[test]
    fn controller_clock_is_independent_of_swing_pages_and_destinations() {
        let pulses = |irrelevant_event_offsets: &[Duration]| {
            let mut phase = ControllerClockPhase::start(Duration::ZERO, 100.0);
            let mut result = Vec::new();
            for _ in 0..48 {
                let at = phase.next_tick();
                assert!(phase.take_due(at));
                result.push(at);
            }
            let _ = irrelevant_event_offsets;
            result
        };
        let straight = pulses(&[]);
        let swung_many_destinations = pulses(&[
            Duration::from_millis(17),
            Duration::from_millis(211),
            Duration::from_millis(499),
        ]);
        assert_eq!(straight, swung_many_destinations);
    }

    #[test]
    fn clock_only_protocol_has_no_channel_voice_sysex_continue_or_song_position() {
        let bytes = [
            ControllerClockMessage::TimingClock.bytes(),
            ControllerClockMessage::Start.bytes(),
            ControllerClockMessage::Stop.bytes(),
        ];
        assert_eq!(bytes, [&[0xf8][..], &[0xfa][..], &[0xfc][..]]);
        assert!(bytes
            .iter()
            .flat_map(|message| message.iter())
            .all(|byte| !matches!(byte, 0x80..=0xef | 0xf0 | 0xf2 | 0xfb)));
        let capabilities = controller_clock_source_capabilities();
        assert!(capabilities.contains(PortCap::NO_EXPORT));
        assert!(!capabilities.contains(PortCap::SUBS_READ));
    }

    #[test]
    fn controller_clock_output_uses_one_exact_stable_alsa_port_name() {
        let names = vec![
            "Minilab3:Minilab3 MIDI 32:0".to_owned(),
            "Minilab3:Minilab3 DIN THRU 32:1".to_owned(),
            "Other:Other MIDI 40:0".to_owned(),
        ];
        assert_eq!(
            matching_controller_output_index(&names, "Minilab3:Minilab3 MIDI").unwrap(),
            0
        );
        assert!(matching_controller_output_index(&names, "Minilab3").is_err());
        let ambiguous = vec![
            "Minilab3:Minilab3 MIDI 32:0".to_owned(),
            "Minilab3:Minilab3 MIDI 41:0".to_owned(),
        ];
        assert!(matching_controller_output_index(&ambiguous, "Minilab3:Minilab3 MIDI").is_err());
        assert_eq!(
            alsa_address_from_midir_name(&names[0]).unwrap(),
            Addr {
                client: 32,
                port: 0
            }
        );
        assert!(alsa_address_from_midir_name("Minilab3:Minilab3 MIDI").is_err());
    }

    struct RecordingClockOutput {
        messages: Arc<Mutex<Vec<Vec<u8>>>>,
        fail: bool,
    }

    impl ControllerClockOutput for RecordingClockOutput {
        fn send(&mut self, message: ControllerClockMessage) -> std::result::Result<(), String> {
            if self.fail {
                return Err("offline".into());
            }
            self.messages.lock().unwrap().push(message.bytes().to_vec());
            Ok(())
        }
    }

    #[test]
    fn controller_transport_sends_one_start_and_stop_and_offline_shutdown_joins() {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        let recorded = Arc::clone(&messages);
        let worker = thread::spawn(move || {
            run_controller_clock(
                rx,
                Box::new(RecordingClockOutput {
                    messages: recorded,
                    fail: false,
                }),
                120.0,
            )
        });
        tx.send(ControllerClockCommand::Start(120.0)).unwrap();
        thread::sleep(Duration::from_millis(55));
        tx.send(ControllerClockCommand::Tempo(90.0)).unwrap();
        tx.send(ControllerClockCommand::Stop).unwrap();
        tx.send(ControllerClockCommand::Stop).unwrap();
        thread::sleep(Duration::from_millis(30));
        tx.send(ControllerClockCommand::Shutdown).unwrap();
        worker.join().unwrap();
        let messages = messages.lock().unwrap();
        assert_eq!(
            messages.iter().filter(|m| m.as_slice() == [0xfa]).count(),
            1
        );
        assert_eq!(
            messages.iter().filter(|m| m.as_slice() == [0xfc]).count(),
            1
        );
        assert!(messages.iter().any(|m| m.as_slice() == [0xf8]));
        let start = messages
            .iter()
            .position(|m| m.as_slice() == [0xfa])
            .unwrap();
        let stop = messages
            .iter()
            .position(|m| m.as_slice() == [0xfc])
            .unwrap();
        assert!(messages[..start].iter().any(|m| m.as_slice() == [0xf8]));
        assert!(messages[stop + 1..].iter().any(|m| m.as_slice() == [0xf8]));
        assert!(messages
            .iter()
            .all(|message| matches!(message.as_slice(), [0xf8] | [0xfa] | [0xfc])));

        let (tx, rx) = mpsc::channel();
        let offline = thread::spawn(move || {
            run_controller_clock(
                rx,
                Box::new(RecordingClockOutput {
                    messages: Arc::new(Mutex::new(Vec::new())),
                    fail: true,
                }),
                120.0,
            )
        });
        tx.send(ControllerClockCommand::Start(120.0)).unwrap();
        tx.send(ControllerClockCommand::Stop).unwrap();
        tx.send(ControllerClockCommand::Shutdown).unwrap();
        offline.join().unwrap();

        let messages = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        let recorded = Arc::clone(&messages);
        let shutdown = thread::spawn(move || {
            run_controller_clock(
                rx,
                Box::new(RecordingClockOutput {
                    messages: recorded,
                    fail: false,
                }),
                120.0,
            )
        });
        tx.send(ControllerClockCommand::Start(120.0)).unwrap();
        tx.send(ControllerClockCommand::Shutdown).unwrap();
        shutdown.join().unwrap();
        let messages = messages.lock().unwrap();
        assert_eq!(
            messages.iter().filter(|m| m.as_slice() == [0xfa]).count(),
            1
        );
        assert_eq!(
            messages.iter().filter(|m| m.as_slice() == [0xfc]).count(),
            1
        );
    }

    #[test]
    fn disabled_controller_clock_starts_no_worker_and_sends_nothing() {
        let clock = TransportClock::default();
        assert!(clock.controller_tx.is_none());
        assert!(clock.controller_thread.lock().unwrap().is_none());
        clock.play(0.0, Bpm::from_whole(120).unwrap());
        clock.tempo("90.0".parse().unwrap());
        clock.stop();
    }

    #[test]
    fn external_clock_owner_suspends_all_controller_clock_output_until_internal_play() {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        let recorded = Arc::clone(&messages);
        let worker = thread::spawn(move || {
            run_controller_clock(
                rx,
                Box::new(RecordingClockOutput {
                    messages: recorded,
                    fail: false,
                }),
                120.0,
            )
        });
        let clock = TransportClock {
            playing: AtomicBool::new(false),
            controller_owned: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            loop_generation: AtomicU64::new(u64::MAX),
            origin_beat: AtomicU64::new(0),
            bpm_x100: AtomicU64::new(u64::from(Bpm::DEFAULT.hundredths())),
            controller_tx: Some(tx),
            controller_thread: Mutex::new(Some(worker)),
        };

        clock.play_external(0.0, Bpm::from_whole(120).unwrap());
        thread::sleep(Duration::from_millis(30));
        messages.lock().unwrap().clear();
        clock.tempo(Bpm::from_whole(90).unwrap());
        clock.stop();
        thread::sleep(Duration::from_millis(55));
        assert!(messages.lock().unwrap().is_empty());

        clock.play(0.0, Bpm::from_whole(120).unwrap());
        thread::sleep(Duration::from_millis(30));
        clock.stop();
        thread::sleep(Duration::from_millis(10));
        let messages = messages.lock().unwrap().clone();
        assert_eq!(
            messages.iter().filter(|m| m.as_slice() == [0xfa]).count(),
            1
        );
        assert_eq!(
            messages.iter().filter(|m| m.as_slice() == [0xfc]).count(),
            1
        );
        assert!(messages.iter().any(|m| m.as_slice() == [0xf8]));
    }

    fn test_renderer(samples: Vec<[f32; 2]>) -> (LoopRenderer, Arc<AtomicMeter>) {
        let clock = Arc::new(TransportClock::default());
        let position = Arc::new(AtomicU64::new(0));
        let published_meter = Arc::new(AtomicMeter::default());
        let control = Arc::new(SlotControl::new(None));
        control.running.store(true, Ordering::Release);
        (
            LoopRenderer {
                samples: Arc::new(samples.clone()),
                source_rate: 48_000,
                interpreted_bpm: 120.0,
                region_start: 0,
                region_len: samples.len(),
                offset_beats: 0,
                meter_beats: 4,
                fade: 0,
                phase: 0.0,
                seen_generation: u64::MAX,
                clock,
                position,
                meter: MeterAccumulator::new(4).unwrap(),
                published_meter: Arc::clone(&published_meter),
                control,
                level: 1.0,
                filter: 0.0,
                lowpass: [0.0; 2],
                highpass_low: [0.0; 2],
                transport_frames: 0,
            },
            published_meter,
        )
    }

    fn render_output(renderer: &mut LoopRenderer, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        if left.len() != right.len()
            || left.len() > MAX_LOOP_CALLBACK_FRAMES
            || !renderer.clock.playing.load(Ordering::Acquire)
        {
            renderer.published_meter.publish(MeterSnapshot::default());
            return;
        }
        render_slot(renderer, left, right);
    }

    fn play_test_renderer(renderer: &LoopRenderer) {
        renderer.clock.play(0.0, Bpm::from_whole(120).unwrap());
        renderer.control.pattern_generation.store(
            renderer.clock.generation.load(Ordering::Acquire),
            Ordering::Release,
        );
        renderer.clock.arm_loops();
    }

    fn test_mixer(samples: [[[f32; 2]; 4]; LOOP_SLOTS]) -> LoopMixerRenderer {
        let mut renderers = Vec::new();
        let mut slot_meters = Vec::new();
        for samples in samples {
            let (renderer, meter) = test_renderer(samples.to_vec());
            play_test_renderer(&renderer);
            renderers.push(renderer);
            slot_meters.push(meter);
        }
        let published_meter = Arc::new(AtomicMeter::default());
        let client_state = Arc::new(LoopClientState {
            active: AtomicBool::new(true),
            published_meter: Arc::clone(&published_meter),
            slot_meters: std::array::from_fn(|slot| Arc::clone(&slot_meters[slot])),
        });
        let mut renderers = renderers.into_iter();
        let active = Box::into_raw(Box::new(RendererSet {
            slots: std::array::from_fn(|_| Some(renderers.next().unwrap())),
        }));
        LoopMixerRenderer {
            active,
            publication: Arc::new(RendererPublication::default()),
            meter: MeterAccumulator::new(4).unwrap(),
            client_state,
        }
    }

    #[test]
    fn bpm_interpretations_and_musical_frame_math() {
        assert_eq!(bpm_candidates(120.0), [60.0, 120.0, 240.0]);
        assert_eq!(beat_to_frame(4.0, 120.0, 48_000), 96_000);
        assert_eq!(bar_to_beat(2, 3), 6);
        assert_eq!(fade_frames(48_000, 100), 25);
        assert_eq!(fade_frames(48_000, 48_000), 240);
    }

    #[test]
    fn order_and_row_convert_to_absolute_beats() {
        let config = crate::config::RuntimeConfig::default().external_midi;
        let mut song = Song::new(&config);
        let setup = song.patterns[&0].clone();
        song.patterns
            .insert(1, crate::sequencer::Pattern::empty_like_setup(8, &setup));
        song.order = vec![0, 1];
        assert_eq!(song_position_beats(&song, 1, 4), 17.0);
    }

    #[test]
    fn auto_alignment_estimates_pulses_and_snaps_to_bars() {
        let sample_rate = 48_000;
        let mut samples = vec![[0.0, 0.0]; sample_rate as usize * 2];
        for beat in 0..4 {
            let start = beat * 24_000;
            for frame in &mut samples[start..start + 512] {
                *frame = [1.0, 1.0];
            }
        }
        let decoded = DecodedLoop {
            samples,
            sample_rate,
            channels: 1,
        };
        let alignment = analyze_alignment(&decoded, Bpm::from_whole(90).unwrap(), 4);
        assert!(alignment.transient_detected);
        assert_eq!(alignment.length_beats, 4);
        assert_eq!(alignment.bars, 1);
        assert!((alignment.source_bpm - 120.0).abs() < 0.01);
    }

    #[test]
    fn auto_alignment_falls_back_to_pattern_tempo_for_flat_audio() {
        let decoded = DecodedLoop {
            samples: vec![[0.0, 0.0]; 48_000 * 3],
            sample_rate: 48_000,
            channels: 2,
        };
        let alignment = analyze_alignment(&decoded, Bpm::from_whole(100).unwrap(), 3);
        assert!(!alignment.transient_detected);
        assert_eq!(alignment.length_beats, 6);
        assert_eq!(alignment.bars, 2);
        assert!((alignment.source_bpm - 120.0).abs() < 0.01);
    }

    #[test]
    fn song_phase_accounts_for_bar_offsets() {
        assert_eq!(loop_phase_from_song(0.0, 0, 16.0), 0.0);
        assert_eq!(loop_phase_from_song(4.0, 4, 16.0), 0.0);
        assert_eq!(loop_phase_from_song(0.0, 4, 16.0), 0.75);
        assert_eq!(loop_phase_from_song(0.0, -4, 16.0), 0.25);
    }

    #[test]
    fn mono_and_stereo_decode_and_malformed_files_are_safe() {
        let base = temp_dir("decode");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let mono = base.join("mono.wav");
        let mut writer = WavWriter::create(
            &mono,
            WavSpec {
                channels: 1,
                sample_rate: 44_100,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
        )
        .unwrap();
        writer.write_sample::<i16>(16_384).unwrap();
        writer.write_sample::<i16>(-16_384).unwrap();
        writer.finalize().unwrap();
        let decoded = DecodedLoop::open(&mono).unwrap();
        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.samples[0], [0.5, 0.5]);

        let stereo = base.join("stereo.wav");
        let mut writer = WavWriter::create(
            &stereo,
            WavSpec {
                channels: 2,
                sample_rate: 44_100,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
        )
        .unwrap();
        writer.write_sample::<i16>(8192).unwrap();
        writer.write_sample::<i16>(-8192).unwrap();
        writer.finalize().unwrap();
        let decoded = DecodedLoop::open(&stereo).unwrap();
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.samples[0], [0.25, -0.25]);

        let bad = base.join("bad.wav");
        fs::write(&bad, b"not a wave").unwrap();
        assert!(DecodedLoop::open(&bad).is_err());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn decoded_loop_frame_limit_is_explicit_and_bounded() {
        assert_eq!(
            checked_loop_frames(MAX_DECODED_LOOP_FRAMES).unwrap(),
            MAX_DECODED_LOOP_FRAMES as usize
        );
        assert!(checked_loop_frames(MAX_DECODED_LOOP_FRAMES + 1)
            .unwrap_err()
            .to_string()
            .contains("safe loop limit"));
    }

    #[test]
    fn import_copies_wavs_to_private_storage_without_replacing_existing_files() {
        let base = temp_dir("import");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let source = base.join("My Loop!.wav");
        let mut writer = WavWriter::create(
            &source,
            WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
        )
        .unwrap();
        writer.write_sample::<i16>(0).unwrap();
        writer.finalize().unwrap();
        let destination = base.join("private");

        let (first, decoded) = import(&source, &destination).unwrap();
        let (second, _) = import(&source, &destination).unwrap();

        assert_eq!(decoded.sample_rate, 48_000);
        assert!(first.starts_with(&destination));
        assert!(second.starts_with(&destination));
        assert_ne!(first, second);
        assert!(first.exists());
        assert!(second.exists());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn listing_ignores_directories_and_symlinks_named_like_wavs() {
        let base = temp_dir("list");
        fs::create_dir_all(base.join("directory.wav")).unwrap();
        fs::write(base.join("real.wav"), []).unwrap();
        std::os::unix::fs::symlink(base.join("real.wav"), base.join("alias.wav")).unwrap();

        assert_eq!(list_wavs(&base), [base.join("real.wav")]);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn invalid_decoded_loop_is_rejected_before_opening_jack() {
        let config = crate::config::RuntimeConfig::default().loop_player;
        let mut player = LoopPlayer::new(&config, Arc::new(TransportClock::default()));
        let settings = LoopSettings {
            file: "empty.wav".into(),
            source_bpm_x100: 12_000,
            interpretation: crate::sequencer::BpmInterpretation::Normal,
            start_beat: 0,
            length_beats: 4,
            offset_beats: 0,
            level_x1000: 1000,
            filter_x1000: 0,
        };

        let error = player
            .load(
                DecodedLoop {
                    samples: Vec::new(),
                    sample_rate: 48_000,
                    channels: 2,
                },
                &settings,
            )
            .unwrap_err();
        assert!(error.to_string().contains("invalid decoded WAV loop"));
    }

    #[test]
    fn decimal_native_tempo_preparation_keeps_native_frames_and_pitch() {
        let tempo = "100.50".parse::<Bpm>().unwrap();
        let frames = beat_to_frame(4.0, tempo.as_f64(), 48_000);
        let decoded = DecodedLoop {
            samples: vec![[0.25, -0.25]; frames],
            sample_rate: 48_000,
            channels: 2,
        };
        let settings = LoopSettings::new(
            "decimal.wav".into(),
            10_050,
            crate::sequencer::BpmInterpretation::Normal,
            0,
            4,
            0,
        );
        let prepared = prepare_loop_slot(decoded, &settings, tempo.as_f64(), 4).unwrap();
        let audio = prepared.prepared.unwrap();
        assert_eq!(audio.interpreted_bpm, 100.50);
        assert_eq!(audio.region_start, 0);
        assert_eq!(audio.region_len, frames);
        assert_eq!(audio.samples.len(), frames);
    }

    #[test]
    fn transport_clock_tracks_play_restart_tempo_and_stop() {
        let clock = TransportClock::default();
        clock.play(3.5, Bpm::from_whole(120).unwrap());
        assert!(clock.playing.load(Ordering::Acquire));
        assert_eq!(clock.origin_beat.load(Ordering::Acquire), 3_500_000);
        let first_generation = clock.generation.load(Ordering::Acquire);

        clock.tempo("150.25".parse().unwrap());
        clock.play(1.0, Bpm::from_whole(90).unwrap());
        assert!(clock.generation.load(Ordering::Acquire) > first_generation);
        assert_eq!(clock.origin_beat.load(Ordering::Acquire), 1_000_000);

        clock.stop();
        assert!(!clock.playing.load(Ordering::Acquire));
    }

    #[test]
    fn native_sample_rendering_wraps_with_bounded_fades() {
        let data = [[1.0, 1.0], [0.5, 0.5], [-1.0, -1.0], [0.0, 0.0]];
        assert_eq!(render_sample(&data, 0, 4, 0.0, 1), [0.0, 0.0]);
        assert!((render_sample(&data, 0, 4, 1.5, 1)[0] + 0.25).abs() < 0.0001);
        assert!((render_sample(&data, 0, 4, 4.5, 1)[0] - 0.375).abs() < 0.0001);
        assert!(fade_frames(48_000, 4) <= 1);
    }

    #[test]
    fn loop_callback_meter_accumulates_publishes_and_separates_stereo() {
        let (mut renderer, published) = test_renderer(vec![[0.5, 0.25]; 4]);
        play_test_renderer(&renderer);
        let mut left = [0.0; 4];
        let mut right = [0.0; 4];

        assert_no_allocations(|| render_output(&mut renderer, &mut left, &mut right));

        assert_eq!(left, [0.0, 0.5, 0.5, 0.5]);
        assert_eq!(right, [0.0, 0.25, 0.25, 0.25]);
        let snapshot = published.load();
        assert_eq!(snapshot.peak, StereoFrame::new(0.5, 0.25));
        assert!((snapshot.rms.left - 0.433_012_7).abs() < 0.000_001);
        assert!((snapshot.rms.right - 0.216_506_35).abs() < 0.000_001);
    }

    #[test]
    fn four_slot_callback_sums_each_stereo_source_once_without_allocating() {
        let mut mixer = test_mixer([
            [[0.05, 0.01]; 4],
            [[0.10, 0.02]; 4],
            [[0.15, 0.03]; 4],
            [[0.20, 0.04]; 4],
        ]);
        let mut left = [9.0; 4];
        let mut right = [9.0; 4];
        assert_no_allocations(|| render_mixer_output(&mut mixer, &mut left, &mut right));
        assert_eq!(left[0], 0.0);
        assert_eq!(right[0], 0.0);
        for frame in 1..4 {
            assert!((left[frame] - 0.50).abs() < 0.000_01);
            assert!((right[frame] - 0.10).abs() < 0.000_01);
        }
        let peak = mixer.client_state.published_meter.load().peak;
        assert!((peak.left - 0.5).abs() < 0.000_01);
        assert!((peak.right - 0.1).abs() < 0.000_01);

        mixer.slots_mut()[2]
            .as_ref()
            .unwrap()
            .control
            .muted
            .store(true, Ordering::Release);
        render_mixer_output(&mut mixer, &mut left, &mut right);
        assert!((left[1] - 0.35).abs() < 0.000_01);
        assert!((right[1] - 0.07).abs() < 0.000_01);
    }

    #[test]
    fn loop_commands_replace_cancel_and_apply_deterministically_at_a_boundary() {
        let (renderer, _) = test_renderer(vec![[0.2, 0.2]; 4]);
        renderer.control.running.store(false, Ordering::Release);
        renderer.control.queue(Some(LoopCommand::Launch));
        renderer.control.queue(Some(LoopCommand::Stop));
        assert_eq!(renderer.control.queued(), Some(LoopCommand::Stop));
        renderer.control.queue(None);
        assert_eq!(renderer.control.queued(), None);

        renderer.control.queue(Some(LoopCommand::Launch));
        apply_queued_command(&renderer);
        assert!(renderer.control.running.load(Ordering::Acquire));
        assert_eq!(renderer.control.queued(), None);
        renderer.control.queue(Some(LoopCommand::Stop));
        apply_queued_command(&renderer);
        assert!(!renderer.control.running.load(Ordering::Acquire));

        let (mut boundary, _) = test_renderer(vec![[0.2, 0.2]; 64]);
        boundary.source_rate = 20;
        boundary.meter_beats = 1;
        play_test_renderer(&boundary);
        let mut left = [0.0; 4];
        let mut right = [0.0; 4];
        render_output(&mut boundary, &mut left, &mut right);
        boundary.control.queue(Some(LoopCommand::Stop));
        render_output(&mut boundary, &mut left, &mut right);
        assert!(boundary.control.running.load(Ordering::Acquire));
        render_output(&mut boundary, &mut left, &mut right);
        assert!(!boundary.control.running.load(Ordering::Acquire));
        assert!(left[2..].iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn level_and_dj_filter_are_smoothed_bounded_neutral_and_finite() {
        let alternating = (0..4096)
            .map(|index| {
                let sample = if index % 2 == 0 { 0.5 } else { -0.5 };
                [sample, -sample]
            })
            .collect::<Vec<_>>();
        let (mut neutral, _) = test_renderer(alternating.clone());
        play_test_renderer(&neutral);
        neutral.filter = 0.0;
        neutral
            .control
            .filter_bits
            .store(0.02_f32.to_bits(), Ordering::Release);
        neutral.control.level_x1000.store(0, Ordering::Release);
        let mut left = vec![0.0; 4096];
        let mut right = vec![0.0; 4096];
        render_output(&mut neutral, &mut left, &mut right);
        assert!(left.iter().all(|sample| sample.is_finite()));
        assert!(left[2].abs() > left[4095].abs());
        assert!(left[2].abs() < 0.5, "level change must be smoothed");

        let (mut lowpass, _) = test_renderer(alternating);
        play_test_renderer(&lowpass);
        lowpass.filter = -1.0;
        lowpass
            .control
            .filter_bits
            .store((-1.0_f32).to_bits(), Ordering::Release);
        render_output(&mut lowpass, &mut left, &mut right);
        let tail_peak = left[2048..]
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(tail_peak < 0.1, "low-pass must attenuate alternating highs");

        let (mut highpass, _) = test_renderer(vec![[0.4, -0.4]; 4096]);
        play_test_renderer(&highpass);
        highpass.filter = 1.0;
        highpass
            .control
            .filter_bits
            .store(1.0_f32.to_bits(), Ordering::Release);
        render_output(&mut highpass, &mut left, &mut right);
        assert!(left.iter().all(|sample| sample.is_finite()));
        assert!(left[4095].abs() < left[2].abs());
    }

    #[test]
    fn tempo_rate_and_phase_contracts_refuse_drift_but_allow_different_bar_lengths() {
        assert!(require_compatible_tempo(120.0, 120.0).is_ok());
        assert!(require_compatible_tempo(120.02, 120.0)
            .unwrap_err()
            .to_string()
            .contains("time-stretching is not available"));
        assert!(require_native_rate(48_000, 44_100).is_err());
        for loop_beats in [4.0, 8.0, 16.0, 32.0] {
            assert_eq!(loop_phase_from_song(loop_beats, 0, loop_beats), 0.0);
            assert_eq!(loop_phase_from_song(loop_beats + 4.0, 4, loop_beats), 0.0);
        }
    }

    #[test]
    fn stopped_silent_and_restarted_loop_cannot_leave_stale_meter_levels() {
        let (mut renderer, published) = test_renderer(vec![[0.8, 0.4]; 4]);
        play_test_renderer(&renderer);
        let mut left = [0.0; 4];
        let mut right = [0.0; 4];
        render_output(&mut renderer, &mut left, &mut right);
        assert_eq!(published.load().peak, StereoFrame::new(0.8, 0.4));

        renderer.clock.stop();
        render_output(&mut renderer, &mut left, &mut right);
        assert_eq!(left, [0.0; 4]);
        assert_eq!(right, [0.0; 4]);
        assert_eq!(published.load(), MeterSnapshot::default());

        published.publish(MeterSnapshot {
            peak: StereoFrame::new(0.6, 0.3),
            ..MeterSnapshot::default()
        });
        published.publish(MeterSnapshot::default());
        assert_eq!(published.load(), MeterSnapshot::default());
        left.fill(1.0);
        right.fill(1.0);
        render_output(&mut renderer, &mut left, &mut right);
        assert_eq!(left, [0.0; 4]);
        assert_eq!(right, [0.0; 4]);
        assert_eq!(published.load(), MeterSnapshot::default());

        renderer.samples = Arc::new(vec![[0.1, 0.2]; 4]);
        play_test_renderer(&renderer);
        render_output(&mut renderer, &mut left, &mut right);
        assert_eq!(published.load().peak, StereoFrame::new(0.1, 0.2));
    }

    #[test]
    fn unloaded_failed_and_oversized_loop_states_clear_meter_availability() {
        let config = crate::config::RuntimeConfig::default().loop_player;
        let mut player = LoopPlayer::new(&config, Arc::new(TransportClock::default()));
        player.meter.publish(MeterSnapshot {
            peak: StereoFrame::new(0.9, 0.7),
            ..MeterSnapshot::default()
        });
        player.unload();
        assert!(player.meter_snapshot().is_none());
        assert_eq!(player.meter.load(), MeterSnapshot::default());

        let settings = LoopSettings {
            file: "empty.wav".into(),
            source_bpm_x100: 12_000,
            interpretation: crate::sequencer::BpmInterpretation::Normal,
            start_beat: 0,
            length_beats: 4,
            offset_beats: 0,
            level_x1000: 1000,
            filter_x1000: 0,
        };
        assert!(player
            .load(
                DecodedLoop {
                    samples: Vec::new(),
                    sample_rate: 48_000,
                    channels: 2,
                },
                &settings,
            )
            .is_err());
        assert!(player.meter_snapshot().is_none());
        assert_eq!(player.meter.load(), MeterSnapshot::default());

        let (mut renderer, published) = test_renderer(vec![[0.5, 0.5]; 4]);
        play_test_renderer(&renderer);
        let mut left = vec![1.0; MAX_LOOP_CALLBACK_FRAMES + 1];
        let mut right = vec![1.0; MAX_LOOP_CALLBACK_FRAMES + 1];
        render_output(&mut renderer, &mut left, &mut right);
        assert!(left.iter().all(|sample| *sample == 0.0));
        assert!(right.iter().all(|sample| *sample == 0.0));
        assert_eq!(published.load(), MeterSnapshot::default());
    }

    #[test]
    fn pattern_boundary_silences_outgoing_until_incoming_generation_is_armed() {
        let (mut renderer, published) = test_renderer(vec![[0.6, 0.3]; 16]);
        play_test_renderer(&renderer);
        let mut left = [0.0; 4];
        let mut right = [0.0; 4];
        render_output(&mut renderer, &mut left, &mut right);
        assert!(left.iter().any(|sample| *sample != 0.0));

        renderer.clock.restart_cycle(0.0);
        render_output(&mut renderer, &mut left, &mut right);
        assert_eq!(left, [0.0; 4]);
        assert_eq!(right, [0.0; 4]);
        assert_eq!(published.load(), MeterSnapshot::default());

        renderer.clock.arm_loops();
        render_output(&mut renderer, &mut left, &mut right);
        assert_eq!(
            left, [0.0; 4],
            "arming the new transport generation must not re-authorize an outgoing renderer"
        );

        renderer.control.pattern_generation.store(
            renderer.clock.generation.load(Ordering::Acquire),
            Ordering::Release,
        );
        renderer.clock.arm_loops();
        render_output(&mut renderer, &mut left, &mut right);
        assert_eq!(left[1], 0.6);
        assert_eq!(right[1], 0.3);
        assert_eq!(renderer.transport_frames, 4);
    }

    #[test]
    fn pattern_preparation_rebuilds_identical_wav_references_with_each_patterns_settings() {
        let decoded = DecodedLoop {
            samples: vec![[0.25, -0.25]; 8_000],
            sample_rate: 8_000,
            channels: 2,
        };
        let first = LoopSettings::new(
            "shared.wav".into(),
            12_000,
            crate::sequencer::BpmInterpretation::Normal,
            0,
            1,
            0,
        );
        let second = LoopSettings {
            start_beat: 1,
            offset_beats: 4,
            level_x1000: 600,
            filter_x1000: -500,
            ..first.clone()
        };
        let first_slot = prepare_loop_slot(decoded.clone(), &first, 120.0, 4).unwrap();
        let second_slot = prepare_loop_slot(decoded, &second, 120.0, 3).unwrap();
        let first_prepared = first_slot.prepared.unwrap();
        let second_prepared = second_slot.prepared.unwrap();

        assert_eq!(first_prepared.region_start, 0);
        assert_eq!(second_prepared.region_start, 4_000);
        assert_eq!(first_prepared.offset_beats, 0);
        assert_eq!(second_prepared.offset_beats, 4);
        assert_eq!(first_prepared.meter, 4);
        assert_eq!(second_prepared.meter, 3);
        assert_eq!(second_slot.control.level_x1000.load(Ordering::Acquire), 600);
        assert_eq!(
            f32::from_bits(second_slot.control.filter_bits.load(Ordering::Acquire)),
            -0.5
        );
    }

    #[test]
    fn tracker_tempo_incompatibility_faults_only_the_affected_loop_renderer() {
        let (mut incompatible, _) = test_renderer(vec![[0.5, 0.25]; 16]);
        let (mut healthy, _) = test_renderer(vec![[0.2, 0.1]; 16]);
        play_test_renderer(&incompatible);
        play_test_renderer(&healthy);
        incompatible.clock.tempo("121.0".parse().unwrap());
        let mut left = [0.0; 4];
        let mut right = [0.0; 4];
        render_output(&mut incompatible, &mut left, &mut right);
        assert_eq!(left, [0.0; 4]);
        assert!(incompatible.control.tempo_fault.load(Ordering::Acquire));
        assert!(!incompatible.control.running.load(Ordering::Acquire));

        render_output(&mut healthy, &mut left, &mut right);
        assert!(left.iter().any(|sample| *sample != 0.0));
        assert!(!healthy.control.tempo_fault.load(Ordering::Acquire));
    }

    #[test]
    fn pattern_preparation_shape_is_fixed_to_four_slots_and_one_callback_sum() {
        let source = include_str!("loop_player.rs");
        assert!(source.contains("inputs: [Option<(DecodedLoop, LoopSettings)>; LOOP_SLOTS]"));
        assert!(source.contains("slots: [Option<LoopRenderer>; LOOP_SLOTS]"));
        assert!(source.contains("renderer: LoopMixerRenderer"));
        let callback = source
            .split("unsafe extern \"C\" fn process_callback")
            .nth(1)
            .unwrap()
            .split("unsafe extern \"C\" fn shutdown_callback")
            .next()
            .unwrap();
        for forbidden in [
            "Mutex",
            "File::",
            "DecodedLoop",
            "format!",
            "Vec::",
            "Box::",
            "loop {",
        ] {
            assert!(
                !callback.contains(forbidden),
                "callback contains forbidden work: {forbidden}"
            );
        }
    }

    #[test]
    fn fixed_pending_pattern_publication_swaps_and_reclaims_outside_the_callback() {
        let mut mixer = test_mixer([[[0.1, 0.1]; 4]; LOOP_SLOTS]);
        let slots = std::array::from_fn(|slot| {
            let (renderer, _) = test_renderer(vec![[0.2 + slot as f32 * 0.1; 2]; 4]);
            play_test_renderer(&renderer);
            Some(renderer)
        });
        mixer.publication.publish(RendererSet { slots }).unwrap();
        assert_eq!(
            mixer.publication.state.load(Ordering::Acquire),
            PUBLICATION_PENDING
        );

        let mut left = [0.0; 4];
        let mut right = [0.0; 4];
        assert_no_allocations(|| render_mixer_output(&mut mixer, &mut left, &mut right));
        assert!((left[1] - 1.4).abs() < 0.000_01);
        assert!((right[1] - 1.4).abs() < 0.000_01);
        assert_eq!(
            mixer.publication.state.load(Ordering::Acquire),
            PUBLICATION_RETIRED
        );

        mixer.publication.reclaim_retired();
        assert_eq!(
            mixer.publication.state.load(Ordering::Acquire),
            PUBLICATION_IDLE
        );
    }

    #[test]
    fn loop_meter_keeps_the_existing_owned_stereo_route() {
        let config = crate::config::RuntimeConfig::default().loop_player;
        assert_eq!(LOOP_OUTPUT_PORT_NAMES, ["output_l", "output_r"]);
        let destinations = loop_destinations(&config.outputs).unwrap();
        assert_eq!(
            destinations,
            [config.outputs[0].as_str(), config.outputs[1].as_str()]
        );
        assert!(loop_destinations(&config.outputs[..1]).is_err());
    }

    #[test]
    fn native_loop_playback_requires_matching_jack_rate() {
        assert!(require_native_rate(44_100, 44_100).is_ok());
        assert!(require_native_rate(44_100, 48_000)
            .unwrap_err()
            .to_string()
            .contains("restart JACK at 44100 Hz"));
    }

    #[test]
    fn loop_library_lists_only_regular_wavs_and_saved_references() {
        let base = temp_dir("library-list");
        let loops = base.join("loops");
        let projects = base.join("projects");
        fs::create_dir_all(&loops).unwrap();
        fs::write(loops.join("free.wav"), b"private").unwrap();
        fs::write(loops.join("used.wav"), b"private").unwrap();
        std::os::unix::fs::symlink(loops.join("free.wav"), loops.join("alias.wav")).unwrap();

        let mut song = Song::new(&crate::config::RuntimeConfig::default().external_midi);
        song.name = "saved".into();
        song.patterns.get_mut(&0).unwrap().audio_loops[0] = Some(LoopSettings {
            file: "used.wav".into(),
            source_bpm_x100: 12_000,
            interpretation: crate::sequencer::BpmInterpretation::Normal,
            start_beat: 0,
            length_beats: 4,
            offset_beats: 0,
            level_x1000: 1000,
            filter_x1000: 0,
        });
        crate::sequencer::save(&projects, &song, false).unwrap();

        let entries = library_entries(&loops, None, &projects).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|entry| entry.file == "free.wav"));
        assert!(!entries.iter().any(|entry| entry.file == "alias.wav"));
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.file == "used.wav")
                .unwrap()
                .saved_references,
            1
        );
        let _ = fs::remove_dir_all(base);
    }
}
