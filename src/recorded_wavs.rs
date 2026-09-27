//! Saved-take browsing and bounded stereo playback. File reads and resampling
//! run on a worker; JACK only drains a fixed ring and applies transport fades.
use anyhow::{bail, Context, Result};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const CAPACITY: usize = 32_768;
const MAX_ENTRIES: usize = 4096;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub duration: Option<Duration>,
}

pub fn list(directory: &Path) -> Result<Vec<Entry>> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file()
            || !entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        {
            continue;
        }
        if found.len() == MAX_ENTRIES {
            bail!("recordings folder exceeds {MAX_ENTRIES} WAVs");
        }
        let path = entry.path();
        let duration = open_reader(&path).ok().and_then(|reader| {
            valid_spec(reader.spec()).ok()?;
            if reader.duration() == 0 {
                return None;
            }
            Some(Duration::from_secs_f64(
                f64::from(reader.duration()) / f64::from(reader.spec().sample_rate),
            ))
        });
        found.push((entry.metadata()?.modified().ok(), Entry { path, duration }));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.path.cmp(&a.1.path)));
    Ok(found.into_iter().map(|(_, entry)| entry).collect())
}

fn open_reader(path: &Path) -> Result<hound::WavReader<std::io::BufReader<std::fs::File>>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        bail!("not a regular WAV file");
    }
    hound::WavReader::new(std::io::BufReader::new(file)).context("read WAV")
}

fn valid_spec(spec: hound::WavSpec) -> Result<()> {
    if !(1..=2).contains(&spec.channels)
        || !(8_000..=384_000).contains(&spec.sample_rate)
        || !match spec.sample_format {
            hound::SampleFormat::Float => spec.bits_per_sample == 32,
            hound::SampleFormat::Int => matches!(spec.bits_per_sample, 8 | 16 | 24 | 32),
        }
    {
        bail!("unsupported WAV format (use mono/stereo PCM or float)");
    }
    Ok(())
}

struct Shared {
    // One producer and one consumer. Release of the cursor publishes the slot;
    // neither side reuses a slot until the other cursor acknowledges it.
    ring: Box<[AtomicU64]>,
    write: AtomicUsize,
    read: AtomicUsize,
    armed: AtomicBool,
    paused: AtomicBool,
    stopping: AtomicBool,
    silent: AtomicBool,
    eof: AtomicBool,
    finished: AtomicBool,
    failed: AtomicBool,
    shutdown: AtomicBool,
    underruns: AtomicU64,
    played: AtomicU64,
}

impl Shared {
    fn new() -> Self {
        Self {
            ring: (0..CAPACITY).map(|_| AtomicU64::new(0)).collect(),
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            armed: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            silent: AtomicBool::new(true),
            eof: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
            played: AtomicU64::new(0),
        }
    }

    fn push(&self, frame: [f32; 2]) -> bool {
        let write = self.write.load(Ordering::Relaxed);
        if write.wrapping_sub(self.read.load(Ordering::Acquire)) >= CAPACITY {
            return false;
        }
        let bits = u64::from(frame[0].to_bits()) | (u64::from(frame[1].to_bits()) << 32);
        self.ring[write % CAPACITY].store(bits, Ordering::Relaxed);
        self.write.store(write.wrapping_add(1), Ordering::Release);
        true
    }

    fn pop(&self) -> Option<[f32; 2]> {
        let read = self.read.load(Ordering::Relaxed);
        if read == self.write.load(Ordering::Acquire) {
            return None;
        }
        let bits = self.ring[read % CAPACITY].load(Ordering::Relaxed);
        self.read.store(read.wrapping_add(1), Ordering::Release);
        Some([
            f32::from_bits(bits as u32),
            f32::from_bits((bits >> 32) as u32),
        ])
    }
}

struct Decoder {
    reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    spec: hound::WavSpec,
    first: [f32; 2],
    second: [f32; 2],
    phase: f64,
    step: f64,
    remaining: u64,
    fade_frames: u64,
}

impl Decoder {
    fn open(path: &Path, output_rate: u32) -> Result<Self> {
        if !(8_000..=384_000).contains(&output_rate) {
            bail!("unsupported playback sample rate");
        }
        let reader = open_reader(path)?;
        let spec = reader.spec();
        valid_spec(spec)?;
        if reader.duration() == 0 {
            bail!("WAV is empty");
        }
        let remaining = (u64::from(reader.duration()) * u64::from(output_rate))
            .div_ceil(u64::from(spec.sample_rate));
        let mut decoder = Self {
            reader,
            spec,
            first: [0.0; 2],
            second: [0.0; 2],
            phase: 0.0,
            step: f64::from(spec.sample_rate) / f64::from(output_rate),
            remaining,
            fade_frames: u64::from(output_rate / 200).max(1),
        };
        decoder.first = decoder.read_frame()?.context("WAV is empty")?;
        decoder.second = decoder.read_frame()?.unwrap_or(decoder.first);
        Ok(decoder)
    }

    fn sample(&mut self) -> Result<Option<f32>> {
        let sample = match self.spec.sample_format {
            hound::SampleFormat::Float => self.reader.samples::<f32>().next().transpose()?,
            hound::SampleFormat::Int => self
                .reader
                .samples::<i32>()
                .next()
                .transpose()?
                .map(|v| v as f32 / (1u64 << (self.spec.bits_per_sample - 1)) as f32),
        };
        Ok(sample.map(|v| {
            if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        }))
    }

    fn read_frame(&mut self) -> Result<Option<[f32; 2]>> {
        let Some(left) = self.sample()? else {
            return Ok(None);
        };
        let right = if self.spec.channels == 2 {
            self.sample()?.context("truncated stereo frame")?
        } else {
            left
        };
        Ok(Some([left, right]))
    }

    fn next(&mut self) -> Result<Option<[f32; 2]>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let fade = (self.remaining as f32 / self.fade_frames as f32).min(1.0);
        let output = std::array::from_fn(|i| {
            (self.first[i] + (self.second[i] - self.first[i]) * self.phase as f32) * fade
        });
        self.remaining -= 1;
        self.phase += self.step;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.first = self.second;
            self.second = self.read_frame()?.unwrap_or(self.first);
        }
        Ok(Some(output))
    }
}

struct Callback {
    shared: Arc<Shared>,
    left: *mut crate::jack::Port,
    right: *mut crate::jack::Port,
    get_buffer: crate::jack::PortGetBuffer,
    gain: f32,
    gain_step: f32,
}

impl Callback {
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.shared.armed.load(Ordering::Acquire) {
            left.fill(0.0);
            right.fill(0.0);
            return;
        }
        let stop = self.shared.stopping.load(Ordering::Acquire);
        let pause = self.shared.paused.load(Ordering::Acquire);
        let target = if stop || pause { 0.0 } else { 1.0 };
        let mut underrun = false;
        for (left, right) in left.iter_mut().zip(right) {
            self.gain += (target - self.gain).clamp(-self.gain_step, self.gain_step);
            let frame = if self.gain > 0.0 {
                match self.shared.pop() {
                    Some(frame) => {
                        self.shared.played.fetch_add(1, Ordering::Relaxed);
                        frame
                    }
                    None => {
                        if self.shared.eof.load(Ordering::Acquire)
                            && self.shared.read.load(Ordering::Relaxed)
                                == self.shared.write.load(Ordering::Acquire)
                        {
                            self.shared.finished.store(true, Ordering::Release);
                        } else if !stop {
                            underrun = true;
                        }
                        [0.0; 2]
                    }
                }
            } else {
                [0.0; 2]
            };
            *left = frame[0] * self.gain;
            *right = frame[1] * self.gain;
        }
        self.shared
            .silent
            .store(self.gain == 0.0, Ordering::Release);
        if underrun {
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}

unsafe extern "C" fn process(frames: libc::c_uint, argument: *mut libc::c_void) -> libc::c_int {
    // SAFETY: Player owns this pinned box until JACK has deactivated.
    let data = unsafe { &mut *argument.cast::<Callback>() };
    let left = unsafe { (data.get_buffer)(data.left, frames) }.cast::<f32>();
    let right = unsafe { (data.get_buffer)(data.right, frames) }.cast::<f32>();
    if left.is_null() || right.is_null() {
        if !left.is_null() {
            unsafe { std::slice::from_raw_parts_mut(left, frames as usize) }.fill(0.0);
        }
        if !right.is_null() {
            unsafe { std::slice::from_raw_parts_mut(right, frames as usize) }.fill(0.0);
        }
        data.shared.failed.store(true, Ordering::Release);
        data.shared.finished.store(true, Ordering::Release);
    } else {
        data.render(
            unsafe { std::slice::from_raw_parts_mut(left, frames as usize) },
            unsafe { std::slice::from_raw_parts_mut(right, frames as usize) },
        );
    }
    0
}

unsafe extern "C" fn shutdown(argument: *mut libc::c_void) {
    // Only shared atomics are accessed by the asynchronous notification.
    let shared = unsafe { &*argument.cast::<Shared>() };
    shared.shutdown.store(true, Ordering::Release);
}

pub struct Player {
    jack: crate::jack::Client,
    callback: Box<Callback>,
    // UI/shutdown access this separate Arc, never the mutably borrowed callback.
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    pub path: PathBuf,
    sample_rate: u32,
}

impl Player {
    pub fn start(path: &Path, client_name: &str, outputs: &[String]) -> Result<Self> {
        let [left_output, right_output] = outputs else {
            bail!("select a stereo playback output in Routing");
        };
        let jack = crate::jack::Client::open(client_name)?;
        let rate = jack.sample_rate();
        if !(8_000..=384_000).contains(&rate) {
            bail!("unsupported playback sample rate");
        }
        let mut decoder = Decoder::open(path, rate)?;
        let shared = Arc::new(Shared::new());
        // Prime before activation so ordinary disk startup cannot underrun.
        for _ in 0..CAPACITY / 2 {
            match decoder.next()? {
                Some(frame) => {
                    shared.push(frame);
                }
                None => {
                    shared.eof.store(true, Ordering::Release);
                    break;
                }
            }
        }
        let left = jack.register_audio_port("left", crate::jack::PortDirection::Output)?;
        let right = jack.register_audio_port("right", crate::jack::PortDirection::Output)?;
        let callback = Box::new(Callback {
            shared: Arc::clone(&shared),
            left,
            right,
            get_buffer: jack.port_get_buffer(),
            gain: 0.0,
            gain_step: 1.0 / (rate as f32 * 0.005),
        });
        let producer = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("recorded-wav".into())
            .spawn(move || {
                while !producer.stopping.load(Ordering::Acquire)
                    && !producer.eof.load(Ordering::Acquire)
                    && !producer.shutdown.load(Ordering::Acquire)
                {
                    let frame = match decoder.next() {
                        Ok(Some(frame)) => frame,
                        Ok(None) => {
                            producer.eof.store(true, Ordering::Release);
                            break;
                        }
                        Err(_) => {
                            producer.failed.store(true, Ordering::Release);
                            producer.eof.store(true, Ordering::Release);
                            break;
                        }
                    };
                    while !producer.push(frame) {
                        if producer.stopping.load(Ordering::Acquire)
                            || producer.shutdown.load(Ordering::Acquire)
                        {
                            return;
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                }
            })?;
        let mut player = Self {
            jack,
            callback,
            shared: Arc::clone(&shared),
            worker: Some(worker),
            path: path.to_owned(),
            sample_rate: rate,
        };
        unsafe {
            player
                .jack
                .set_process_callback(process, (&mut *player.callback as *mut Callback).cast())?;
            player
                .jack
                .set_shutdown_callback(shutdown, Arc::as_ptr(&shared).cast_mut().cast());
        }
        // Player's Drop also covers partial connection/activation failure.
        player.jack.activate()?;
        player.jack.connect_port_to_external(left, left_output)?;
        player.jack.connect_port_to_external(right, right_output)?;
        shared.silent.store(false, Ordering::Release);
        shared.armed.store(true, Ordering::Release);
        Ok(player)
    }

    pub fn paused(&self) -> bool {
        self.shared.paused.load(Ordering::Acquire)
    }
    pub fn toggle_pause(&self) {
        self.shared.paused.fetch_xor(true, Ordering::AcqRel);
    }
    pub fn finished(&self) -> bool {
        self.shared.finished.load(Ordering::Acquire) || self.shared.shutdown.load(Ordering::Acquire)
    }
    pub fn elapsed(&self) -> Duration {
        Duration::from_secs_f64(
            self.shared.played.load(Ordering::Relaxed) as f64 / f64::from(self.sample_rate),
        )
    }
    pub fn fault(&self) -> Option<&'static str> {
        if self.shared.shutdown.load(Ordering::Acquire) {
            Some("JACK stopped · press PLAY to retry")
        } else if self.shared.failed.load(Ordering::Acquire) {
            Some("WAV playback failed · retry")
        } else if self.shared.underruns.load(Ordering::Relaxed) > 0 {
            Some("WAV playback underrun")
        } else {
            None
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        for _ in 0..25 {
            if self.shared.silent.load(Ordering::Acquire)
                || self.shared.shutdown.load(Ordering::Acquire)
            {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        self.jack.deactivate();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::allocation_test::assert_no_allocations;

    fn callback(shared: Arc<Shared>) -> Callback {
        shared.armed.store(true, Ordering::Release);
        unsafe extern "C" fn unused(_: *mut crate::jack::Port, _: u32) -> *mut libc::c_void {
            std::ptr::null_mut()
        }
        Callback {
            shared,
            left: std::ptr::null_mut(),
            right: std::ptr::null_mut(),
            get_buffer: unused,
            gain: 0.0,
            gain_step: 0.25,
        }
    }

    #[test]
    fn unconnected_outputs_do_not_consume_the_start_of_a_take() {
        let shared = Arc::new(Shared::new());
        shared.push([0.5; 2]);
        let mut callback = callback(Arc::clone(&shared));
        shared.armed.store(false, Ordering::Release);
        let mut left = [1.0; 8];
        let mut right = [1.0; 8];
        callback.render(&mut left, &mut right);
        assert_eq!(left, [0.0; 8]);
        assert_eq!(right, [0.0; 8]);
        assert_eq!(shared.played.load(Ordering::Relaxed), 0);
        assert_eq!(shared.pop(), Some([0.5; 2]));
    }

    #[test]
    fn ring_wraps_without_overwriting_unplayed_frames() {
        let shared = Shared::new();
        for round in 0..3 {
            for index in 0..CAPACITY {
                assert!(shared.push([index as f32, round as f32]));
            }
            assert!(!shared.push([-1.0; 2]));
            for index in 0..CAPACITY {
                assert_eq!(shared.pop(), Some([index as f32, round as f32]));
            }
            assert_eq!(shared.pop(), None);
        }
    }

    #[test]
    fn producer_and_consumer_keep_stereo_frames_in_order() {
        let shared = Arc::new(Shared::new());
        let producer = Arc::clone(&shared);
        let worker = thread::spawn(move || {
            for index in 0..100_000 {
                while !producer.push([index as f32, -(index as f32)]) {
                    thread::yield_now();
                }
            }
        });
        for index in 0..100_000 {
            loop {
                if let Some(frame) = shared.pop() {
                    assert_eq!(frame, [index as f32, -(index as f32)]);
                    break;
                }
                thread::yield_now();
            }
        }
        worker.join().unwrap();
        assert_eq!(shared.pop(), None);
    }

    #[test]
    fn playback_fades_pauses_drains_and_finishes_without_callback_allocations() {
        let shared = Arc::new(Shared::new());
        for _ in 0..32 {
            assert!(shared.push([0.5, -0.5]));
        }
        let mut callback = callback(Arc::clone(&shared));
        let mut left = [0.0; 8];
        let mut right = [0.0; 8];
        assert_no_allocations(|| callback.render(&mut left, &mut right));
        assert_eq!(&left[..4], &[0.125, 0.25, 0.375, 0.5]);
        assert!(left.iter().zip(right).all(|(l, r)| *l == -r));
        shared.paused.store(true, Ordering::Release);
        callback.render(&mut left, &mut right);
        let position = shared.played.load(Ordering::Relaxed);
        assert_eq!(left[7], 0.0);
        assert_no_allocations(|| callback.render(&mut left, &mut right));
        assert_eq!(shared.played.load(Ordering::Relaxed), position);
        assert_eq!(left, [0.0; 8]);
        shared.paused.store(false, Ordering::Release);
        shared.eof.store(true, Ordering::Release);
        for _ in 0..4 {
            callback.render(&mut left, &mut right);
        }
        assert_eq!(shared.played.load(Ordering::Relaxed), 32);
        assert!(shared.finished.load(Ordering::Acquire));
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn empty_stream_reports_underrun_and_stop_silences_without_advancing() {
        let shared = Arc::new(Shared::new());
        let mut callback = callback(Arc::clone(&shared));
        let mut left = [1.0; 8];
        let mut right = [1.0; 8];
        callback.render(&mut left, &mut right);
        assert_eq!(left, [0.0; 8]);
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 1);
        assert!(!shared.finished.load(Ordering::Acquire));
        shared.stopping.store(true, Ordering::Release);
        callback.render(&mut left, &mut right);
        assert!(shared.silent.load(Ordering::Acquire));
        assert_eq!(shared.played.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn browser_lists_finished_wavs_and_decoder_preserves_stereo_and_bounds_duration() {
        let directory = std::env::temp_dir().join(format!(
            "shr-wav-browser-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("take.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..480 {
            writer.write_sample(2_097_152i32).unwrap();
            writer.write_sample(-4_194_304i32).unwrap();
        }
        writer.finalize().unwrap();
        std::fs::write(directory.join("unfinished.wav.part"), b"in progress").unwrap();
        std::fs::create_dir_all(directory.join("raw.take")).unwrap();
        std::os::unix::fs::symlink(&path, directory.join("link.wav")).unwrap();
        let entries = list(&directory).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].duration, Some(Duration::from_millis(10)));
        let mut decoder = Decoder::open(&path, 48_000).unwrap();
        assert_eq!(decoder.next().unwrap(), Some([0.25, -0.5]));
        let mut count = 1;
        let mut last = [0.0; 2];
        while let Some(frame) = decoder.next().unwrap() {
            count += 1;
            last = frame;
        }
        assert_eq!(count, 480);
        assert!(last[0].abs() < 0.002 && last[1].abs() < 0.003);
        let mut resampled = Decoder::open(&path, 96_000).unwrap();
        let mut count = 0;
        while resampled.next().unwrap().is_some() {
            count += 1;
        }
        assert_eq!(count, 960);
        assert!(Decoder::open(&directory.join("link.wav"), 48_000).is_err());
        std::fs::write(directory.join("broken.wav"), b"broken").unwrap();
        assert!(list(&directory)
            .unwrap()
            .iter()
            .any(|entry| entry.duration.is_none()));
        assert!(Decoder::open(&directory.join("broken.wav"), 48_000).is_err());
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(list(&directory).unwrap().is_empty());
    }
}
