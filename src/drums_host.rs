//! SHR-owned JACK host for the in-process drum library.
//!
//! Package loading, validation, sample decoding, and engine preparation happen
//! before activation. The JACK callback only drains a bounded lock-free event
//! queue and renders into preallocated engine state.

use crate::config::DrumEngineConfig;
use crate::dsp::StereoFrame as EffectFrame;
use crate::effects::EffectSlot;
use crate::jack::{Client as JackClient, Port as JackPort, PortDirection, PortGetBuffer};
use crate::tempo::Bpm;
use anyhow::{bail, Context, Result};
use libc::{c_int, c_uint, c_void};
use shr_drums::{
    event_queue, load_package, DrumEngine, DrumEvent, EventSender, KitManifest, KitTuning,
    MusicalMode, PitchClass, ProjectKey, StereoFrame,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub type SharedDrumOutput = Arc<Mutex<Option<EventSender>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KitEntry {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

pub fn discover_kits(directory: &Path) -> Vec<KitEntry> {
    let mut kits = fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let path = entry.path();
            let bytes = fs::read(path.join("manifest.json")).ok()?;
            let manifest = KitManifest::from_json(&bytes).ok()?;
            Some(KitEntry {
                id: manifest.kit_id,
                name: manifest.display_name,
                path,
            })
        })
        .collect::<Vec<_>>();
    kits.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    kits.dedup_by(|left, right| left.id == right.id);
    kits
}

struct CallbackData {
    engine: DrumEngine,
    effects: DrumEffectStack,
    tempo_bpm: Arc<AtomicU32>,
    applied_tempo_bits: u32,
    output_left: *mut JackPort,
    output_right: *mut JackPort,
    port_get_buffer: PortGetBuffer,
    maximum_frames: usize,
    lost: AtomicBool,
}

struct DrumEffectStack {
    slots: Vec<EffectSlot>,
}

impl DrumEffectStack {
    fn compile(
        rack: &crate::audio_graph::InsertRack,
        sample_rate: u32,
        meter_window: usize,
        tempo: Bpm,
    ) -> Result<Self> {
        crate::audio_graph::validate_drum_rack(rack)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let mut slots = Vec::with_capacity(crate::audio_graph::DRUM_EFFECT_COUNT);
        for id in &rack.order {
            let mut effect = rack.effect(*id).expect("validated drum rack order").clone();
            if effect.kind == crate::audio_graph::EffectKind::Delay {
                effect
                    .parameters
                    .insert("tempo_bpm".into(), tempo.as_f64() as f32);
            }
            slots.push(
                EffectSlot::compile(&effect, sample_rate, meter_window)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            );
        }
        let stack = Self { slots };
        if stack.memory_bytes() > crate::audio_graph::MAX_EFFECT_MEMORY_BYTES {
            bail!("SHR Drums effect memory bound exceeded");
        }
        Ok(stack)
    }

    fn set_tempo(&mut self, tempo: f32) {
        for slot in &mut self.slots {
            if slot.kind() == crate::audio_graph::EffectKind::Delay {
                let _ = slot.set_parameter("tempo_bpm", tempo);
            }
        }
    }

    fn process(&mut self, frames: &mut [EffectFrame]) {
        for slot in &mut self.slots {
            slot.process(frames);
        }
        const CEILING: f32 = 0.891_250_9;
        for frame in frames {
            *frame = frame.finite_or_silence();
            frame.left = frame.left.clamp(-CEILING, CEILING);
            frame.right = frame.right.clamp(-CEILING, CEILING);
        }
    }

    fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.reset();
        }
    }

    fn memory_bytes(&self) -> usize {
        self.slots.iter().map(EffectSlot::memory_bytes).sum()
    }
}

unsafe impl Send for CallbackData {}

pub struct DrumHost {
    jack: JackClient,
    callback: Box<CallbackData>,
    sender: EventSender,
    shared: SharedDrumOutput,
    output_ports: [String; 2],
    project_key: crate::scale::Scale,
    tuning: KitTuning,
    drum_rack: crate::audio_graph::InsertRack,
    tempo_bpm: Arc<AtomicU32>,
}

impl DrumHost {
    pub fn start(
        config: &DrumEngineConfig,
        kit: &KitEntry,
        project_key: crate::scale::Scale,
        tuning: &KitTuning,
        drum_rack: &crate::audio_graph::InsertRack,
        tempo: Bpm,
        destinations: &[String],
        shared: SharedDrumOutput,
    ) -> Result<Self> {
        if destinations.len() != 2 {
            bail!("SHR Drums requires exactly two resolved playback destinations");
        }
        let mut jack = JackClient::open(&config.client_name).context("open SHR Drums JACK host")?;
        let sample_rate = jack.sample_rate();
        let key = ProjectKey {
            tonic: PitchClass(project_key.root),
            mode: match project_key.kind {
                crate::scale::ScaleKind::Major => MusicalMode::Major,
                crate::scale::ScaleKind::NaturalMinor => MusicalMode::NaturalMinor,
            },
        };
        let prepared = load_package(&kit.path, key, tuning).map_err(anyhow::Error::msg)?;
        let (sender, receiver) = event_queue();
        let engine = DrumEngine::new(sample_rate, prepared, receiver)
            .map_err(anyhow::Error::msg)
            .context("prepare SHR Drums engine")?;
        let effects = DrumEffectStack::compile(
            drum_rack,
            sample_rate,
            config.maximum_callback_frames,
            tempo,
        )
        .context("prepare SHR Drums effects")?;
        let tempo_bits = (tempo.as_f64() as f32).to_bits();
        let tempo_bpm = Arc::new(AtomicU32::new(tempo_bits));
        let output_left = jack.register_audio_port("out_l", PortDirection::Output)?;
        let output_right = jack.register_audio_port("out_r", PortDirection::Output)?;
        let output_ports = [
            jack.port_name_string(output_left)?,
            jack.port_name_string(output_right)?,
        ];
        let mut callback = Box::new(CallbackData {
            engine,
            effects,
            tempo_bpm: Arc::clone(&tempo_bpm),
            applied_tempo_bits: tempo_bits,
            output_left,
            output_right,
            port_get_buffer: jack.port_get_buffer(),
            maximum_frames: config.maximum_callback_frames,
            lost: AtomicBool::new(false),
        });
        let pointer = ((&mut *callback) as *mut CallbackData).cast();
        // SAFETY: the callback box remains pinned until explicit deactivation.
        unsafe {
            jack.set_process_callback(process_callback, pointer)?;
            jack.set_shutdown_callback(shutdown_callback, pointer);
        }
        jack.activate().context("activate SHR Drums JACK host")?;
        for (source, destination) in output_ports.iter().zip(destinations) {
            if let Err(error) = jack.ensure_connection(source, destination) {
                jack.deactivate();
                return Err(error.context("connect SHR Drums direct output"));
            }
        }
        *shared
            .lock()
            .map_err(|_| anyhow::anyhow!("SHR Drums output lock failed"))? = Some(sender.clone());
        Ok(Self {
            jack,
            callback,
            sender,
            shared,
            output_ports,
            project_key,
            tuning: tuning.clone(),
            drum_rack: drum_rack.clone(),
            tempo_bpm,
        })
    }

    pub fn kit_id(&self) -> &str {
        self.callback.engine.kit_id()
    }

    pub fn output_ports(&self) -> [String; 2] {
        self.output_ports.clone()
    }

    pub fn matches_configuration(
        &self,
        kit_id: &str,
        project_key: crate::scale::Scale,
        tuning: &KitTuning,
        drum_rack: &crate::audio_graph::InsertRack,
    ) -> bool {
        self.kit_id() == kit_id
            && self.project_key == project_key
            && &self.tuning == tuning
            && &self.drum_rack == drum_rack
    }

    pub fn lost(&self) -> bool {
        self.callback.lost.load(Ordering::Acquire)
    }

    pub fn all_notes_off(&self) {
        let _ = self.sender.all_notes_off();
    }

    pub fn drain(&self) {
        let _ = self.sender.push(DrumEvent::Drain);
    }

    pub fn set_tempo(&self, tempo: Bpm) {
        self.tempo_bpm
            .store((tempo.as_f64() as f32).to_bits(), Ordering::Release);
    }
}

impl Drop for DrumHost {
    fn drop(&mut self) {
        let _ = self.sender.all_notes_off();
        self.jack.deactivate();
        self.callback.engine.all_notes_off();
        self.callback.effects.reset();
        if let Ok(mut shared) = self.shared.lock() {
            *shared = None;
        }
    }
}

pub fn send_midi(sender: &EventSender, bytes: &[u8]) -> std::result::Result<(), String> {
    let event = match bytes {
        [status, note, velocity] if status & 0xf0 == 0x90 && *velocity > 0 => {
            Some(DrumEvent::NoteOn {
                note: *note,
                velocity: *velocity,
            })
        }
        [status, note, _] if matches!(status & 0xf0, 0x80 | 0x90) => {
            Some(DrumEvent::NoteOff { note: *note })
        }
        [status, controller, _] if status & 0xf0 == 0xb0 && matches!(controller, 120 | 123) => {
            Some(DrumEvent::AllNotesOff)
        }
        [] | [_] | [_, _] => None,
        _ => None,
    };
    event.map_or(Ok(()), |event| {
        sender
            .push(event)
            .map_err(|_| "SHR Drums event queue is full".into())
    })
}

fn process_block(callback: &mut CallbackData, frames: usize, left: &mut [f32], right: &mut [f32]) {
    if frames > callback.maximum_frames || left.len() < frames || right.len() < frames {
        left.fill(0.0);
        right.fill(0.0);
        return;
    }
    for offset in (0..frames).step_by(256) {
        let end = (offset + 256).min(frames);
        let mut rendered = [StereoFrame::SILENCE; 256];
        callback.engine.process(&mut rendered[..end - offset]);
        if callback.engine.take_hard_reset_request() {
            callback.effects.reset();
        }
        let tempo_bits = callback.tempo_bpm.load(Ordering::Acquire);
        if tempo_bits != callback.applied_tempo_bits {
            callback.effects.set_tempo(f32::from_bits(tempo_bits));
            callback.applied_tempo_bits = tempo_bits;
        }
        let mut effected = [EffectFrame::SILENCE; 256];
        for (target, source) in effected[..end - offset]
            .iter_mut()
            .zip(&rendered[..end - offset])
        {
            *target = EffectFrame::new(source.left, source.right);
        }
        callback.effects.process(&mut effected[..end - offset]);
        for (index, frame) in effected[..end - offset].iter().enumerate() {
            left[offset + index] = frame.left;
            right[offset + index] = frame.right;
        }
    }
}

unsafe extern "C" fn process_callback(frames: c_uint, argument: *mut c_void) -> c_int {
    if argument.is_null() {
        return 0;
    }
    // SAFETY: JACK receives the pinned callback pointer during start.
    let callback = unsafe { &mut *argument.cast::<CallbackData>() };
    let frames = frames as usize;
    let left_pointer =
        unsafe { (callback.port_get_buffer)(callback.output_left, frames as c_uint) }.cast::<f32>();
    let right_pointer =
        unsafe { (callback.port_get_buffer)(callback.output_right, frames as c_uint) }
            .cast::<f32>();
    if left_pointer.is_null() || right_pointer.is_null() {
        callback.lost.store(true, Ordering::Release);
        return 0;
    }
    // SAFETY: JACK owns buffers for exactly this callback's frame count.
    let left = unsafe { std::slice::from_raw_parts_mut(left_pointer, frames) };
    let right = unsafe { std::slice::from_raw_parts_mut(right_pointer, frames) };
    process_block(callback, frames, left, right);
    0
}

unsafe extern "C" fn shutdown_callback(argument: *mut c_void) {
    if !argument.is_null() {
        // SAFETY: JACK keeps the registered callback argument live until
        // deactivation/close; this callback only sets a lock-free flag.
        unsafe { &*argument.cast::<CallbackData>() }
            .lost
            .store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_graph::{default_drum_rack, EffectKind, InsertRack};
    use crate::dsp::allocation_test::assert_no_allocations;

    const SAMPLE_RATE: u32 = 48_000;

    fn rack_mode(reverb: bool, delay: bool) -> InsertRack {
        let mut rack = default_drum_rack("electronic-house", 70).unwrap();
        rack.effect_mut(70).unwrap().bypass = !reverb;
        rack.effect_mut(71).unwrap().bypass = !delay;
        rack
    }

    fn set(rack: &mut InsertRack, kind: EffectKind, name: &str, value: f32) {
        rack.effects
            .iter_mut()
            .find(|effect| effect.kind == kind)
            .unwrap()
            .parameters
            .insert(name.into(), value);
    }

    fn render_impulse(rack: &InsertRack, frames: usize) -> Vec<EffectFrame> {
        let mut stack = DrumEffectStack::compile(rack, SAMPLE_RATE, 256, Bpm::DEFAULT).unwrap();
        let mut output = vec![EffectFrame::SILENCE; frames];
        output[0] = EffectFrame::new(0.5, 0.5);
        for chunk in output.chunks_mut(256) {
            stack.process(chunk);
        }
        output
    }

    #[test]
    fn internal_midi_translation_preserves_gm_note_numbers_and_all_notes_off() {
        let (sender, mut receiver) = event_queue();
        send_midi(&sender, &[0x99, 36, 101]).unwrap();
        send_midi(&sender, &[0x89, 36, 0]).unwrap();
        send_midi(&sender, &[0xb9, 123, 0]).unwrap();
        assert_eq!(
            receiver.pop(),
            Some(DrumEvent::NoteOn {
                note: 36,
                velocity: 101
            })
        );
        assert_eq!(receiver.pop(), Some(DrumEvent::NoteOff { note: 36 }));
        assert_eq!(receiver.pop(), Some(DrumEvent::AllNotesOff));
    }

    #[test]
    fn four_drum_effect_modes_are_independent_and_reverb_has_no_333_ms_slap() {
        let off = render_impulse(&rack_mode(false, false), SAMPLE_RATE as usize);
        assert_eq!(off[0], EffectFrame::new(0.5, 0.5));
        assert!(off[1..].iter().all(|frame| *frame == EffectFrame::SILENCE));

        let mut reverb_rack = rack_mode(true, false);
        set(&mut reverb_rack, EffectKind::Reverb, "wet_percent", 100.0);
        set(&mut reverb_rack, EffectKind::Reverb, "dry_percent", 0.0);
        let reverb = render_impulse(&reverb_rack, SAMPLE_RATE as usize);
        let center = (SAMPLE_RATE as f32 * 0.333).round() as usize;
        let window = &reverb[center - 960..center + 960];
        let peak = window
            .iter()
            .map(|frame| frame.left.abs().max(frame.right.abs()))
            .fold(0.0_f32, f32::max);
        let diffuse_samples = window
            .iter()
            .filter(|frame| frame.left.abs().max(frame.right.abs()) > peak * 0.02)
            .count();
        assert!(peak > 1.0e-5);
        assert!(
            diffuse_samples > 200,
            "333 ms window was sparse ({diffuse_samples} samples)"
        );

        let mut delay_rack = rack_mode(false, true);
        set(&mut delay_rack, EffectKind::Delay, "tempo_sync", 0.0);
        set(&mut delay_rack, EffectKind::Delay, "time_ms", 100.0);
        set(&mut delay_rack, EffectKind::Delay, "feedback_percent", 0.0);
        set(&mut delay_rack, EffectKind::Delay, "stereo_ratio", 1.0);
        set(&mut delay_rack, EffectKind::Delay, "wet_percent", 100.0);
        set(&mut delay_rack, EffectKind::Delay, "dry_percent", 0.0);
        let delay = render_impulse(&delay_rack, 9_600);
        assert!(delay[..4_800]
            .iter()
            .all(|frame| frame.left.abs().max(frame.right.abs()) < 1.0e-7));
        assert!((delay[4_800].left - 0.5).abs() < 1.0e-5);
        assert!(delay[4_801..]
            .iter()
            .all(|frame| frame.left.abs().max(frame.right.abs()) < 1.0e-7));

        let mut combined_rack = reverb_rack.clone();
        combined_rack.effect_mut(71).unwrap().bypass = false;
        set(&mut combined_rack, EffectKind::Delay, "tempo_sync", 0.0);
        set(&mut combined_rack, EffectKind::Delay, "time_ms", 100.0);
        set(
            &mut combined_rack,
            EffectKind::Delay,
            "feedback_percent",
            0.0,
        );
        set(&mut combined_rack, EffectKind::Delay, "stereo_ratio", 1.0);
        set(&mut combined_rack, EffectKind::Delay, "wet_percent", 50.0);
        set(&mut combined_rack, EffectKind::Delay, "dry_percent", 100.0);
        let combined = render_impulse(&combined_rack, SAMPLE_RATE as usize);
        assert!(combined[..4_800]
            .iter()
            .zip(&reverb[..4_800])
            .all(|(both, room)| {
                (both.left - room.left).abs() < 1.0e-6 && (both.right - room.right).abs() < 1.0e-6
            }));
        assert!(combined[4_800..]
            .iter()
            .zip(&reverb[4_800..])
            .any(|(both, room)| (both.left - room.left).abs() > 1.0e-5));
    }

    #[test]
    fn drum_stack_is_deterministic_bounded_resettable_and_allocation_free() {
        let rack = default_drum_rack("experimental-noise-muldjord", 80).unwrap();
        let mut first = DrumEffectStack::compile(&rack, SAMPLE_RATE, 128, Bpm::DEFAULT).unwrap();
        let mut second = DrumEffectStack::compile(&rack, SAMPLE_RATE, 128, Bpm::DEFAULT).unwrap();
        assert!(first.memory_bytes() > 800_000);
        assert!(first.memory_bytes() < 1_100_000);
        let input = (0..4_096)
            .map(|index| {
                let value = (index as f32 * 0.071).sin() * 2.0;
                EffectFrame::new(value, -value * 0.75)
            })
            .collect::<Vec<_>>();
        let mut left = input.clone();
        let mut right = input;
        assert_no_allocations(|| {
            for chunk in left.chunks_mut(128) {
                first.process(chunk);
            }
        });
        for chunk in right.chunks_mut(73) {
            second.process(chunk);
        }
        assert_eq!(left, right);
        assert!(left.iter().all(|frame| {
            frame.left.is_finite()
                && frame.right.is_finite()
                && frame.left.abs() <= 0.891_251
                && frame.right.abs() <= 0.891_251
        }));

        first.reset();
        let mut silence = [EffectFrame::SILENCE; 512];
        first.process(&mut silence);
        assert!(silence.iter().all(|frame| *frame == EffectFrame::SILENCE));
    }

    #[test]
    #[ignore = "writes deterministic private effect-stack review WAVs"]
    fn render_private_drum_effect_review_pack() {
        use crate::final_bus::{BusControls, BusSource, FinalBusMeters, FinalBusProcessor};
        use hound::{SampleFormat, WavSpec, WavWriter};
        use shr_drums::{KitTuning, ProjectKey};
        use std::fmt::Write as _;
        use std::fs;
        use std::path::Path;

        const ROW_FRAMES: usize = 6_000;
        const TAIL_FRAMES: usize = SAMPLE_RATE as usize * 6;

        fn write_wav(path: &Path, frames: &[EffectFrame]) {
            let mut writer = WavWriter::create(
                path,
                WavSpec {
                    channels: 2,
                    sample_rate: SAMPLE_RATE,
                    bits_per_sample: 24,
                    sample_format: SampleFormat::Int,
                },
            )
            .unwrap();
            for frame in frames {
                for sample in [frame.left, frame.right] {
                    writer
                        .write_sample((sample.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32)
                        .unwrap();
                }
            }
            writer.finalize().unwrap();
        }

        fn groove_hits(row: usize) -> Vec<(u8, u8)> {
            let mut hits = Vec::with_capacity(5);
            if row % 16 == 0 {
                hits.push((49, if row == 0 { 112 } else { 92 }));
            }
            if matches!(row % 16, 0 | 7 | 10) {
                hits.push((36, if row % 16 == 0 { 116 } else { 98 }));
            }
            if matches!(row % 16, 4 | 12) {
                hits.push((38, if row % 16 == 12 { 112 } else { 104 }));
            }
            if row % 2 == 0 {
                hits.push((42, 62 + ((row * 11) % 24) as u8));
            }
            if row % 16 == 14 {
                hits.push((46, 82));
            }
            if row >= 56 {
                let tom = [50, 47, 45, 43][(row - 56) / 2 % 4];
                if row % 2 == 0 {
                    hits.push((tom, 96 + ((row - 56) * 3) as u8));
                }
            } else if row % 8 == 6 {
                hits.push((51, 74));
            }
            hits
        }

        fn render_kit(kit: &KitEntry, rack: &InsertRack, rows: usize) -> Vec<EffectFrame> {
            let prepared =
                load_package(&kit.path, ProjectKey::default(), &KitTuning::default()).unwrap();
            let (sender, receiver) = event_queue();
            let mut engine = DrumEngine::new(SAMPLE_RATE, prepared, receiver).unwrap();
            let mut effects =
                DrumEffectStack::compile(rack, SAMPLE_RATE, 256, Bpm::DEFAULT).unwrap();
            let mut output = Vec::with_capacity(rows * ROW_FRAMES + TAIL_FRAMES);
            let mut rendered = [StereoFrame::SILENCE; 256];
            let mut effected = [EffectFrame::SILENCE; 256];
            let mut render_frames =
                |count: usize,
                 engine: &mut DrumEngine,
                 effects: &mut DrumEffectStack,
                 output: &mut Vec<EffectFrame>| {
                    for block_frames in std::iter::repeat_n(256, count / 256)
                        .chain((count % 256 != 0).then_some(count % 256))
                    {
                        rendered[..block_frames].fill(StereoFrame::SILENCE);
                        engine.process(&mut rendered[..block_frames]);
                        for (target, source) in effected[..block_frames]
                            .iter_mut()
                            .zip(&rendered[..block_frames])
                        {
                            *target = EffectFrame::new(source.left, source.right);
                        }
                        effects.process(&mut effected[..block_frames]);
                        output.extend_from_slice(&effected[..block_frames]);
                    }
                };
            for row in 0..rows {
                for (note, velocity) in groove_hits(row) {
                    sender.push(DrumEvent::NoteOn { note, velocity }).unwrap();
                }
                render_frames(ROW_FRAMES, &mut engine, &mut effects, &mut output);
            }
            sender.push(DrumEvent::Drain).unwrap();
            render_frames(TAIL_FRAMES, &mut engine, &mut effects, &mut output);
            output
        }

        fn measured(frames: &[EffectFrame], source_end: usize) -> (f32, f32) {
            let peak = frames
                .iter()
                .map(|frame| frame.left.abs().max(frame.right.abs()))
                .fold(0.0_f32, f32::max);
            let last = frames
                .iter()
                .enumerate()
                .skip(source_end)
                .rfind(|(_, frame)| frame.left.abs().max(frame.right.abs()) >= 0.000_1)
                .map_or(source_end, |(index, _)| index);
            (
                peak,
                last.saturating_sub(source_end) as f32 / SAMPLE_RATE as f32,
            )
        }

        let destination = std::path::PathBuf::from(
            std::env::var("SHSYNTH_EFFECT_REVIEW_DIR")
                .expect("set SHSYNTH_EFFECT_REVIEW_DIR to one new private directory"),
        );
        let private_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("user");
        assert!(destination.is_absolute() && destination.starts_with(&private_root));
        fs::create_dir(&destination).unwrap();

        let kit_root = std::path::PathBuf::from(
            std::env::var("SHSYNTH_DRUM_REVIEW_KIT_DIR")
                .expect("set SHSYNTH_DRUM_REVIEW_KIT_DIR to the private kit directory"),
        );
        let kits = discover_kits(&kit_root);
        let find_kit = |id: &str| kits.iter().find(|kit| kit.id == id).unwrap().clone();
        let big_rock = find_kit("big-rock-muldjord");
        let noise = kits
            .iter()
            .find(|kit| {
                kit.id == "experimental-noise-muldjord" || kit.id == "industrial-metal-muldjord"
            })
            .unwrap()
            .clone();
        let house = find_kit("electronic-house");

        let mut report = String::from(
            "sample_rate_hz=48000\nrow_frames=6000\nthreshold_dbfs=-80\n\
             design_max_delay_seconds=2.0\ndesign_max_reverb_rt60_seconds=8.0\n\
             design_max_reverb_tail_seconds=12.4\n",
        );

        for (index, (name, reverb, delay)) in [
            ("off", false, false),
            ("reverb", true, false),
            ("reverb-delay", true, true),
            ("delay", false, true),
        ]
        .into_iter()
        .enumerate()
        {
            let mut rack = rack_mode(reverb, delay);
            if delay {
                set(&mut rack, EffectKind::Delay, "time_ms", 250.0);
                set(&mut rack, EffectKind::Delay, "feedback_percent", 35.0);
            }
            let response = render_impulse(&rack, SAMPLE_RATE as usize * 6);
            write_wav(
                &destination.join(format!("{:02}-impulse-{name}.wav", index + 1)),
                &response,
            );
            let stack = DrumEffectStack::compile(&rack, SAMPLE_RATE, 256, Bpm::DEFAULT).unwrap();
            let reverb_bytes = stack
                .slots
                .iter()
                .filter(|slot| slot.kind() == EffectKind::Reverb)
                .map(EffectSlot::memory_bytes)
                .sum::<usize>();
            let delay_bytes = stack
                .slots
                .iter()
                .filter(|slot| slot.kind() == EffectKind::Delay)
                .map(EffectSlot::memory_bytes)
                .sum::<usize>();
            let (peak, tail) = measured(&response, 1);
            writeln!(
                report,
                "impulse_{name}: peak={peak:.6} tail_seconds={tail:.4} \
                 reverb_bytes={reverb_bytes} delay_bytes={delay_bytes}"
            )
            .unwrap();
        }

        for (index, (name, reverb, delay)) in [
            ("dry", false, false),
            ("reverb", true, false),
            ("delay", false, true),
            ("combined", true, true),
        ]
        .into_iter()
        .enumerate()
        {
            let mut rack = rack_mode(reverb, delay);
            if delay {
                set(&mut rack, EffectKind::Delay, "tempo_sync", 1.0);
                set(&mut rack, EffectKind::Delay, "division", 0.0);
            }
            let frames = render_kit(&house, &rack, 16);
            write_wav(
                &destination.join(format!("{:02}-comparison-{name}.wav", index + 10)),
                &frames,
            );
            let (peak, tail) = measured(&frames, 16 * ROW_FRAMES);
            writeln!(
                report,
                "comparison_{name}: peak={peak:.6} tail_seconds={tail:.4}"
            )
            .unwrap();
        }

        for (index, kit) in [&big_rock, &noise, &house].into_iter().enumerate() {
            let rack = default_drum_rack(&kit.id, 90).unwrap();
            let frames = render_kit(kit, &rack, 64);
            write_wav(
                &destination.join(format!("{:02}-full-kit-{}.wav", index + 20, kit.id)),
                &frames,
            );
            let (peak, tail) = measured(&frames, 64 * ROW_FRAMES);
            writeln!(
                report,
                "full_kit_{}: peak={peak:.6} tail_seconds={tail:.4}",
                kit.id
            )
            .unwrap();
        }

        let rack = default_drum_rack(&house.id, 100).unwrap();
        let mut drums = render_kit(&house, &rack, 32);
        let mut synth = vec![EffectFrame::SILENCE; drums.len()];
        let notes = [
            220.0_f32, 261.626, 329.628, 293.665, 246.942, 329.628, 392.0, 329.628,
        ];
        for (index, frame) in synth.iter_mut().enumerate().take(32 * ROW_FRAMES) {
            let row = index / ROW_FRAMES;
            let within = index % (ROW_FRAMES * 4);
            let envelope = if within < ROW_FRAMES * 3 {
                let attack = (within as f32 / 240.0).min(1.0);
                let release = ((ROW_FRAMES * 3 - within) as f32 / 1_200.0).min(1.0);
                attack * release
            } else {
                0.0
            };
            let frequency = notes[(row / 4).min(notes.len() - 1)];
            let phase = 2.0 * std::f32::consts::PI * frequency * index as f32 / SAMPLE_RATE as f32;
            *frame = EffectFrame::new(phase.sin() * envelope * 0.24, phase.sin() * envelope * 0.22);
        }
        let controls = Arc::new(BusControls::default());
        let meters = Arc::new(FinalBusMeters::default());
        let (mut final_bus, _) =
            FinalBusProcessor::with_neutral_strip(SAMPLE_RATE, 256, controls, meters).unwrap();
        for chunk in drums.chunks_mut(256) {
            final_bus.process_source(BusSource::Drums, chunk);
        }
        for chunk in synth.chunks_mut(256) {
            final_bus.process_source(BusSource::Synth, chunk);
        }
        for (drum, melody) in drums.iter_mut().zip(synth) {
            drum.left += melody.left;
            drum.right += melody.right;
        }
        for chunk in drums.chunks_mut(256) {
            final_bus.process_final(chunk);
        }
        write_wav(
            &destination.join("30-melodic-phrase-plus-drums.wav"),
            &drums,
        );
        let (peak, tail) = measured(&drums, 32 * ROW_FRAMES);
        writeln!(
            report,
            "melody_plus_drums: peak={peak:.6} tail_seconds={tail:.4}"
        )
        .unwrap();

        fs::write(destination.join("measurements.txt"), report).unwrap();
    }

    #[test]
    #[ignore = "writes a deterministic private Big Rock effect audition"]
    fn render_private_big_rock_effect_audition() {
        use hound::{SampleFormat, WavSpec, WavWriter};
        use shr_drums::{KitTuning, ProjectKey};
        use std::fmt::Write as _;
        use std::fs;
        use std::path::Path;

        const ROW_FRAMES: usize = 6_000;
        const ROWS: usize = 16;
        const TAIL_FRAMES: usize = SAMPLE_RATE as usize * 4;

        fn write_wav(path: &Path, frames: &[EffectFrame]) {
            let mut writer = WavWriter::create(
                path,
                WavSpec {
                    channels: 2,
                    sample_rate: SAMPLE_RATE,
                    bits_per_sample: 24,
                    sample_format: SampleFormat::Int,
                },
            )
            .unwrap();
            for frame in frames {
                for sample in [frame.left, frame.right] {
                    writer
                        .write_sample((sample.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32)
                        .unwrap();
                }
            }
            writer.finalize().unwrap();
        }

        fn groove_hits(row: usize) -> [(u8, u8); 4] {
            [
                (36, if row % 8 == 0 { 116 } else { 0 }),
                (38, if matches!(row % 8, 4) { 108 } else { 0 }),
                (42, if row % 2 == 0 { 78 } else { 0 }),
                (49, if row == 0 { 104 } else { 0 }),
            ]
        }

        let destination = PathBuf::from(
            std::env::var("SHSYNTH_ROCK_EFFECT_REVIEW_DIR")
                .expect("set SHSYNTH_ROCK_EFFECT_REVIEW_DIR to one new private directory"),
        );
        let private_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("user");
        assert!(destination.is_absolute() && destination.starts_with(&private_root));
        fs::create_dir(&destination).unwrap();

        let kit_root = PathBuf::from(
            std::env::var("SHSYNTH_DRUM_REVIEW_KIT_DIR")
                .expect("set SHSYNTH_DRUM_REVIEW_KIT_DIR to the private kit directory"),
        );
        let kit = discover_kits(&kit_root)
            .into_iter()
            .find(|kit| kit.id == "big-rock-muldjord")
            .expect("Big Rock review kit");
        let prepared =
            load_package(&kit.path, ProjectKey::default(), &KitTuning::default()).unwrap();
        let (sender, receiver) = event_queue();
        let mut engine = DrumEngine::new(SAMPLE_RATE, prepared, receiver).unwrap();
        let mut dry = Vec::with_capacity(ROWS * ROW_FRAMES + TAIL_FRAMES);
        let mut rendered = [StereoFrame::SILENCE; 256];
        let mut render_frames =
            |count: usize, engine: &mut DrumEngine, dry: &mut Vec<EffectFrame>| {
                for block_frames in std::iter::repeat_n(256, count / 256)
                    .chain((count % 256 != 0).then_some(count % 256))
                {
                    rendered[..block_frames].fill(StereoFrame::SILENCE);
                    engine.process(&mut rendered[..block_frames]);
                    dry.extend(
                        rendered[..block_frames]
                            .iter()
                            .map(|frame| EffectFrame::new(frame.left, frame.right)),
                    );
                }
            };
        for row in 0..ROWS {
            for (note, velocity) in groove_hits(row) {
                if velocity > 0 {
                    sender.push(DrumEvent::NoteOn { note, velocity }).unwrap();
                }
            }
            render_frames(ROW_FRAMES, &mut engine, &mut dry);
        }
        sender.push(DrumEvent::Drain).unwrap();
        render_frames(TAIL_FRAMES, &mut engine, &mut dry);

        let mut report = String::from(
            "sample_rate_hz=48000\nkit=big-rock-muldjord\nrows=16\n\
             row_frames=6000\nsource_seconds=2.0\nrender_seconds=6.0\n\
             threshold_dbfs=-80\n",
        );
        for (index, (name, reverb, delay)) in [
            ("dry", false, false),
            ("reverb", true, false),
            ("delay", false, true),
            ("combined", true, true),
        ]
        .into_iter()
        .enumerate()
        {
            let mut rack = default_drum_rack(&kit.id, 110).unwrap();
            rack.effect_mut(110).unwrap().bypass = !reverb;
            rack.effect_mut(111).unwrap().bypass = !delay;
            let mut stack =
                DrumEffectStack::compile(&rack, SAMPLE_RATE, 256, Bpm::DEFAULT).unwrap();
            let mut frames = dry.clone();
            for chunk in frames.chunks_mut(256) {
                stack.process(chunk);
            }
            let peak = frames
                .iter()
                .map(|frame| frame.left.abs().max(frame.right.abs()))
                .fold(0.0_f32, f32::max);
            let source_end = ROWS * ROW_FRAMES;
            let tail = frames
                .iter()
                .enumerate()
                .skip(source_end)
                .rfind(|(_, frame)| frame.left.abs().max(frame.right.abs()) >= 0.000_1)
                .map_or(0.0, |(sample, _)| {
                    sample.saturating_sub(source_end) as f32 / SAMPLE_RATE as f32
                });
            write_wav(
                &destination.join(format!("{:02}-big-rock-{name}.wav", index + 1)),
                &frames,
            );
            writeln!(report, "{name}: peak={peak:.6} tail_seconds={tail:.4}").unwrap();
        }
        fs::write(destination.join("measurements.txt"), report).unwrap();
    }
}
