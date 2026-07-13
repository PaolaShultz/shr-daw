use crate::control::CONTROLS;
use crate::preset::{BackendKind, Preset, PresetId};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const FORMAT_VERSION: u32 = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimedEvent {
    pub micros: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct Recorder {
    started: Option<Instant>,
    pub events: Vec<TimedEvent>,
}

impl Recorder {
    pub fn start(&mut self, now: Instant) {
        self.events.clear();
        self.started = Some(now);
    }
    pub fn stop(&mut self) {
        self.started = None;
    }
    pub fn is_recording(&self) -> bool {
        self.started.is_some()
    }
    pub fn capture(&mut self, now: Instant, bytes: &[u8]) {
        let Some(start) = self.started else { return };
        if is_musical(bytes) {
            self.events.push(TimedEvent {
                micros: now.duration_since(start).as_micros() as u64,
                bytes: bytes.to_vec(),
            });
        }
    }
}

pub fn is_musical(m: &[u8]) -> bool {
    matches!(
        m.first().map(|b| b & 0xf0),
        Some(0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0)
    )
}

pub fn all_notes_off() -> Vec<Vec<u8>> {
    (0..16).map(|ch| vec![0xb0 | ch, 123, 0]).collect()
}

pub fn ideas_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into()))
                .join(".local/share")
        })
        .join("shsynth/ideas")
}

pub fn safe_name(input: &str) -> String {
    let s: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    s.trim_matches('-').chars().take(64).collect::<String>()
}

pub fn list(base: &Path) -> Result<Vec<String>> {
    let mut names = match fs::read_dir(base) {
        Ok(entries) => entries
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    names.sort_by_key(|x| x.to_lowercase());
    Ok(names)
}

pub fn inspect(base: &Path, name: &str) -> Result<String> {
    fs::read_to_string(base.join(safe_name(name)).join("metadata.json")).map_err(Into::into)
}

pub fn load(base: &Path, name: &str) -> Result<(Preset, Vec<TimedEvent>)> {
    let dir = base.join(safe_name(name));
    let preset = if dir.join("preset.ref").is_file() {
        read_preset_ref(&dir.join("preset.ref"), &dir)?
    } else {
        let path = dir.join("preset.synthv1");
        if !path.is_file() {
            bail!("idea preset snapshot is missing");
        }
        Preset::synthv1(name, path)
    };
    let events = decode_smf(&fs::read(dir.join("recording.mid"))?)?;
    Ok((preset, events))
}

pub fn delete(base: &Path, name: &str) -> Result<()> {
    let safe = safe_name(name);
    if safe.is_empty() || safe != name {
        bail!("invalid idea name");
    }
    let path = base.join(safe);
    if !path.is_dir() {
        bail!("idea does not exist");
    }
    fs::remove_dir_all(path)?;
    Ok(())
}

pub fn save(
    base: &Path,
    name: &str,
    preset: &Preset,
    values: &HashMap<u8, f32>,
    events: &[TimedEvent],
) -> Result<PathBuf> {
    let name = safe_name(name);
    if name.is_empty() {
        bail!("idea name is empty after sanitizing");
    }
    fs::create_dir_all(base)?;
    let final_dir = base.join(&name);
    if final_dir.exists() {
        bail!("idea '{name}' already exists; choose another name or delete it explicitly");
    }
    let tmp = base.join(format!(".{name}.tmp-{}", std::process::id()));
    if tmp.exists() {
        fs::remove_dir_all(&tmp).context("remove stale temporary idea")?;
    }
    fs::create_dir(&tmp)?;
    let result = (|| -> Result<()> {
        write_preset_ref(&tmp.join("preset.ref"), preset)?;
        if let PresetId::Synthv1 { path } = &preset.id {
            fs::copy(path, tmp.join("preset.synthv1"))?;
        }
        fs::write(tmp.join("recording.mid"), encode_smf(events))?;
        let created = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let params = if preset.backend == BackendKind::Synthv1 {
            CONTROLS
                .iter()
                .map(|c| {
                    format!(
                        "    \"{}\": {:.6}",
                        c.xml_name,
                        values.get(&c.cc).copied().unwrap_or(c.min)
                    )
                })
                .collect::<Vec<_>>()
                .join(",\n")
        } else {
            String::new()
        };
        let snapshot = matches!(preset.id, PresetId::Synthv1 { .. }).then_some("preset.synthv1");
        let metadata = format!("{{\n  \"format\": \"shsynth-idea\",\n  \"version\": {FORMAT_VERSION},\n  \"created_unix\": {created},\n  \"backend\": \"{}\",\n  \"preset\": \"{}\",\n  \"preset_snapshot\": {},\n  \"preset_reference\": \"preset.ref\",\n  \"midi\": \"recording.mid\",\n  \"event_count\": {},\n  \"parameters\": {{\n{params}\n  }}\n}}\n", preset.backend.label(), json_escape(&preset.name), snapshot.map(|value| format!("\"{value}\"")).unwrap_or_else(|| "null".into()), events.len());
        fs::write(tmp.join("metadata.json"), metadata)?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, &final_dir).context("atomically publish idea")?;
    Ok(final_dir)
}

fn write_preset_ref(path: &Path, preset: &Preset) -> Result<()> {
    let (source, extra) = match &preset.id {
        PresetId::Synthv1 { .. } => ("preset.synthv1".to_owned(), String::new()),
        PresetId::Yoshimi { path } => (safe_ref_value(&path.to_string_lossy())?, String::new()),
        PresetId::FluidSynth {
            soundfont,
            soundfont_index,
            bank,
            program,
        } => (
            safe_ref_value(&soundfont.to_string_lossy())?,
            format!("soundfont_index={soundfont_index}\nbank={bank}\nprogram={program}\n"),
        ),
    };
    fs::write(
        path,
        format!(
            "backend={}\nname={}\ncategory={}\npath={}\n{extra}",
            preset.backend.label().to_ascii_lowercase(),
            safe_ref_value(&preset.name)?,
            safe_ref_value(preset.category.as_deref().unwrap_or(""))?,
            source
        ),
    )?;
    Ok(())
}

fn read_preset_ref(path: &Path, idea_dir: &Path) -> Result<Preset> {
    let text = fs::read_to_string(path)?;
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name).map(str::to_owned))
    };
    let backend: BackendKind = field("backend=")
        .context("preset reference has no backend")?
        .parse()?;
    let name = field("name=").context("preset reference has no name")?;
    let category = field("category=").filter(|value| !value.is_empty());
    let source = field("path=").context("preset reference has no path")?;
    let source = if backend == BackendKind::Synthv1 {
        idea_dir.join(source)
    } else {
        PathBuf::from(source)
    };
    if !source.is_file() {
        bail!("idea preset source is missing: {}", source.display());
    }
    let id = match backend {
        BackendKind::Synthv1 => PresetId::Synthv1 { path: source },
        BackendKind::Yoshimi => PresetId::Yoshimi { path: source },
        BackendKind::FluidSynth => PresetId::FluidSynth {
            soundfont: source,
            soundfont_index: field("soundfont_index=")
                .context("FluidSynth reference has no SoundFont index")?
                .parse()?,
            bank: field("bank=")
                .context("FluidSynth reference has no bank")?
                .parse()?,
            program: field("program=")
                .context("FluidSynth reference has no program")?
                .parse()?,
        },
    };
    Ok(Preset {
        backend,
        name,
        category,
        id,
    })
}

fn safe_ref_value(value: &str) -> Result<String> {
    if value.contains(['\n', '\r']) {
        bail!("preset reference contains a newline");
    }
    Ok(value.to_owned())
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn encode_smf(events: &[TimedEvent]) -> Vec<u8> {
    const TPQ: u16 = 1000; // At 60 BPM, one tick is one millisecond.
    let mut track = Vec::new();
    let mut previous_ms = 0u64;
    for event in events {
        let ms = event.micros / 1000;
        vlq((ms - previous_ms).min(0x0fff_ffff) as u32, &mut track);
        previous_ms = ms;
        track.extend_from_slice(&event.bytes);
    }
    track.extend_from_slice(&[0, 0xff, 0x2f, 0]);
    let mut out = b"MThd".to_vec();
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&TPQ.to_be_bytes());
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(track.len() as u32).to_be_bytes());
    out.extend(track);
    out
}

fn vlq(mut n: u32, out: &mut Vec<u8>) {
    let mut buf = [0u8; 4];
    let mut i = 3;
    buf[i] = (n & 0x7f) as u8;
    while {
        n >>= 7;
        n != 0
    } {
        i -= 1;
        buf[i] = ((n & 0x7f) as u8) | 0x80;
    }
    out.extend_from_slice(&buf[i..]);
}

pub fn decode_smf(bytes: &[u8]) -> Result<Vec<TimedEvent>> {
    if bytes.len() < 22 || &bytes[..4] != b"MThd" {
        bail!("not a supported MIDI file");
    }
    let division = u16::from_be_bytes([bytes[12], bytes[13]]) as u64;
    if division == 0 {
        bail!("invalid MIDI division");
    }
    let pos = bytes
        .windows(4)
        .position(|w| w == b"MTrk")
        .context("missing MIDI track")?;
    let len = u32::from_be_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
    let mut p = pos + 8;
    let end = (p + len).min(bytes.len());
    let mut ticks = 0u64;
    let mut out = Vec::new();
    while p < end {
        let (delta, n) = read_vlq(&bytes[p..end])?;
        p += n;
        ticks += delta as u64;
        if p >= end {
            break;
        }
        if bytes[p] == 0xff {
            if p + 2 >= end {
                break;
            }
            let l = bytes[p + 2] as usize;
            p = (p + 3 + l).min(end);
            continue;
        }
        let size = match bytes[p] & 0xf0 {
            0xc0 | 0xd0 => 2,
            _ => 3,
        };
        if p + size > end {
            break;
        }
        out.push(TimedEvent {
            micros: ticks * 1_000_000 / division,
            bytes: bytes[p..p + size].to_vec(),
        });
        p += size;
    }
    Ok(out)
}
fn read_vlq(b: &[u8]) -> Result<(u32, usize)> {
    let mut v = 0;
    for (i, x) in b.iter().take(4).enumerate() {
        v = (v << 7) | u32::from(x & 0x7f);
        if x & 0x80 == 0 {
            return Ok((v, i + 1));
        }
    }
    bail!("invalid MIDI delta")
}

pub fn play_events<F: FnMut(&[u8])>(
    events: &[TimedEvent],
    mut send: F,
    stop: &std::sync::atomic::AtomicBool,
) {
    let start = Instant::now();
    for e in events {
        while start.elapsed() < Duration::from_micros(e.micros) {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        send(&e.bytes);
    }
    for m in all_notes_off() {
        send(&m);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timing_and_smf_round_trip() {
        let t = Instant::now();
        let mut r = Recorder::default();
        r.start(t);
        r.capture(t + Duration::from_millis(25), &[0x90, 60, 99]);
        r.capture(t + Duration::from_millis(100), &[0x80, 60, 0]);
        assert_eq!(decode_smf(&encode_smf(&r.events)).unwrap(), r.events);
    }
    #[test]
    fn cleanup_has_all_channels() {
        let m = all_notes_off();
        assert_eq!(m.len(), 16);
        assert!(m.iter().all(|x| x[1] == 123));
    }
    #[test]
    fn filenames_are_safe_and_overwrite_is_refused() {
        assert_eq!(safe_name(" My idea/one "), "My-idea-one");
        let base = std::env::temp_dir().join(format!("shsynth-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("taken")).unwrap();
        let preset_path = base.join("p.synthv1");
        fs::write(&preset_path, "preset").unwrap();
        let preset = Preset::synthv1("p", preset_path);
        assert!(save(&base, "taken", &preset, &HashMap::new(), &[]).is_err());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn backend_identity_round_trips_without_copying_external_sound_data() {
        let base =
            std::env::temp_dir().join(format!("shsynth-idea-backend-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let instrument = base.join("system-instrument.xiz");
        fs::write(&instrument, "external").unwrap();
        let preset = Preset {
            backend: BackendKind::Yoshimi,
            name: "External Bass".into(),
            category: Some("Bass".into()),
            id: PresetId::Yoshimi {
                path: instrument.clone(),
            },
        };
        let saved = save(&base, "idea", &preset, &HashMap::new(), &[]).unwrap();
        assert!(!saved.join("preset.synthv1").exists());
        let (loaded, _) = load(&base, "idea").unwrap();
        assert_eq!(loaded, preset);
        let _ = fs::remove_dir_all(base);
    }
}
