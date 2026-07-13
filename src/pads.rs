use anyhow::{bail, Context, Result};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, Instant};

const DEFAULT_CONTROLLER_CONFIG: &str = include_str!("../config/controller.conf");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PadAction {
    Arp,
    Pad,
    Prog,
    Loop,
    Stop,
    Play,
    Rec,
    TapTempo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncoderAction {
    Up,
    Down,
    Select,
}

impl fmt::Display for PadAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Arp => "arp",
            Self::Pad => "pad",
            Self::Prog => "prog",
            Self::Loop => "loop",
            Self::Stop => "stop",
            Self::Play => "play",
            Self::Rec => "rec",
            Self::TapTempo => "tap-tempo",
        })
    }
}

impl FromStr for PadAction {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "arp" => Ok(Self::Arp),
            "pad" => Ok(Self::Pad),
            "prog" => Ok(Self::Prog),
            "loop" => Ok(Self::Loop),
            "stop" | "stop-record" | "stop-recording" | "panic" | "stop-synth" => Ok(Self::Stop),
            "play" | "play-stop" => Ok(Self::Play),
            "rec" | "record" | "start-recording" => Ok(Self::Rec),
            "tap" | "tap-tempo" => Ok(Self::TapTempo),
            _ => bail!("unknown pad action: {value}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PadConfig {
    pub input_match: Option<String>,
    pub pads: HashMap<u8, PadAction>,
    /// Incoming controller CC -> synthv1 mapped CC from control::CONTROLS.
    pub controls: HashMap<u8, u8>,
    pub encoder_relative_cc: Option<u8>,
    pub encoder_press_cc: Option<u8>,
    /// Dedicated toggle control; this uses the raw Shift CC, not its shifted pad layer.
    pub lock_cc: Option<u8>,
}

impl Default for PadConfig {
    fn default() -> Self {
        let mut config = Self {
            input_match: None,
            pads: HashMap::new(),
            controls: HashMap::new(),
            encoder_relative_cc: None,
            encoder_press_cc: None,
            lock_cc: None,
        };
        config
            .merge(
                DEFAULT_CONTROLLER_CONFIG,
                Path::new("config/controller.conf"),
            )
            .expect("bundled controller.conf must be valid");
        config
    }
}

impl PadConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
        };
        let mut config = Self::default();
        config.merge(&text, path)?;
        Ok(config)
    }

    fn merge(&mut self, text: &str, path: &Path) -> Result<()> {
        let mut saw_pads = false;
        let mut saw_controls = false;
        for (line_no, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line.split_once('=').with_context(|| {
                format!("{}:{}: expected KEY=VALUE", path.display(), line_no + 1)
            })?;
            if key.trim() == "input" {
                self.input_match = (!value.trim().is_empty()).then(|| value.trim().to_owned());
                continue;
            }
            if key.trim() == "encoder.relative_cc" {
                self.encoder_relative_cc = optional_cc(value, "encoder relative CC")?;
                continue;
            }
            if key.trim() == "encoder.press_cc" {
                self.encoder_press_cc = optional_cc(value, "encoder press CC")?;
                continue;
            }
            if key.trim() == "lock.cc" {
                self.lock_cc = optional_cc(value, "pad lock CC")?;
                continue;
            }
            if let Some(raw) = key.trim().strip_prefix("cc.") {
                if !saw_controls {
                    self.controls.clear();
                    saw_controls = true;
                }
                let raw: u8 = raw.parse().context("controller CC must be 0..127")?;
                let target: u8 = value
                    .trim()
                    .parse()
                    .context("target CC must be a mapped CC number")?;
                if crate::control::by_cc(target).is_none() {
                    bail!("target CC {target} is not one of the 12 mapped controls");
                }
                self.controls.insert(raw, target);
                continue;
            }
            if !saw_pads {
                self.pads.clear();
                saw_pads = true;
            }
            let note_text = key.trim().strip_prefix("pad.").unwrap_or(key.trim());
            let note: u8 = note_text.parse().context("pad note must be 0..127")?;
            if note > 127 {
                bail!("pad note must be 0..127");
            }
            self.pads.insert(note, value.trim().parse()?);
        }
        for encoder_cc in [
            self.encoder_relative_cc,
            self.encoder_press_cc,
            self.lock_cc,
        ]
        .into_iter()
        .flatten()
        {
            if self.controls.contains_key(&encoder_cc) {
                bail!("encoder CC {encoder_cc} is also mapped as a synth control");
            }
        }
        if self.encoder_relative_cc == self.encoder_press_cc && self.encoder_relative_cc.is_some() {
            bail!("encoder turn and press CCs must be different");
        }
        if self.lock_cc.is_some()
            && [self.encoder_relative_cc, self.encoder_press_cc].contains(&self.lock_cc)
        {
            bail!("pad lock CC must differ from encoder CCs");
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut entries: Vec<_> = self.pads.iter().collect();
        entries.sort_by_key(|(note, _)| **note);
        let mut text = String::from("# SHSynth controller profile v1\n");
        if let Some(input) = &self.input_match {
            text.push_str(&format!("input={input}\n"));
        }
        text.push_str(&format!(
            "encoder.relative_cc={}\nencoder.press_cc={}\nlock.cc={}\n",
            self.encoder_relative_cc
                .map(|cc| cc.to_string())
                .unwrap_or_default(),
            self.encoder_press_cc
                .map(|cc| cc.to_string())
                .unwrap_or_default(),
            self.lock_cc.map(|cc| cc.to_string()).unwrap_or_default(),
        ));
        let mut controls: Vec<_> = self.controls.iter().collect();
        controls.sort_by_key(|(cc, _)| **cc);
        for (incoming, target) in controls {
            text.push_str(&format!("cc.{incoming}={target}\n"));
        }
        for (note, action) in entries {
            text.push_str(&format!("pad.{note}={action}\n"));
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    /// Returns an action only for note-on with non-zero velocity. Note-off is
    /// consumed too, preventing both stuck notes and double triggering.
    pub fn route(&self, message: &[u8]) -> (bool, Option<PadAction>) {
        if message.len() < 3 {
            return (false, None);
        }
        let kind = message[0] & 0xf0;
        if kind != 0x90 && kind != 0x80 {
            return (false, None);
        }
        match self.pads.get(&message[1]).copied() {
            Some(action) => (true, (kind == 0x90 && message[2] > 0).then_some(action)),
            None => (false, None),
        }
    }

    pub fn action_state(&self, message: &[u8]) -> Option<(PadAction, bool)> {
        if message.len() < 3 {
            return None;
        }
        let kind = message[0] & 0xf0;
        if kind != 0x90 && kind != 0x80 {
            return None;
        }
        self.pads.get(&message[1]).copied().map(|action| {
            let pressed = kind == 0x90 && message[2] > 0;
            (action, pressed)
        })
    }

    pub fn target_cc(&self, incoming: u8) -> Option<u8> {
        self.controls.get(&incoming).copied()
    }

    /// Arturia relative mode uses 64 as stationary, lower values for left and
    /// higher values for right. Press and release are both consumed, while
    /// only a non-zero press selects.
    pub fn encoder_action(&self, message: &[u8]) -> (bool, Option<EncoderAction>) {
        if message.len() < 3 || message[0] & 0xf0 != 0xb0 {
            return (false, None);
        }
        if self.encoder_relative_cc == Some(message[1]) {
            let action = match message[2].cmp(&64) {
                std::cmp::Ordering::Less => Some(EncoderAction::Up),
                std::cmp::Ordering::Greater => Some(EncoderAction::Down),
                std::cmp::Ordering::Equal => None,
            };
            return (true, action);
        }
        if self.encoder_press_cc == Some(message[1]) {
            return (true, (message[2] > 0).then_some(EncoderAction::Select));
        }
        (false, None)
    }

    /// Press and release are consumed; only a non-zero press toggles the lock.
    pub fn lock_action(&self, message: &[u8]) -> (bool, bool) {
        if message.len() < 3 || message[0] & 0xf0 != 0xb0 || self.lock_cc != Some(message[1]) {
            return (false, false);
        }
        (true, message[2] > 0)
    }
}

fn optional_cc(value: &str, description: &str) -> Result<Option<u8>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<u8>()
        .with_context(|| format!("{description} must be 0..127"))
        .map(Some)
}

#[derive(Debug, Default)]
pub struct TapTempo {
    taps: VecDeque<Instant>,
    bpm: Option<f32>,
}

impl TapTempo {
    pub fn tap(&mut self, now: Instant) -> Option<f32> {
        if let Some(last) = self.taps.back() {
            let gap = now.duration_since(*last);
            if !(Duration::from_millis(250)..=Duration::from_secs(2)).contains(&gap) {
                self.taps.clear();
                self.bpm = None;
            }
        }
        self.taps.push_back(now);
        while self.taps.len() > 5 {
            self.taps.pop_front();
        }
        if self.taps.len() >= 2 {
            let mut gaps: Vec<_> = self
                .taps
                .iter()
                .zip(self.taps.iter().skip(1))
                .map(|(a, b)| b.duration_since(*a).as_secs_f32())
                .collect();
            gaps.sort_by(f32::total_cmp);
            let seconds = gaps[gaps.len() / 2];
            self.bpm = Some(60.0 / seconds);
        }
        self.bpm
    }
    pub fn bpm(&self) -> Option<f32> {
        self.bpm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_note_on_triggers_once_and_note_off_is_consumed() {
        let c = PadConfig {
            pads: HashMap::from([(36, PadAction::Rec)]),
            ..PadConfig::default()
        };
        assert_eq!(c.route(&[0x90, 36, 100]), (true, Some(PadAction::Rec)));
        assert_eq!(c.route(&[0x80, 36, 0]), (true, None));
        assert_eq!(c.route(&[0x90, 40, 100]), (false, None));
    }
    #[test]
    fn relative_encoder_turns_and_press_are_consumed() {
        let c = PadConfig {
            encoder_relative_cc: Some(28),
            encoder_press_cc: Some(118),
            ..PadConfig::default()
        };
        assert_eq!(
            c.encoder_action(&[0xb0, 28, 61]),
            (true, Some(EncoderAction::Up))
        );
        assert_eq!(
            c.encoder_action(&[0xb0, 28, 66]),
            (true, Some(EncoderAction::Down))
        );
        assert_eq!(
            c.encoder_action(&[0xb0, 118, 127]),
            (true, Some(EncoderAction::Select))
        );
        assert_eq!(c.encoder_action(&[0xb0, 118, 0]), (true, None));
    }
    #[test]
    fn older_controller_profile_inherits_encoder_defaults() {
        let path =
            std::env::temp_dir().join(format!("shsynth-controller-{}.conf", std::process::id()));
        fs::write(&path, "input=AudioBox USB 96\ncc.86=74\npad.36=arp\n").unwrap();
        let config = PadConfig::load(&path).unwrap();
        assert_eq!(config.input_match.as_deref(), Some("AudioBox USB 96"));
        assert_eq!(config.controls, HashMap::from([(86, 74)]));
        assert_eq!(config.encoder_relative_cc, Some(28));
        assert_eq!(config.encoder_press_cc, Some(118));
        let _ = fs::remove_file(path);
    }
    #[test]
    fn tap_tempo_uses_stable_recent_intervals_and_rejects_long_gap() {
        let t = Instant::now();
        let mut tap = TapTempo::default();
        assert_eq!(tap.tap(t), None);
        assert!((tap.tap(t + Duration::from_millis(500)).unwrap() - 120.0).abs() < 0.1);
        assert_eq!(tap.tap(t + Duration::from_secs(4)), None);
    }
    #[test]
    fn shift_press_toggles_pad_lock_and_release_is_only_consumed() {
        let c = PadConfig {
            lock_cc: Some(27),
            ..PadConfig::default()
        };
        assert_eq!(c.lock_action(&[0xb0, 27, 127]), (true, true));
        assert_eq!(c.lock_action(&[0xb0, 27, 0]), (true, false));
        assert_eq!(c.lock_action(&[0xb0, 28, 127]), (false, false));
    }
}
