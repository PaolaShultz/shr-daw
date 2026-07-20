//! Non-audible controller discovery and MIDI learn.

use crate::control::CONTROLS;
use crate::pads::{ControllerLayout, PadAction, PadConfig};
use anyhow::{anyhow, bail, Context, Result};
use midir::{Ignore, MidiInput, MidiInputConnection};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn input_names() -> Result<Vec<String>> {
    let input = MidiInput::new("SHR-DAW controller discovery")?;
    input
        .ports()
        .iter()
        .map(|port| input.port_name(port).map_err(anyhow::Error::from))
        .collect()
}

pub fn resolve_input(wanted: Option<&str>) -> Result<String> {
    let names = input_names()?;
    resolve_input_name(&names, wanted)
}

pub fn resolve_input_name(names: &[String], wanted: Option<&str>) -> Result<String> {
    if let Some(wanted) = wanted {
        let wanted_lower = wanted.to_ascii_lowercase();
        let matches = names
            .iter()
            .filter(|name| name.to_ascii_lowercase().contains(&wanted_lower))
            .collect::<Vec<_>>();
        return match matches.as_slice() {
            [name] => Ok((*name).clone()),
            [] => bail!("MIDI input not found: {wanted}"),
            _ => bail!("MIDI input match is ambiguous: {wanted}"),
        };
    }
    let candidates = names
        .iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            !lower.contains("midi through") && !lower.contains("shr-daw")
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [name] => Ok((*name).clone()),
        [] => bail!("no external MIDI input detected"),
        _ => bail!(
            "more than one MIDI input detected; pass part of the port name:\n{}",
            candidates
                .iter()
                .map(|name| format!("  {name}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearnRole {
    AbsoluteControl(usize),
    EncoderClockwise,
    EncoderCounterClockwise,
    EncoderClick,
    Pad(usize),
    Confirm,
}

impl LearnRole {
    pub fn label(self) -> String {
        match self {
            Self::AbsoluteControl(index) => {
                format!("CONTROL {} · {}", index + 1, CONTROLS[index].name)
            }
            Self::EncoderClockwise => "MASTER ENCODER · TURN RIGHT".into(),
            Self::EncoderCounterClockwise => "MASTER ENCODER · TURN LEFT".into(),
            Self::EncoderClick => "MASTER ENCODER · CLICK".into(),
            Self::Pad(index) if index < 4 => format!("PAD {} · PAGE {}", index + 1, index + 1),
            Self::Pad(index) => format!("PAD {} · ITEM {}", index + 1, index - 3),
            Self::Confirm => "REVIEW AND SAVE".into(),
        }
    }

    pub const fn skippable(self) -> bool {
        !matches!(self, Self::Confirm)
    }
}

#[derive(Clone, Debug)]
pub struct LearnSession {
    draft: PadConfig,
    step: usize,
    feedback: String,
}

impl LearnSession {
    pub fn new(input_name: &str) -> Self {
        let mut draft = PadConfig::unmapped(stable_input_match(input_name));
        draft.profile = Some("learned".into());
        Self {
            draft,
            step: 0,
            feedback: "Move or press the named hardware control".into(),
        }
    }

    pub fn role(&self) -> LearnRole {
        match self.step {
            0..=11 => LearnRole::AbsoluteControl(self.step),
            12 => LearnRole::EncoderClockwise,
            13 => LearnRole::EncoderCounterClockwise,
            14 => LearnRole::EncoderClick,
            15..=22 => LearnRole::Pad(self.step - 15),
            _ => LearnRole::Confirm,
        }
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.step.min(23) + 1, 24)
    }

    pub fn feedback(&self) -> &str {
        &self.feedback
    }

    pub fn draft(&self) -> &PadConfig {
        &self.draft
    }

    pub fn retry(&mut self) {
        self.feedback = format!("Retry · waiting for {}", self.role().label());
    }

    pub fn skip(&mut self) -> bool {
        if !self.role().skippable() {
            self.feedback = "Save or cancel from the review step".into();
            return false;
        }
        let skipped = self.role().label();
        if self.role() == LearnRole::EncoderClockwise {
            self.step = 14;
        } else {
            self.step += 1;
        }
        self.feedback = format!("Skipped {skipped}");
        true
    }

    pub fn receive(&mut self, message: &[u8]) -> bool {
        let role = self.role();
        let accepted = match role {
            LearnRole::AbsoluteControl(index) => self.learn_absolute(index, message),
            LearnRole::EncoderClockwise => self.learn_encoder_clockwise(message),
            LearnRole::EncoderCounterClockwise => self.learn_encoder_counterclockwise(message),
            LearnRole::EncoderClick => self.learn_click(message),
            LearnRole::Pad(index) => self.learn_pad(index, message),
            LearnRole::Confirm => return false,
        };
        match accepted {
            Ok(description) => {
                self.step += 1;
                self.feedback = format!("Received {description} · OK");
                true
            }
            Err(message) => {
                self.feedback = message;
                false
            }
        }
    }

    fn learn_absolute(&mut self, index: usize, message: &[u8]) -> Result<String, String> {
        if message.len() < 3 || message[0] & 0xf0 != 0xb0 {
            return Err("Expected an absolute knob/fader CC".into());
        }
        let cc = message[1];
        if used_ccs(&self.draft).contains(&cc) {
            return Err(format!("Conflict · CC {cc} is already assigned · retry"));
        }
        self.draft.controls.insert(cc, CONTROLS[index].cc);
        Ok(format!("CC {cc} = {}", CONTROLS[index].name))
    }

    fn learn_encoder_clockwise(&mut self, message: &[u8]) -> Result<String, String> {
        if message.len() < 3 || message[0] & 0xf0 != 0xb0 || message[2] == 64 {
            return Err("Expected a moving relative CC (not value 64)".into());
        }
        let cc = message[1];
        if used_ccs(&self.draft).contains(&cc) {
            return Err(format!("Conflict · CC {cc} is already assigned · retry"));
        }
        self.draft.encoder_relative_cc = Some(cc);
        self.draft.encoder_relative_reverse = message[2] < 64;
        Ok(format!("CC {cc} value {} = right", message[2]))
    }

    fn learn_encoder_counterclockwise(&mut self, message: &[u8]) -> Result<String, String> {
        let Some(cc) = self.draft.encoder_relative_cc else {
            return Err("Learn the clockwise direction first".into());
        };
        if message.len() < 3 || message[0] & 0xf0 != 0xb0 || message[1] != cc {
            return Err(format!("Expected the same encoder CC {cc}"));
        }
        let expected_less = !self.draft.encoder_relative_reverse;
        if message[2] == 64 || (message[2] < 64) != expected_less {
            return Err("Direction conflict · turn the encoder left and retry".into());
        }
        Ok(format!("CC {cc} value {} = left", message[2]))
    }

    fn learn_click(&mut self, message: &[u8]) -> Result<String, String> {
        let button = button_from_message(message, &used_ccs(&self.draft), &used_notes(&self.draft))
            .ok_or_else(|| "Expected an unused CC or note press".to_owned())?;
        match button {
            Button::Cc { cc, channel } => {
                self.draft.encoder_press_cc = Some(cc);
                self.draft.encoder_press_channel = Some(channel);
                Ok(format!("CC {cc} ch {} = encoder click", channel + 1))
            }
            Button::Note { note, channel } => {
                self.draft.encoder_press_note = Some(note);
                self.draft.encoder_press_channel = Some(channel);
                Ok(format!("note {note} ch {} = encoder click", channel + 1))
            }
        }
    }

    fn learn_pad(&mut self, index: usize, message: &[u8]) -> Result<String, String> {
        let action = [
            PadAction::Page1,
            PadAction::Page2,
            PadAction::Page3,
            PadAction::Page4,
            PadAction::Item1,
            PadAction::Item2,
            PadAction::Item3,
            PadAction::Item4,
        ][index];
        let button = button_from_message(message, &used_ccs(&self.draft), &used_notes(&self.draft))
            .ok_or_else(|| "Conflict or release · press an unused pad/button".to_owned())?;
        self.draft.layout = ControllerLayout::Eight;
        match button {
            Button::Cc { cc, channel } => {
                self.draft.cc_buttons.insert(cc, action);
                self.draft.cc_button_channels.insert(cc, channel);
                Ok(format!("CC {cc} ch {} = {action}", channel + 1))
            }
            Button::Note { note, channel } => {
                self.draft.pads.insert(note, action);
                self.draft.pad_channels.insert(note, channel);
                Ok(format!("note {note} ch {} = {action}", channel + 1))
            }
        }
    }

    pub fn validated_config(&self) -> Result<PadConfig> {
        if self.role() != LearnRole::Confirm {
            bail!("MIDI Learn is not at final confirmation");
        }
        self.draft.validate()?;
        Ok(self.draft.clone())
    }

    pub fn confirms_with(&self, message: &[u8]) -> bool {
        self.role() == LearnRole::Confirm
            && (self.draft.encoder_action(message).1 == Some(crate::pads::EncoderAction::Select)
                || self.draft.encoder_note_action(message).1
                    == Some(crate::pads::EncoderAction::Select))
    }
}

pub fn stable_input_match(name: &str) -> String {
    name.split_whitespace()
        .filter(|part| {
            let token = part.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != ':'
            });
            let Some((left, right)) = token.split_once(':') else {
                return true;
            };
            !(left.chars().all(|c| c.is_ascii_digit()) && right.chars().all(|c| c.is_ascii_digit()))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn learn(config: &mut PadConfig, input_name: &str) -> Result<()> {
    let (connection, receiver) = listen(input_name)?;
    let _connection = connection;
    config.input_match = Some(stable_input_match(input_name));
    println!("Listening to {input_name}. MIDI is not being forwarded to an instrument.");

    let missing = CONTROLS
        .iter()
        .filter(|control| !config.controls.values().any(|target| *target == control.cc))
        .count();
    if missing > 0 {
        let count = ask_number(
            &format!("Additional knobs/faders to learn (0-{missing}) [0]: "),
            0,
            missing,
        )?;
        let targets = CONTROLS
            .iter()
            .filter(|control| !config.controls.values().any(|target| *target == control.cc))
            .take(count)
            .copied()
            .collect::<Vec<_>>();
        for control in targets {
            let cc = capture_cc(
                &receiver,
                &format!("Move the control for {}", control.name),
                &used_ccs(config),
            )?;
            config.controls.insert(cc, control.cc);
            println!("  CC {cc} -> {}", control.name);
        }
    }

    if config.encoder_relative_cc.is_none() && ask_yes_no("Learn a main endless encoder? [y/N]: ")?
    {
        let (cc, value) = capture_cc_value(
            &receiver,
            "Turn the main encoder clockwise",
            &used_ccs(config),
        )?;
        if value == 64 {
            bail!("encoder sent only its stationary value; turn it farther and retry");
        }
        config.encoder_relative_cc = Some(cc);
        config.encoder_relative_reverse = value < 64;
        println!("  encoder CC {cc}; direction convention detected");
    }

    if config.encoder_press_cc.is_none()
        && config.encoder_press_note.is_none()
        && ask_yes_no("Learn the main encoder press/select? [y/N]: ")?
    {
        match capture_button(
            &receiver,
            "Press the main encoder",
            &used_ccs(config),
            &used_notes(config),
        )? {
            Button::Cc { cc, .. } => config.encoder_press_cc = Some(cc),
            Button::Note { note, .. } => config.encoder_press_note = Some(note),
        }
    }

    let layout = ask_number("Command buttons available (0, 4, 5, or 8) [0]: ", 0, 8)?;
    if !matches!(layout, 0 | 4 | 5 | 8) {
        bail!("command-button count must be 0, 4, 5, or 8");
    }
    if layout == 0 {
        config.layout = ControllerLayout::Four;
        config.pads.clear();
        config.pad_channels.clear();
        config.cc_buttons.clear();
        config.cc_button_channels.clear();
        config.lock_cc = None;
    }
    if layout > 0 {
        config.layout = match layout {
            4 => ControllerLayout::Four,
            5 => ControllerLayout::Five,
            8 => ControllerLayout::Eight,
            _ => unreachable!(),
        };
        config.pads.clear();
        config.pad_channels.clear();
        config.cc_buttons.clear();
        config.cc_button_channels.clear();
        let actions: &[PadAction] = match layout {
            4 => &[
                PadAction::Item1,
                PadAction::Item2,
                PadAction::Item3,
                PadAction::Item4,
            ],
            5 => &[
                PadAction::CyclePage,
                PadAction::Item1,
                PadAction::Item2,
                PadAction::Item3,
                PadAction::Item4,
            ],
            8 => &[
                PadAction::Page1,
                PadAction::Page2,
                PadAction::Page3,
                PadAction::Page4,
                PadAction::Item1,
                PadAction::Item2,
                PadAction::Item3,
                PadAction::Item4,
            ],
            _ => unreachable!(),
        };
        for &action in actions {
            let binding = capture_button(
                &receiver,
                &format!("Press the button for {action}"),
                &used_ccs(config),
                &used_notes(config),
            )?;
            match binding {
                Button::Cc { cc, channel } => {
                    config.cc_buttons.insert(cc, action);
                    config.cc_button_channels.insert(cc, channel);
                }
                Button::Note { note, channel } => {
                    config.pads.insert(note, action);
                    config.pad_channels.insert(note, channel);
                }
            }
        }
    }

    if config.lock_cc.is_none() && ask_yes_no("Learn an optional command-button lock CC? [y/N]: ")?
    {
        config.lock_cc = Some(capture_cc(
            &receiver,
            "Press the lock control",
            &used_ccs(config),
        )?);
    }
    Ok(())
}

pub fn backup(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for revision in 0..1000 {
        let suffix = if revision == 0 {
            format!("conf.bak-{stamp}")
        } else {
            format!("conf.bak-{stamp}-{revision}")
        };
        let backup = path.with_extension(suffix);
        let mut destination = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        let result = (|| -> Result<()> {
            let mut source = std::fs::File::open(path)?;
            io::copy(&mut source, &mut destination)?;
            destination.sync_all()?;
            std::fs::set_permissions(&backup, source.metadata()?.permissions())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&backup);
        }
        result?;
        return Ok(Some(backup));
    }
    bail!("could not allocate a unique controller backup name")
}

enum Button {
    Cc { cc: u8, channel: u8 },
    Note { note: u8, channel: u8 },
}

fn listen(input_name: &str) -> Result<(MidiInputConnection<()>, Receiver<Vec<u8>>)> {
    let mut input = MidiInput::new("SHR-DAW MIDI learn")?;
    input.ignore(Ignore::None);
    let port = input
        .ports()
        .into_iter()
        .find(|port| input.port_name(port).ok().as_deref() == Some(input_name))
        .with_context(|| format!("MIDI input disappeared: {input_name}"))?;
    let (sender, receiver) = mpsc::channel();
    let connection = input
        .connect(
            &port,
            "SHR-DAW MIDI learn",
            move |_stamp, message, _| {
                let _ = sender.send(message.to_vec());
            },
            (),
        )
        .map_err(|error| anyhow!("open MIDI input for learning: {error}"))?;
    Ok((connection, receiver))
}

fn capture_cc(receiver: &Receiver<Vec<u8>>, prompt: &str, used: &HashSet<u8>) -> Result<u8> {
    capture_cc_value(receiver, prompt, used).map(|(cc, _)| cc)
}

fn capture_cc_value(
    receiver: &Receiver<Vec<u8>>,
    prompt: &str,
    used: &HashSet<u8>,
) -> Result<(u8, u8)> {
    receiver.try_iter().for_each(drop);
    println!("{prompt} …");
    loop {
        let message = receiver.recv().context("MIDI learn input closed")?;
        if message.len() >= 3 && message[0] & 0xf0 == 0xb0 && !used.contains(&message[1]) {
            return Ok((message[1], message[2]));
        }
    }
}

fn capture_button(
    receiver: &Receiver<Vec<u8>>,
    prompt: &str,
    used_ccs: &HashSet<u8>,
    used_notes: &HashSet<u8>,
) -> Result<Button> {
    receiver.try_iter().for_each(drop);
    println!("{prompt} …");
    loop {
        let message = receiver.recv().context("MIDI learn input closed")?;
        if let Some(button) = button_from_message(&message, used_ccs, used_notes) {
            return Ok(button);
        }
    }
}

fn button_from_message(
    message: &[u8],
    used_ccs: &HashSet<u8>,
    used_notes: &HashSet<u8>,
) -> Option<Button> {
    if message.len() < 3 || message[2] == 0 {
        return None;
    }
    match message[0] & 0xf0 {
        0xb0 if !used_ccs.contains(&message[1]) => Some(Button::Cc {
            cc: message[1],
            channel: message[0] & 0x0f,
        }),
        0x90 if !used_notes.contains(&message[1]) => Some(Button::Note {
            note: message[1],
            channel: message[0] & 0x0f,
        }),
        _ => None,
    }
}

fn used_ccs(config: &PadConfig) -> HashSet<u8> {
    config
        .controls
        .keys()
        .chain(config.cc_buttons.keys())
        .copied()
        .chain(
            [
                config.encoder_relative_cc,
                config.encoder_press_cc,
                config.lock_cc,
            ]
            .into_iter()
            .flatten(),
        )
        .collect()
}

fn used_notes(config: &PadConfig) -> HashSet<u8> {
    config
        .pads
        .keys()
        .copied()
        .chain(config.encoder_press_note)
        .collect()
}

fn ask_yes_no(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn ask_number(prompt: &str, default: usize, maximum: usize) -> Result<usize> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim().is_empty() {
        return Ok(default);
    }
    let value = answer
        .trim()
        .parse::<usize>()
        .context("expected a number")?;
    if value > maximum {
        bail!("value must be no more than {maximum}");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unstable_alsa_address_is_removed_from_saved_match() {
        assert_eq!(
            stable_input_match("MiniLab3 MIDI:MiniLab3 MIDI 1 24:0"),
            "MiniLab3 MIDI:MiniLab3 MIDI 1"
        );
    }

    #[test]
    fn button_learning_retains_observed_note_and_cc_channels() {
        match button_from_message(&[0x99, 36, 100], &HashSet::new(), &HashSet::new()).unwrap() {
            Button::Note { note, channel } => {
                assert_eq!((note, channel), (36, 9));
            }
            Button::Cc { .. } => panic!("learned note as CC"),
        }

        match button_from_message(&[0xb2, 44, 127], &HashSet::new(), &HashSet::new()).unwrap() {
            Button::Cc { cc, channel } => {
                assert_eq!((cc, channel), (44, 2));
            }
            Button::Note { .. } => panic!("learned CC as note"),
        }
    }

    #[test]
    fn repeated_backups_do_not_overwrite_each_other() {
        let base =
            std::env::temp_dir().join(format!("shsynth-controller-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let path = base.join("controller.conf");
        std::fs::write(&path, "first").unwrap();
        let first = backup(&path).unwrap().unwrap();
        std::fs::write(&path, "second").unwrap();
        let second = backup(&path).unwrap().unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(first).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(second).unwrap(), "second");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn live_session_learns_absolute_relative_click_and_channel_qualified_pads() {
        let mut learn = LearnSession::new("Test Controller MIDI 44:0");
        assert!(learn.receive(&[0xb0, 10, 20]));
        assert!(!learn.receive(&[0xb0, 10, 30]));
        assert!(learn.feedback().contains("Conflict"));
        for cc in 11..=21 {
            assert!(learn.receive(&[0xb0, cc, 64]));
        }
        assert_eq!(learn.role(), LearnRole::EncoderClockwise);
        assert!(learn.receive(&[0xb0, 28, 65]));
        assert!(learn.receive(&[0xb0, 28, 63]));
        assert!(learn.receive(&[0xb0, 118, 127]));
        for note in 36..=43 {
            assert!(learn.receive(&[0x99, note, 100]));
        }
        assert_eq!(learn.role(), LearnRole::Confirm);
        let config = learn.validated_config().unwrap();
        assert_eq!(config.input_match.as_deref(), Some("Test Controller MIDI"));
        assert_eq!(config.controls.len(), 12);
        assert_eq!(config.encoder_relative_cc, Some(28));
        assert!(!config.encoder_relative_reverse);
        assert_eq!(config.encoder_press_cc, Some(118));
        assert_eq!(config.pads.len(), 8);
        assert!(config.pad_channels.values().all(|channel| *channel == 9));
    }

    #[test]
    fn live_session_skip_retry_and_early_validation_are_non_destructive() {
        let mut learn = LearnSession::new("Unknown Controller");
        learn.retry();
        assert!(learn.feedback().contains("Retry"));
        assert!(learn.skip());
        assert_eq!(learn.role(), LearnRole::AbsoluteControl(1));
        assert!(learn.validated_config().is_err());
    }
}
