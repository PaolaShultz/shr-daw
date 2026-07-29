//! SHR-owned JACK host for the in-process drum library.
//!
//! Package loading, validation, sample decoding, and engine preparation happen
//! before activation. The JACK callback only drains a bounded lock-free event
//! queue and renders into preallocated engine state.

use crate::config::DrumEngineConfig;
use crate::jack::{Client as JackClient, Port as JackPort, PortDirection, PortGetBuffer};
use anyhow::{bail, Context, Result};
use libc::{c_int, c_uint, c_void};
use shr_drums::{
    event_queue, load_package, DrumEngine, DrumEvent, EventSender, KitManifest, KitTuning,
    MusicalMode, PitchClass, ProjectKey, StereoFrame,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    output_left: *mut JackPort,
    output_right: *mut JackPort,
    port_get_buffer: PortGetBuffer,
    maximum_frames: usize,
    lost: AtomicBool,
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
}

impl DrumHost {
    pub fn start(
        config: &DrumEngineConfig,
        kit: &KitEntry,
        project_key: crate::scale::Scale,
        tuning: &KitTuning,
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
        let output_left = jack.register_audio_port("out_l", PortDirection::Output)?;
        let output_right = jack.register_audio_port("out_r", PortDirection::Output)?;
        let output_ports = [
            jack.port_name_string(output_left)?,
            jack.port_name_string(output_right)?,
        ];
        let mut callback = Box::new(CallbackData {
            engine,
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
    ) -> bool {
        self.kit_id() == kit_id && self.project_key == project_key && &self.tuning == tuning
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
}

impl Drop for DrumHost {
    fn drop(&mut self) {
        let _ = self.sender.all_notes_off();
        self.jack.deactivate();
        self.callback.engine.all_notes_off();
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
        for (index, frame) in rendered[..end - offset].iter().enumerate() {
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
}
