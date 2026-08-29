//! Non-audible controller discovery and MIDI learn.

use crate::pads::{
    ControllerButton, ControllerLayout, PadAction, PadConfig, MAPPED_ROTARY_COUNT,
    SECONDARY_CLICK_ROTARY,
};
use anyhow::{anyhow, bail, Context, Result};
use midir::{Ignore, MidiInput, MidiInputConnection};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    RotaryTurn(usize),
    EncoderClockwise,
    EncoderCounterClockwise,
    EncoderClick,
    SecondaryEncoderClick,
    EncoderModifier,
    Pad(usize),
    Confirm,
}

const FIRST_OPTIONAL_STEP: usize = 3;
const CONTROL_STEP_START: usize = FIRST_OPTIONAL_STEP + 1;
const SECONDARY_CLICK_STEP: usize = CONTROL_STEP_START + 8;
const BUTTON_STEP_START: usize = CONTROL_STEP_START + MAPPED_ROTARY_COUNT as usize + 1;
const PAD_LEARN_STEPS: usize = 9;
const CONFIRM_STEP: usize = BUTTON_STEP_START + PAD_LEARN_STEPS;

impl LearnRole {
    fn label(self) -> String {
        match self {
            Self::RotaryTurn(index) => format!("TURN ROTARY {}", index + 2),
            Self::EncoderClockwise => "TURN ROTARY 1 RIGHT".into(),
            Self::EncoderCounterClockwise => "TURN ROTARY 1 LEFT".into(),
            Self::EncoderClick => "CLICK ROTARY 1".into(),
            Self::SecondaryEncoderClick => format!("CLICK ROTARY {SECONDARY_CLICK_ROTARY}"),
            Self::EncoderModifier => "SHIFT + TURN ROTARY 1 LEFT".into(),
            Self::Pad(index) => format!(
                "PRESS PAD {}",
                if index < 4 {
                    index + 1
                } else if index == 4 {
                    1
                } else {
                    index - 4
                }
            ),
            Self::Confirm => "REVIEW · CLICK ROTARY 1 TO SAVE".into(),
        }
    }

    pub const fn skippable(self) -> bool {
        matches!(
            self,
            Self::EncoderModifier | Self::RotaryTurn(_) | Self::Pad(_)
        )
    }
}

#[derive(Clone, Debug)]
pub struct LearnSession {
    draft: PadConfig,
    encoder_left_value: Option<u8>,
    rotary_proof: Option<RotaryProof>,
    step: usize,
    feedback: String,
    state: LearnState,
    trace_path: Option<PathBuf>,
    trace_started: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RotaryProof {
    channel: u8,
    cc: u8,
    left_min: u8,
    left_max: u8,
    right_min: u8,
    right_max: u8,
    reverse: bool,
    left_proven: bool,
    repeats: u8,
}

enum RotaryLearn {
    Pending,
    LeftProven { cc: u8 },
    Complete(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LearnInput {
    Cc { channel: u8, cc: u8 },
    Note { channel: u8, note: u8 },
}

#[derive(Clone, Copy, Debug)]
enum LearnState {
    EntryQuiet {
        deadline: Instant,
    },
    Armed,
    Settling {
        cc: u8,
        deadline: Instant,
    },
    RotaryLeftSettling {
        cc: u8,
        deadline: Instant,
    },
    DirectShiftCandidate {
        channel: u8,
        cc: u8,
        value: u8,
        deadline: Instant,
    },
    ShiftLeftSettling {
        cc: u8,
        modifier: Option<LearnInput>,
        deadline: Instant,
    },
    ShiftLeftRelease {
        modifier: LearnInput,
    },
    ShiftRightArmed {
        modifier: Option<LearnInput>,
    },
    ShiftRightHeld {
        modifier: LearnInput,
    },
    ButtonHeld {
        input: LearnInput,
    },
    EncoderModifierCandidate {
        modifier: LearnInput,
    },
    EncoderModifierChordHeld {
        modifier: LearnInput,
    },
    CycleCandidate {
        modifier: LearnInput,
    },
    CycleConfirm {
        candidate: LearnInput,
    },
    CycleConfirmHeld {
        candidate: LearnInput,
    },
    CycleChordHeld {
        modifier: LearnInput,
    },
    PostRelease {
        deadline: Instant,
    },
    SaveButtonHeld {
        input: LearnInput,
        saved: bool,
    },
    Saved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearnAction {
    None,
    Save,
    FinishSaved,
}

const ENTRY_QUIET: Duration = Duration::from_millis(120);
const GESTURE_SETTLE: Duration = Duration::from_millis(650);
const ERROR_SETTLE: Duration = Duration::from_millis(650);
const SHIFT_GESTURE_SETTLE: Duration = Duration::from_secs(2);

impl LearnInput {
    fn from_message(message: &[u8]) -> Option<Self> {
        if message.len() < 3 {
            return None;
        }
        match message[0] & 0xf0 {
            0xb0 => Some(Self::Cc {
                channel: message[0] & 0x0f,
                cc: message[1],
            }),
            0x90 if message[2] > 0 => Some(Self::Note {
                channel: message[0] & 0x0f,
                note: message[1],
            }),
            _ => None,
        }
    }

    fn matches_message(self, message: &[u8]) -> bool {
        if message.len() < 3 {
            return false;
        }
        match self {
            Self::Cc { channel, cc } => {
                message[0] & 0xf0 == 0xb0 && message[0] & 0x0f == channel && message[1] == cc
            }
            Self::Note { channel, note } => {
                matches!(message[0] & 0xf0, 0x80 | 0x90 | 0xa0)
                    && message[0] & 0x0f == channel
                    && message[1] == note
            }
        }
    }

    fn is_release(self, message: &[u8]) -> bool {
        if !self.matches_message(message) {
            return false;
        }
        match self {
            Self::Cc { .. } => message[2] == 0,
            Self::Note { .. } => {
                message[0] & 0xf0 == 0x80 || (message[0] & 0xf0 == 0x90 && message[2] == 0)
            }
        }
    }

    fn controller_button(self) -> ControllerButton {
        match self {
            Self::Cc { channel, cc } => ControllerButton::Cc { channel, cc },
            Self::Note { channel, note } => ControllerButton::Note { channel, note },
        }
    }
}

impl LearnSession {
    pub fn new_at(input_name: &str, now: Instant) -> Self {
        Self::new_for_profile_at(input_name, None, now)
    }

    pub fn new_for_profile(input_name: &str, profile: Option<&str>) -> Self {
        Self::new_for_profile_at(input_name, profile, Instant::now())
    }

    pub fn new_for_profile_with_trace(
        input_name: &str,
        profile: Option<&str>,
        trace_path: &Path,
    ) -> Self {
        Self::new_for_profile_at_with_trace(input_name, profile, Instant::now(), Some(trace_path))
    }

    pub fn new_for_profile_at(input_name: &str, profile: Option<&str>, now: Instant) -> Self {
        Self::new_for_profile_at_with_trace(input_name, profile, now, None)
    }

    fn new_for_profile_at_with_trace(
        input_name: &str,
        profile: Option<&str>,
        now: Instant,
        trace_path: Option<&Path>,
    ) -> Self {
        let mut draft = PadConfig::unmapped(stable_input_match(input_name));
        draft.profile = Some(profile.unwrap_or("learned").to_owned());
        draft.layout = ControllerLayout::Four;
        let mut session = Self {
            draft,
            encoder_left_value: None,
            rotary_proof: None,
            step: 0,
            feedback: "Release the opening control · waiting for quiet".into(),
            state: LearnState::EntryQuiet {
                deadline: now + ENTRY_QUIET,
            },
            trace_path: None,
            trace_started: now,
        };
        if let Some(path) = trace_path {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)
            {
                let _ = writeln!(
                    file,
                    "START input={:?} profile={:?}",
                    input_name,
                    profile.unwrap_or("learned")
                );
                session.trace_path = Some(path.to_path_buf());
                session.trace_context("ASK", now);
            }
        }
        session
    }

    pub fn role(&self) -> LearnRole {
        match self.step {
            0 => LearnRole::EncoderCounterClockwise,
            1 => LearnRole::EncoderClockwise,
            2 => LearnRole::EncoderClick,
            3 => LearnRole::EncoderModifier,
            CONTROL_STEP_START..BUTTON_STEP_START => {
                if self.step == SECONDARY_CLICK_STEP {
                    LearnRole::SecondaryEncoderClick
                } else {
                    let order_index = self.step
                        - CONTROL_STEP_START
                        - usize::from(self.step > SECONDARY_CLICK_STEP);
                    LearnRole::RotaryTurn(order_index)
                }
            }
            BUTTON_STEP_START..CONFIRM_STEP => LearnRole::Pad(self.step - BUTTON_STEP_START),
            _ => LearnRole::Confirm,
        }
    }

    pub fn role_label(&self) -> String {
        match self.role() {
            LearnRole::EncoderModifier => match self.state {
                LearnState::ShiftLeftSettling { .. } | LearnState::ShiftLeftRelease { .. } => {
                    "RELEASE SHIFT".into()
                }
                LearnState::ShiftRightArmed { .. } | LearnState::ShiftRightHeld { .. } => {
                    "SHIFT + TURN ROTARY 1 RIGHT".into()
                }
                LearnState::EncoderModifierChordHeld { .. } | LearnState::Settling { .. } => {
                    "RELEASE SHIFT".into()
                }
                _ => LearnRole::EncoderModifier.label(),
            },
            LearnRole::RotaryTurn(index) => format!(
                "TURN ROTARY {} {}",
                index + 2,
                if self.rotary_proof.is_some_and(|proof| proof.left_proven)
                    && !matches!(self.state, LearnState::RotaryLeftSettling { .. })
                {
                    "RIGHT"
                } else {
                    "LEFT"
                }
            ),
            LearnRole::Pad(index) => format!("PAD {}", self.pad_for_step(index).number().unwrap()),
            role => role.label(),
        }
    }

    fn pad_for_step(&self, index: usize) -> PadAction {
        let number = match index {
            0..=3 => index as u8 + 1,
            4 => 1,
            5..=8 => {
                let action = index as u8 - 5;
                match self.draft.layout {
                    ControllerLayout::Eight => action + 5,
                    ControllerLayout::Five => action + 2,
                    ControllerLayout::Four => action + 1,
                }
            }
            _ => unreachable!("PAD learn step is bounded"),
        };
        PadAction::physical(number).expect("bounded physical PAD")
    }

    pub fn feedback(&self) -> &str {
        &self.feedback
    }

    pub fn prompt_line(&self) -> String {
        if self.feedback_is_error() {
            if self.feedback.starts_with("POSITIONAL") {
                return "POSITIONAL · release; retry same step".into();
            }
            if self.feedback.starts_with("DIRECTION") {
                return format!("{} · release; retry", self.feedback);
            }
            return "Not accepted · release; retry step".into();
        }
        match self.state {
            LearnState::ShiftLeftSettling { .. } => {
                return "Next: Shift + turn R1 right".into();
            }
            LearnState::ShiftLeftRelease { .. } => {
                return "Next: Shift + turn R1 right".into();
            }
            LearnState::ShiftRightArmed { .. } => {
                return "Turn right; release Shift when done".into();
            }
            LearnState::ShiftRightHeld { .. } => {
                return "Turn R1 right; release when done".into();
            }
            LearnState::EncoderModifierChordHeld { .. } | LearnState::Settling { .. }
                if self.role() == LearnRole::EncoderModifier =>
            {
                return "Axis learned; release Shift".into();
            }
            LearnState::RotaryLeftSettling { .. } => {
                return "Finish left turn; wait".into();
            }
            _ => {}
        }
        if let LearnRole::RotaryTurn(index) = self.role() {
            if self.rotary_proof.is_some_and(|proof| proof.left_proven) {
                return format!("Turn R{} right slowly", index + 2);
            }
            return format!("Turn R{} left slowly", index + 2);
        }
        match self.state {
            LearnState::EntryQuiet { .. } => "Release controls".into(),
            LearnState::Armed => match self.role() {
                LearnRole::EncoderCounterClockwise => "Turn left now".into(),
                LearnRole::EncoderClockwise => "Turn right now".into(),
                LearnRole::EncoderClick => "Press and release".into(),
                LearnRole::EncoderModifier => "Turn left; release Shift when done".into(),
                LearnRole::SecondaryEncoderClick => "Press and release".into(),
                LearnRole::RotaryTurn(index) => format!("Turn R{}", index + 2),
                LearnRole::Pad(index) => {
                    format!("Press PAD {}", self.pad_for_step(index).number().unwrap())
                }
                LearnRole::Confirm => "Click rotary 1 to save".into(),
            },
            LearnState::DirectShiftCandidate { .. } => {
                "Turn R1 left; release Shift when done".into()
            }
            LearnState::Settling { .. } | LearnState::PostRelease { .. } => {
                "OK · finish movement".into()
            }
            LearnState::RotaryLeftSettling { .. } => "Finish left turn; wait".into(),
            LearnState::ShiftLeftSettling { .. } => "Next: Shift + turn R1 right".into(),
            LearnState::ShiftLeftRelease { .. } => "Release Shift; then Shift + turn right".into(),
            LearnState::ShiftRightArmed { .. } => "Turn right; release Shift when done".into(),
            LearnState::ShiftRightHeld { .. } => "Turn R1 right; release when done".into(),
            LearnState::ButtonHeld { .. } => "OK · release button".into(),
            LearnState::EncoderModifierCandidate { .. } => {
                "Turn R1 left; release Shift when done".into()
            }
            LearnState::EncoderModifierChordHeld { .. } => "Axis learned; release Shift".into(),
            LearnState::CycleCandidate { .. } => "Hold it; move the page control".into(),
            LearnState::CycleConfirm { .. } => "Press it again to use it alone".into(),
            LearnState::CycleConfirmHeld { .. } => "Release to confirm this button".into(),
            LearnState::CycleChordHeld { .. } => "OK · release the modifier".into(),
            LearnState::SaveButtonHeld { saved, .. } => {
                if saved {
                    "Saved · release rotary 1".into()
                } else {
                    "Saving · keep rotary 1 held".into()
                }
            }
            LearnState::Saved => "Saved".into(),
        }
    }

    pub fn feedback_is_error(&self) -> bool {
        let feedback = self.feedback.to_ascii_lowercase();
        feedback.starts_with("conflict")
            || feedback.starts_with("expected")
            || feedback.starts_with("no position")
            || feedback.starts_with("learn the")
            || feedback.starts_with("rotary")
            || feedback.starts_with("positional")
            || feedback.starts_with("direction")
            || feedback.starts_with("shift released")
            || feedback.starts_with("save failed")
    }

    pub fn draft(&self) -> &PadConfig {
        &self.draft
    }

    fn trace_state_name(&self) -> &'static str {
        match self.state {
            LearnState::EntryQuiet { .. } => "entry-quiet",
            LearnState::Armed => "armed",
            LearnState::Settling { .. } => "settling",
            LearnState::RotaryLeftSettling { .. } => "rotary-left-settling",
            LearnState::DirectShiftCandidate { .. } => "direct-shift-candidate",
            LearnState::ShiftLeftSettling { .. } => "shift-left-settling",
            LearnState::ShiftLeftRelease { .. } => "shift-left-release",
            LearnState::ShiftRightArmed { .. } => "shift-right-armed",
            LearnState::ShiftRightHeld { .. } => "shift-right-held",
            LearnState::ButtonHeld { .. } => "button-held",
            LearnState::EncoderModifierCandidate { .. } => "modifier-candidate",
            LearnState::EncoderModifierChordHeld { .. } => "modifier-chord-held",
            LearnState::CycleCandidate { .. } => "cycle-candidate",
            LearnState::CycleConfirm { .. } => "cycle-confirm",
            LearnState::CycleConfirmHeld { .. } => "cycle-confirm-held",
            LearnState::CycleChordHeld { .. } => "cycle-chord-held",
            LearnState::PostRelease { .. } => "post-release",
            LearnState::SaveButtonHeld { .. } => "save-button-held",
            LearnState::Saved => "saved",
        }
    }

    fn trace_summary(&self) -> String {
        format!(
            "step={:?} state={} prompt={:?} feedback={:?}",
            self.role_label(),
            self.trace_state_name(),
            self.prompt_line(),
            self.feedback
        )
    }

    fn trace_line(&self, event: &str, now: Instant, detail: &str) {
        let Some(path) = self.trace_path.as_ref() else {
            return;
        };
        let elapsed = now
            .saturating_duration_since(self.trace_started)
            .as_millis();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "+{elapsed:06}ms {event} {detail}");
        }
    }

    fn trace_context(&self, event: &str, now: Instant) {
        self.trace_line(event, now, &self.trace_summary());
    }

    fn trace_context_if_changed(&self, before: Option<String>, now: Instant) {
        if before.is_some_and(|before| before != self.trace_summary()) {
            self.trace_context("ASK", now);
        }
    }

    fn trace_input(&self, message: &[u8], now: Instant) {
        if self.trace_path.is_none() {
            return;
        }
        let bytes = message
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        self.trace_line(
            "INPUT",
            now,
            &format!(
                "step={:?} state={} bytes={bytes}",
                self.role_label(),
                self.trace_state_name()
            ),
        );
    }

    pub fn tick(&mut self, now: Instant) {
        let before = self.trace_path.as_ref().map(|_| self.trace_summary());
        match self.state {
            LearnState::EntryQuiet { deadline } if now >= deadline => {
                self.state = LearnState::Armed;
                self.feedback = format!("Ready · {}", self.role_label());
            }
            LearnState::Settling { deadline, .. } if now >= deadline => {
                self.advance_after_capture();
            }
            LearnState::RotaryLeftSettling { deadline, .. } if now >= deadline => {
                self.state = LearnState::Armed;
                self.feedback = format!("Ready · {}", self.role_label());
            }
            LearnState::DirectShiftCandidate { deadline, .. } if now >= deadline => {
                self.clear_current_mapping();
                self.state = LearnState::Armed;
                self.feedback = "Shift press ignored · Shift + turn rotary 1 left".into();
            }
            LearnState::ShiftLeftSettling {
                modifier, deadline, ..
            } if now >= deadline => {
                if let Some(modifier) = modifier {
                    self.state = LearnState::ShiftLeftRelease { modifier };
                    self.feedback = "Left verified · release Shift".into();
                } else {
                    self.state = LearnState::ShiftRightArmed { modifier: None };
                    self.feedback = "Left verified · Shift + turn right".into();
                }
            }
            LearnState::PostRelease { deadline } if now >= deadline => {
                self.advance_after_capture();
            }
            _ => {}
        }
        self.trace_context_if_changed(before, now);
    }

    pub fn retry(&mut self) {
        self.retry_at(Instant::now());
    }

    pub fn retry_at(&mut self, now: Instant) {
        let before = self.trace_path.as_ref().map(|_| self.trace_summary());
        self.retry_at_inner(now);
        self.trace_context_if_changed(before, now);
    }

    fn retry_at_inner(&mut self, now: Instant) {
        if matches!(
            self.state,
            LearnState::Saved | LearnState::SaveButtonHeld { saved: true, .. }
        ) {
            self.feedback = "Profile saved · release the encoder to exit".into();
            return;
        }
        self.clear_current_mapping();
        self.state = LearnState::EntryQuiet {
            deadline: now + ENTRY_QUIET,
        };
        self.feedback = format!(
            "Retry · release control, then wait for {}",
            self.role_label()
        );
    }

    pub fn previous(&mut self) -> bool {
        let now = Instant::now();
        let before = self.trace_path.as_ref().map(|_| self.trace_summary());
        let changed = self.previous_inner();
        self.trace_context_if_changed(before, now);
        changed
    }

    fn previous_inner(&mut self) -> bool {
        if !matches!(self.state, LearnState::Armed) {
            return false;
        }
        if self.step <= FIRST_OPTIONAL_STEP {
            self.feedback = "Master encoder setup is complete · browse optional mappings".into();
            return false;
        }
        self.step_backward();
        self.feedback = format!("Selected {}", self.role_label());
        true
    }

    pub fn skip(&mut self) -> bool {
        let now = Instant::now();
        let before = self.trace_path.as_ref().map(|_| self.trace_summary());
        let changed = self.skip_inner();
        self.trace_context_if_changed(before, now);
        changed
    }

    fn skip_inner(&mut self) -> bool {
        if !matches!(self.state, LearnState::Armed) {
            return false;
        }
        if !self.role().skippable() {
            self.feedback = if self.can_finish() {
                "Go to Review and click rotary 1 to save".into()
            } else {
                "Rotary 1 turn/click and rotary 9 click are required".into()
            };
            return false;
        }
        let skipped = self.role_label();
        self.step_forward();
        self.feedback = format!("Skipped {skipped}");
        true
    }

    fn master_step_direction(&self, message: &[u8]) -> Option<i8> {
        if self.step < 2
            || !(matches!(self.state, LearnState::Armed)
                || matches!(self.state, LearnState::EntryQuiet { .. }) && self.feedback_is_error())
            || message.len() < 3
            || message[0] & 0xf0 != 0xb0
            || Some(message[1]) != self.draft.encoder_relative_cc
        {
            return None;
        }
        match (self.draft.encoder_relative_reverse, message[2]) {
            (false, 61..=63) | (true, 125..=127) => Some(-1),
            (false, 65..=67) | (true, 1..=3) => Some(1),
            _ => None,
        }
    }

    fn change_step_with_master(&mut self, direction: i8, now: Instant) {
        let changed = if direction < 0 {
            if self.step <= 2 {
                false
            } else {
                self.step_backward();
                true
            }
        } else if self.step >= CONFIRM_STEP {
            false
        } else {
            self.step_forward();
            true
        };
        self.state = LearnState::EntryQuiet {
            deadline: now + ENTRY_QUIET,
        };
        self.feedback = if changed {
            format!("Selected {}", self.role_label())
        } else if direction < 0 {
            "Rotary 1 setup is the first fixed step".into()
        } else {
            "Review is the final step".into()
        };
    }

    pub fn receive(&mut self, message: &[u8], now: Instant) -> LearnAction {
        let before = self.trace_path.as_ref().map(|_| self.trace_summary());
        self.trace_input(message, now);
        let action = self.receive_inner(message, now);
        self.trace_context_if_changed(before, now);
        action
    }

    fn receive_inner(&mut self, message: &[u8], now: Instant) -> LearnAction {
        if let Some(direction) = self.master_step_direction(message) {
            self.change_step_with_master(direction, now);
            return LearnAction::None;
        }
        match self.state {
            LearnState::EntryQuiet { ref mut deadline } => {
                if message_marks_activity(message) {
                    *deadline = now + ENTRY_QUIET;
                }
                return LearnAction::None;
            }
            LearnState::Settling { cc, .. } => {
                if cc_message(message, cc) {
                    if self.step == 0 && matches!(message[2], 61..=63 | 125..=127) {
                        self.encoder_left_value = Some(message[2]);
                    }
                    self.state = LearnState::Settling {
                        cc,
                        deadline: now + GESTURE_SETTLE,
                    };
                }
                return LearnAction::None;
            }
            LearnState::RotaryLeftSettling { cc, .. } => {
                if cc_message(message, cc) {
                    self.state = LearnState::RotaryLeftSettling {
                        cc,
                        deadline: now + GESTURE_SETTLE,
                    };
                }
                return LearnAction::None;
            }
            LearnState::DirectShiftCandidate {
                channel, cc, value, ..
            } => {
                let same_candidate = message.len() >= 3
                    && message[0] & 0xf0 == 0xb0
                    && message[0] & 0x0f == channel
                    && message[1] == cc;
                if same_candidate && message[2] == 0 {
                    self.rotary_proof = None;
                    self.state = LearnState::Armed;
                    self.feedback = "Shift press ignored · Shift + turn left".into();
                    return LearnAction::None;
                }
                if moving_cc(message).is_some() {
                    self.state = LearnState::Armed;
                    if same_candidate {
                        let candidate = [0xb0 | channel, cc, value];
                        self.capture_shift_rotary(None, &candidate, now);
                    }
                    self.capture_shift_rotary(None, message, now);
                }
                return LearnAction::None;
            }
            LearnState::ShiftLeftSettling {
                cc,
                modifier,
                deadline: _,
            } => {
                if modifier.is_some_and(|input| input.is_release(message)) {
                    self.state = LearnState::ShiftRightArmed { modifier };
                    self.feedback = "Left verified · Shift + turn right".into();
                } else if cc_message(message, cc) {
                    self.state = LearnState::ShiftLeftSettling {
                        cc,
                        modifier,
                        deadline: now + GESTURE_SETTLE,
                    };
                }
                return LearnAction::None;
            }
            LearnState::ShiftLeftRelease { modifier } => {
                if modifier.is_release(message) {
                    self.state = LearnState::ShiftRightArmed {
                        modifier: Some(modifier),
                    };
                    self.feedback = "Left verified · Shift + turn right".into();
                }
                return LearnAction::None;
            }
            LearnState::ShiftRightArmed { modifier } => {
                if let Some(modifier) = modifier {
                    if !modifier.is_release(message) && modifier.matches_message(message) {
                        self.state = LearnState::ShiftRightHeld { modifier };
                        self.feedback = "Shift pressed · turn rotary 1 right".into();
                    }
                } else if self
                    .rotary_proof
                    .is_some_and(|proof| cc_message(message, proof.cc))
                {
                    self.capture_shift_rotary(None, message, now);
                }
                return LearnAction::None;
            }
            LearnState::ShiftRightHeld { modifier } => {
                if modifier.is_release(message) {
                    let repeats = self.rotary_proof.map(|proof| proof.repeats).unwrap_or(0);
                    self.state = LearnState::ShiftRightArmed {
                        modifier: Some(modifier),
                    };
                    self.feedback = format!("Right {repeats}/3 · Shift + turn right again");
                } else {
                    self.capture_shift_rotary(Some(modifier), message, now);
                }
                return LearnAction::None;
            }
            LearnState::ButtonHeld { input } => {
                if input.is_release(message) {
                    self.advance_after_capture();
                }
                return LearnAction::None;
            }
            LearnState::EncoderModifierCandidate { modifier } => {
                if modifier.is_release(message) {
                    self.state = LearnState::Armed;
                    let repeats = self.rotary_proof.map(|proof| proof.repeats).unwrap_or(0);
                    self.feedback = format!("Left {repeats}/3 · Shift + turn left again");
                } else if let Some((cc, _)) = moving_cc(message) {
                    let modifier_cc = match modifier {
                        LearnInput::Cc { cc, .. } => Some(cc),
                        LearnInput::Note { .. } => None,
                    };
                    if Some(cc) == modifier_cc {
                        return LearnAction::None;
                    }
                    self.capture_shift_rotary(Some(modifier), message, now);
                }
                return LearnAction::None;
            }
            LearnState::EncoderModifierChordHeld { modifier } => {
                if modifier.is_release(message) {
                    self.advance_after_capture();
                }
                return LearnAction::None;
            }
            LearnState::CycleCandidate { modifier } => {
                if modifier.is_release(message) {
                    self.state = LearnState::CycleConfirm {
                        candidate: modifier,
                    };
                    self.feedback =
                        "No chord seen · press the same page-switch button again to confirm".into();
                } else if let Some(trigger) = LearnInput::from_message(message) {
                    if trigger != modifier {
                        self.draft.page_cycle_modifier = Some(modifier.controller_button());
                        self.draft.page_cycle_trigger = Some(trigger.controller_button());
                        self.draft.layout = ControllerLayout::Five;
                        self.state = LearnState::CycleChordHeld { modifier };
                        self.feedback = format!(
                            "Learned {} + {} = page-cycle · OK · release modifier",
                            learn_input_description(modifier),
                            learn_input_description(trigger)
                        );
                    }
                }
                return LearnAction::None;
            }
            LearnState::CycleConfirm { candidate } => {
                if let Some(input) = LearnInput::from_message(message) {
                    if input == candidate {
                        self.state = LearnState::CycleConfirmHeld { candidate };
                        self.feedback =
                            "Release to confirm this button, or keep holding and use a trigger"
                                .into();
                    } else {
                        self.state = LearnState::CycleCandidate { modifier: input };
                        self.feedback =
                            "Modifier held · now move or press the page-switch control".into();
                    }
                }
                return LearnAction::None;
            }
            LearnState::CycleConfirmHeld { candidate } => {
                if candidate.is_release(message) {
                    match self.learn_pad_input(4, candidate) {
                        Ok(description) => {
                            self.feedback = format!("Learned {description} · OK");
                            self.state = LearnState::PostRelease {
                                deadline: now + GESTURE_SETTLE,
                            };
                        }
                        Err(error) => {
                            self.state = LearnState::Armed;
                            self.feedback = error;
                        }
                    }
                } else if let Some(trigger) = LearnInput::from_message(message) {
                    if trigger != candidate {
                        self.draft.page_cycle_modifier = Some(candidate.controller_button());
                        self.draft.page_cycle_trigger = Some(trigger.controller_button());
                        self.draft.layout = ControllerLayout::Five;
                        self.state = LearnState::CycleChordHeld {
                            modifier: candidate,
                        };
                        self.feedback = format!(
                            "Learned {} + {} = page-cycle · OK · release modifier",
                            learn_input_description(candidate),
                            learn_input_description(trigger)
                        );
                    }
                }
                return LearnAction::None;
            }
            LearnState::CycleChordHeld { modifier } => {
                if modifier.is_release(message) {
                    self.advance_after_capture();
                }
                return LearnAction::None;
            }
            LearnState::PostRelease { .. } => return LearnAction::None,
            LearnState::SaveButtonHeld { input, saved } => {
                if input.is_release(message) {
                    if saved {
                        self.state = LearnState::Saved;
                        return LearnAction::FinishSaved;
                    }
                    self.state = LearnState::Armed;
                    self.feedback = "Save failed · release received · ready to retry".into();
                }
                return LearnAction::None;
            }
            LearnState::Saved => return LearnAction::None,
            LearnState::Armed => {}
        }

        let role = self.role();
        if self.can_finish() {
            let action = {
                let cc_action = self
                    .draft
                    .encoder_action_with_modifier_and_state(message, false);
                if cc_action.0 {
                    (cc_action.0, cc_action.1)
                } else {
                    self.draft.encoder_note_action(message)
                }
            };
            if action.0 {
                if role == LearnRole::Confirm
                    && action.1 == Some(crate::pads::EncoderAction::Select)
                {
                    if let Some(input) = LearnInput::from_message(message) {
                        self.state = LearnState::SaveButtonHeld {
                            input,
                            saved: false,
                        };
                        self.feedback = "Save requested · keep the encoder held".into();
                        return LearnAction::Save;
                    }
                }
                return LearnAction::None;
            }
        }
        if role == LearnRole::Confirm {
            return LearnAction::None;
        }

        if !message_is_relevant(role, message) {
            return LearnAction::None;
        }
        if self.role_is_mapped() {
            self.clear_current_mapping();
        }
        if role == LearnRole::Pad(4) {
            if let Some(modifier) = LearnInput::from_message(message) {
                self.state = LearnState::CycleCandidate { modifier };
                self.feedback = "Modifier held · now move or press the page-switch control".into();
            }
            return LearnAction::None;
        }
        if role == LearnRole::EncoderModifier {
            let direct_hardware_shift =
                self.draft.profile.as_deref() == Some("arturia-minilab-mkii");
            if direct_hardware_shift {
                let Some((cc, value)) = moving_cc(message) else {
                    self.feedback = "Shift + turn rotary 1 left".into();
                    return LearnAction::None;
                };
                if self.rotary_proof.is_none()
                    && value == 127
                    && !self.draft.encoder_relative_reverse
                {
                    self.state = LearnState::DirectShiftCandidate {
                        channel: message[0] & 0x0f,
                        cc,
                        value,
                        deadline: now + SHIFT_GESTURE_SETTLE,
                    };
                    self.feedback = "Shift pressed · turn rotary 1 left".into();
                } else {
                    self.capture_shift_rotary(None, message, now);
                }
                return LearnAction::None;
            }
            let input = match self.learn_encoder_modifier_button(message) {
                Ok(input) => input,
                Err(message) => {
                    self.feedback = message;
                    return LearnAction::None;
                }
            };
            self.state = LearnState::EncoderModifierCandidate { modifier: input };
            self.feedback = format!(
                "{} held · now turn rotary 1 left",
                learn_input_description(input)
            );
            return LearnAction::None;
        }
        if let LearnRole::RotaryTurn(index) = role {
            match self.learn_rotary(index, message) {
                Ok(RotaryLearn::Complete(description)) => {
                    let Some(cc) = cc_number(message) else {
                        return LearnAction::None;
                    };
                    self.state = LearnState::Settling {
                        cc,
                        deadline: now + GESTURE_SETTLE,
                    };
                    self.feedback = format!("Learned {description} · OK · finish movement");
                }
                Ok(RotaryLearn::LeftProven { cc }) => {
                    self.state = LearnState::RotaryLeftSettling {
                        cc,
                        deadline: now + GESTURE_SETTLE,
                    };
                    self.feedback = "Left direction verified · finish the turn".into();
                }
                Ok(RotaryLearn::Pending) => {}
                Err(message) => {
                    self.clear_current_mapping();
                    self.state = LearnState::EntryQuiet {
                        deadline: now + ERROR_SETTLE,
                    };
                    self.feedback = message;
                }
            }
            return LearnAction::None;
        }
        let accepted = match role {
            LearnRole::RotaryTurn(_) => unreachable!("handled as a proven relative stream"),
            LearnRole::EncoderCounterClockwise => self.learn_encoder_counterclockwise(message),
            LearnRole::EncoderClockwise => self.learn_encoder_clockwise(message),
            LearnRole::EncoderClick => self.learn_click(message),
            LearnRole::SecondaryEncoderClick => self.learn_secondary_click(message),
            LearnRole::EncoderModifier => unreachable!("handled as a held Shift+rotary chord"),
            LearnRole::Pad(index) => self.learn_pad(index, message),
            LearnRole::Confirm => return LearnAction::None,
        };
        match accepted {
            Ok(description) => {
                if matches!(
                    role,
                    LearnRole::EncoderClick | LearnRole::SecondaryEncoderClick | LearnRole::Pad(_)
                ) {
                    let Some(input) = LearnInput::from_message(message) else {
                        return LearnAction::None;
                    };
                    self.state = LearnState::ButtonHeld { input };
                    self.feedback = format!("Learned {description} · OK · release to continue");
                } else {
                    let Some(cc) = cc_number(message) else {
                        return LearnAction::None;
                    };
                    self.state = LearnState::Settling {
                        cc,
                        deadline: now + GESTURE_SETTLE,
                    };
                    self.feedback = format!("Learned {description} · OK · finish movement");
                }
                LearnAction::None
            }
            Err(message) => {
                self.feedback = message;
                LearnAction::None
            }
        }
    }

    pub fn mark_save_result(&mut self, saved: bool) {
        if let LearnState::SaveButtonHeld {
            saved: ref mut state,
            ..
        } = self.state
        {
            *state = saved;
            self.feedback = if saved {
                "Profile saved and activated · release encoder to exit".into()
            } else {
                "Save failed · release encoder before retrying".into()
            };
        }
    }

    pub fn save_committed(&self) -> bool {
        matches!(
            self.state,
            LearnState::SaveButtonHeld { saved: true, .. } | LearnState::Saved
        )
    }

    fn advance_after_capture(&mut self) {
        self.step_forward();
        self.state = LearnState::Armed;
        self.feedback = if self.role() == LearnRole::Confirm {
            "Learning complete · click rotary 1 to save".into()
        } else {
            format!("Ready · {}", self.role_label())
        };
    }

    fn step_forward(&mut self) {
        self.rotary_proof = None;
        self.step = (self.step + 1).min(CONFIRM_STEP);
        if self.role() == LearnRole::Pad(4) && !self.cycle_page_role_needed() {
            self.step = (self.step + 1).min(CONFIRM_STEP);
        }
    }

    fn step_backward(&mut self) {
        self.rotary_proof = None;
        self.step = self.step.saturating_sub(1);
        if self.role() == LearnRole::Pad(4) && !self.cycle_page_role_needed() {
            self.step = self.step.saturating_sub(1);
        }
    }

    fn cycle_page_role_needed(&self) -> bool {
        self.draft.layout != ControllerLayout::Eight
    }

    fn learn_rotary(&mut self, index: usize, message: &[u8]) -> Result<RotaryLearn, String> {
        if message.len() < 3 || message[0] & 0xf0 != 0xb0 {
            return Err("Expected a relative rotary CC".into());
        }
        let channel = message[0] & 0x0f;
        let cc = message[1];
        if self
            .rotary_proof
            .is_some_and(|proof| proof.channel != channel || proof.cc != cc)
        {
            return Ok(RotaryLearn::Pending);
        }
        let value = message[2];
        let neutral = if self.draft.encoder_relative_reverse {
            value == 0
        } else {
            value == 64
        };
        if neutral {
            return Ok(RotaryLearn::Pending);
        }
        let learning_right = self.rotary_proof.is_some_and(|proof| proof.left_proven);
        let expected_value = if self.draft.encoder_relative_reverse {
            if learning_right {
                matches!(value, 1..=3)
            } else {
                matches!(value, 125..=127)
            }
        } else if learning_right {
            matches!(value, 65..=67)
        } else {
            matches!(value, 61..=63)
        };
        if !expected_value {
            return Err(if learning_right {
                "DIRECTION · turn right".into()
            } else {
                self.rotary_proof = None;
                "POSITIONAL · set Relative 1/2".into()
            });
        }
        if used_ccs(&self.draft).contains(&cc) {
            return Err(format!("Conflict · CC {cc} is already assigned · retry"));
        }
        let repeats = match self.rotary_proof {
            Some(proof) => proof.repeats.saturating_add(1),
            None => 1,
        };
        let mut proof = self.rotary_proof.unwrap_or(RotaryProof {
            channel,
            cc,
            left_min: if self.draft.encoder_relative_reverse {
                125
            } else {
                61
            },
            left_max: if self.draft.encoder_relative_reverse {
                127
            } else {
                63
            },
            right_min: if self.draft.encoder_relative_reverse {
                1
            } else {
                65
            },
            right_max: if self.draft.encoder_relative_reverse {
                3
            } else {
                67
            },
            reverse: self.draft.encoder_relative_reverse,
            left_proven: false,
            repeats: 0,
        });
        proof.repeats = repeats;
        self.rotary_proof = Some(proof);
        if repeats < 3 {
            return Ok(RotaryLearn::Pending);
        }
        if !learning_right {
            if let Some(proof) = self.rotary_proof.as_mut() {
                proof.left_proven = true;
                proof.repeats = 0;
            }
            return Ok(RotaryLearn::LeftProven { cc });
        }
        self.draft.controls.insert(cc, index as u8 + 1);
        self.rotary_proof = None;
        Ok(RotaryLearn::Complete(format!(
            "CC {cc} = ROTARY {}",
            index + 2
        )))
    }

    fn learn_encoder_clockwise(&mut self, message: &[u8]) -> Result<String, String> {
        let Some(cc) = self.draft.encoder_relative_cc else {
            return Err("Learn the counterclockwise direction first".into());
        };
        if message.len() < 3 || message[0] & 0xf0 != 0xb0 || message[1] != cc {
            return Err(format!("Expected the same encoder CC {cc}"));
        }
        let left = self
            .encoder_left_value
            .ok_or_else(|| "Learn the counterclockwise direction first".to_owned())?;
        let right = message[2];
        if (61..=63).contains(&left) && (65..=67).contains(&right) {
            self.draft.encoder_relative_reverse = false;
            return Ok(format!("CC {cc} = relative 1 navigation · right"));
        }
        if (125..=127).contains(&left) && (1..=3).contains(&right) {
            self.draft.encoder_relative_reverse = true;
            return Ok(format!("CC {cc} = relative 2 navigation · right"));
        }
        Err("Rotary 1 is not sending relative steps · set the controller to Relative 1 or 2".into())
    }

    fn learn_encoder_counterclockwise(&mut self, message: &[u8]) -> Result<String, String> {
        let Some((cc, value)) = moving_cc(message) else {
            return Err("Expected a moving relative CC".into());
        };
        if used_ccs(&self.draft).contains(&cc) {
            return Err(format!("Conflict · CC {cc} is already assigned · retry"));
        }
        self.draft.encoder_relative_cc = Some(cc);
        self.draft.encoder_relative_reverse = false;
        self.encoder_left_value = Some(value);
        Ok(format!("CC {cc} value {value} = left"))
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

    fn learn_secondary_click(&mut self, message: &[u8]) -> Result<String, String> {
        let button = button_from_message(message, &used_ccs(&self.draft), &used_notes(&self.draft))
            .ok_or_else(|| "Expected an unused CC or note press".to_owned())?;
        match button {
            Button::Cc { cc, channel } => {
                self.draft.secondary_encoder_press_cc = Some(cc);
                self.draft.secondary_encoder_press_channel = Some(channel);
                self.draft.synth_press_cc = Some(cc);
                self.draft.synth_press_channel = Some(channel);
                Ok(format!(
                    "CC {cc} ch {} = rotary {SECONDARY_CLICK_ROTARY} synth click",
                    channel + 1
                ))
            }
            Button::Note { note, channel } => {
                self.draft.secondary_encoder_press_note = Some(note);
                self.draft.secondary_encoder_press_channel = Some(channel);
                self.draft.synth_press_note = Some(note);
                self.draft.synth_press_channel = Some(channel);
                Ok(format!(
                    "note {note} ch {} = rotary {SECONDARY_CLICK_ROTARY} synth click",
                    channel + 1
                ))
            }
        }
    }

    fn learn_encoder_modifier_button(&self, message: &[u8]) -> Result<LearnInput, String> {
        let input = LearnInput::from_message(message)
            .ok_or_else(|| "Expected an unused CC or note press".to_owned())?;
        match input {
            LearnInput::Cc { cc, .. } if used_ccs(&self.draft).contains(&cc) => {
                Err(format!("Conflict · CC {cc} is already assigned · retry"))
            }
            LearnInput::Note { note, .. } if used_notes(&self.draft).contains(&note) => Err(
                format!("Conflict · note {note} is already assigned · retry"),
            ),
            _ => Ok(input),
        }
    }

    fn learn_pad(&mut self, index: usize, message: &[u8]) -> Result<String, String> {
        let input = LearnInput::from_message(message)
            .ok_or_else(|| "Conflict or release · press an unused pad/button".to_owned())?;
        self.learn_pad_input(index, input)
    }

    fn learn_pad_input(&mut self, index: usize, input: LearnInput) -> Result<String, String> {
        self.draft.layout = match index {
            0..=3 => ControllerLayout::Eight,
            4 => ControllerLayout::Five,
            _ => self.draft.layout,
        };
        let pad = self.pad_for_step(index);
        match input {
            LearnInput::Cc { cc, channel } => {
                if used_ccs(&self.draft).contains(&cc) {
                    return Err(format!("Conflict · CC {cc} is already assigned · retry"));
                }
                self.draft.cc_buttons.insert(cc, pad);
                self.draft.cc_button_channels.insert(cc, channel);
            }
            LearnInput::Note { note, channel } => {
                if used_notes(&self.draft).contains(&note) {
                    return Err(format!(
                        "Conflict · note {note} is already assigned · retry"
                    ));
                }
                self.draft.pads.insert(note, pad);
                self.draft.pad_channels.insert(note, channel);
            }
        }
        Ok(format!(
            "{} = PAD {}",
            learn_input_description(input),
            pad.number().unwrap()
        ))
    }

    pub fn validated_config(&self) -> Result<PadConfig> {
        if !self.can_finish() {
            bail!("learn rotary 1 turn/click and rotary 9 click before saving");
        }
        self.draft.validate()?;
        Ok(self.draft.clone())
    }

    pub fn can_finish(&self) -> bool {
        self.draft.encoder_relative_cc.is_some()
            && (self.draft.encoder_press_cc.is_some() || self.draft.encoder_press_note.is_some())
            && (self.draft.secondary_encoder_press_cc.is_some()
                || self.draft.secondary_encoder_press_note.is_some())
    }

    pub fn ready_to_save(&self) -> bool {
        self.role() == LearnRole::Confirm && self.can_finish()
    }

    fn role_is_mapped(&self) -> bool {
        match self.role() {
            LearnRole::EncoderModifier => {
                self.draft.encoder_modifier.is_some()
                    || self.draft.encoder_modified_relative_cc.is_some()
            }
            LearnRole::EncoderClick => {
                self.draft.encoder_press_cc.is_some() || self.draft.encoder_press_note.is_some()
            }
            LearnRole::SecondaryEncoderClick => {
                self.draft.secondary_encoder_press_cc.is_some()
                    || self.draft.secondary_encoder_press_note.is_some()
            }
            LearnRole::RotaryTurn(index) => self
                .draft
                .controls
                .values()
                .any(|position| *position == index as u8 + 1),
            LearnRole::Pad(index) => {
                self.draft
                    .pads
                    .values()
                    .chain(self.draft.cc_buttons.values())
                    .any(|pad| *pad == self.pad_for_step(index))
                    || (index == 4
                        && self.draft.page_cycle_modifier.is_some()
                        && self.draft.page_cycle_trigger.is_some())
            }
            _ => false,
        }
    }

    fn clear_current_mapping(&mut self) {
        self.rotary_proof = None;
        match self.role() {
            LearnRole::EncoderCounterClockwise => {
                self.draft.encoder_relative_cc = None;
                self.draft.encoder_relative_reverse = false;
                self.encoder_left_value = None;
            }
            LearnRole::EncoderClick => {
                self.draft.encoder_press_cc = None;
                self.draft.encoder_press_note = None;
                self.draft.encoder_press_channel = None;
            }
            LearnRole::SecondaryEncoderClick => {
                self.draft.secondary_encoder_press_cc = None;
                self.draft.secondary_encoder_press_note = None;
                self.draft.secondary_encoder_press_channel = None;
                self.draft.synth_press_cc = None;
                self.draft.synth_press_note = None;
                self.draft.synth_press_channel = None;
            }
            LearnRole::EncoderModifier => {
                self.draft.encoder_modifier = None;
                self.draft.encoder_modified_relative_cc = None;
                self.draft.encoder_modified_relative_reverse = false;
            }
            LearnRole::RotaryTurn(index) => {
                let position = index as u8 + 1;
                self.draft.controls.retain(|_, mapped| *mapped != position);
            }
            LearnRole::Pad(index) => {
                let pad = self.pad_for_step(index);
                if index == 4 {
                    self.draft.page_cycle_modifier = None;
                    self.draft.page_cycle_trigger = None;
                }
                let notes = self
                    .draft
                    .pads
                    .iter()
                    .filter_map(|(note, mapped)| (*mapped == pad).then_some(*note))
                    .collect::<Vec<_>>();
                for note in notes {
                    self.draft.pads.remove(&note);
                    self.draft.pad_channels.remove(&note);
                }
                let ccs = self
                    .draft
                    .cc_buttons
                    .iter()
                    .filter_map(|(cc, mapped)| (*mapped == pad).then_some(*cc))
                    .collect::<Vec<_>>();
                for cc in ccs {
                    self.draft.cc_buttons.remove(&cc);
                    self.draft.cc_button_channels.remove(&cc);
                }
            }
            LearnRole::EncoderClockwise | LearnRole::Confirm => {}
        }
    }

    fn capture_shift_rotary(&mut self, modifier: Option<LearnInput>, message: &[u8], now: Instant) {
        match self.learn_shift_rotary(modifier, message) {
            Ok(RotaryLearn::Pending) => {}
            Ok(RotaryLearn::LeftProven { cc }) => {
                self.state = LearnState::ShiftLeftSettling {
                    cc,
                    modifier,
                    deadline: now + GESTURE_SETTLE,
                };
                self.feedback = "Left verified · release Shift".into();
            }
            Ok(RotaryLearn::Complete(description)) => {
                let Some(cc) = cc_number(message) else {
                    return;
                };
                self.state = if let Some(modifier) = modifier {
                    LearnState::EncoderModifierChordHeld { modifier }
                } else {
                    LearnState::Settling {
                        cc,
                        deadline: now + GESTURE_SETTLE,
                    }
                };
                self.feedback = format!("Learned {description} · release Shift");
            }
            Err(message) => {
                self.rotary_proof = None;
                self.state = LearnState::Armed;
                self.feedback = message;
            }
        }
    }

    fn learn_shift_rotary(
        &mut self,
        modifier: Option<LearnInput>,
        message: &[u8],
    ) -> Result<RotaryLearn, String> {
        if message.len() < 3 || message[0] & 0xf0 != 0xb0 {
            return Err("Expected Shift plus a relative rotary CC".into());
        }
        let channel = message[0] & 0x0f;
        let cc = message[1];
        let value = message[2];
        let modifier_cc = modifier.and_then(|input| match input {
            LearnInput::Cc { cc, .. } => Some(cc),
            LearnInput::Note { .. } => None,
        });
        if Some(cc) == modifier_cc {
            return Ok(RotaryLearn::Pending);
        }
        let ordinary_cc = self.draft.encoder_relative_cc;
        if modifier.is_none() && Some(cc) == ordinary_cc {
            return Err(format!(
                "Expected the alternate Shift CC, not ordinary rotary CC {cc}"
            ));
        }
        if Some(cc) != ordinary_cc && used_ccs(&self.draft).contains(&cc) {
            return Err(format!(
                "Conflict · shifted rotary CC {cc} is already assigned"
            ));
        }
        if matches!(value, 0 | 64) {
            return Ok(RotaryLearn::Pending);
        }
        let learning_right = self.rotary_proof.is_some_and(|proof| proof.left_proven);
        let signature = match self.rotary_proof {
            Some(proof) => proof,
            None => match value {
                61..=63 => RotaryProof {
                    channel,
                    cc,
                    left_min: 61,
                    left_max: 63,
                    right_min: 65,
                    right_max: 67,
                    reverse: false,
                    left_proven: false,
                    repeats: 0,
                },
                65..=67 => RotaryProof {
                    channel,
                    cc,
                    left_min: 65,
                    left_max: 67,
                    right_min: 61,
                    right_max: 63,
                    reverse: true,
                    left_proven: false,
                    repeats: 0,
                },
                125..=127 => RotaryProof {
                    channel,
                    cc,
                    left_min: 125,
                    left_max: 127,
                    right_min: 1,
                    right_max: 3,
                    reverse: true,
                    left_proven: false,
                    repeats: 0,
                },
                _ => return Err("DIRECTION · turn left".into()),
            },
        };
        let expected = if learning_right {
            (signature.right_min..=signature.right_max).contains(&value)
        } else {
            (signature.left_min..=signature.left_max).contains(&value)
        };
        if !expected {
            return Err(format!(
                "DIRECTION · turn {}",
                if learning_right { "right" } else { "left" }
            ));
        }
        if Some(cc) == ordinary_cc && signature.reverse != self.draft.encoder_relative_reverse {
            return Err("Shifted ordinary CC has the opposite direction".into());
        }
        let repeats = match self.rotary_proof {
            Some(proof) if proof.channel != channel || proof.cc != cc => {
                return Err(format!("Expected the same shifted rotary CC {}", proof.cc));
            }
            Some(proof) => proof.repeats.saturating_add(1),
            None => 1,
        };
        self.rotary_proof = Some(RotaryProof {
            repeats,
            ..signature
        });
        if repeats < 3 {
            self.feedback = format!(
                "{} direction proof {repeats}/3",
                if learning_right { "Right" } else { "Left" }
            );
            return Ok(RotaryLearn::Pending);
        }
        if !learning_right {
            if let Some(proof) = self.rotary_proof.as_mut() {
                proof.left_proven = true;
                proof.repeats = 0;
            }
            return Ok(RotaryLearn::LeftProven { cc });
        }
        let reverse = self
            .rotary_proof
            .map(|proof| proof.reverse)
            .unwrap_or(false);
        self.draft.encoder_modifier = modifier.map(LearnInput::controller_button);
        if Some(cc) == ordinary_cc {
            self.draft.encoder_modified_relative_cc = None;
            self.draft.encoder_modified_relative_reverse = false;
        } else {
            self.draft.encoder_modified_relative_cc = Some(cc);
            self.draft.encoder_modified_relative_reverse = reverse;
        }
        self.rotary_proof = None;
        Ok(RotaryLearn::Complete(format!("Shift rotary CC {cc}")))
    }
}

fn cc_number(message: &[u8]) -> Option<u8> {
    (message.len() >= 3 && message[0] & 0xf0 == 0xb0).then_some(message[1])
}

fn cc_message(message: &[u8], cc: u8) -> bool {
    cc_number(message) == Some(cc)
}

fn moving_cc(message: &[u8]) -> Option<(u8, u8)> {
    if message.len() < 3 || message[0] & 0xf0 != 0xb0 || matches!(message[2], 0 | 64) {
        return None;
    }
    Some((message[1], message[2]))
}

fn message_marks_activity(message: &[u8]) -> bool {
    message.len() >= 3 && matches!(message[0] & 0xf0, 0x80 | 0x90 | 0xb0)
}

fn learn_input_description(input: LearnInput) -> String {
    match input {
        LearnInput::Cc { channel, cc } => format!("CC {cc} ch {}", channel + 1),
        LearnInput::Note { channel, note } => format!("note {note} ch {}", channel + 1),
    }
}

fn message_is_relevant(role: LearnRole, message: &[u8]) -> bool {
    if message.len() < 3 {
        return false;
    }
    match role {
        LearnRole::RotaryTurn(_) => message[0] & 0xf0 == 0xb0,
        LearnRole::EncoderClockwise | LearnRole::EncoderCounterClockwise => {
            moving_cc(message).is_some()
        }
        LearnRole::EncoderClick
        | LearnRole::SecondaryEncoderClick
        | LearnRole::EncoderModifier
        | LearnRole::Pad(_) => message[2] > 0 && matches!(message[0] & 0xf0, 0x90 | 0xb0),
        LearnRole::Confirm => false,
    }
}

pub fn stable_input_match(name: &str) -> String {
    crate::midi_endpoint::stable_identity(name)
}

pub fn learn(config: &mut PadConfig, input_name: &str) -> Result<()> {
    let (connection, receiver) = listen(input_name)?;
    let _connection = connection;
    config.input_match = Some(stable_input_match(input_name));
    println!("Listening to {input_name}. MIDI is not being forwarded to an instrument.");

    let missing = (1..=usize::from(MAPPED_ROTARY_COUNT))
        .filter(|position| {
            !config
                .controls
                .values()
                .any(|mapped| usize::from(*mapped) == *position)
        })
        .count();
    if missing > 0 {
        let count = ask_number(
            &format!("Additional rotary turns to learn (0-{missing}) [0]: "),
            0,
            missing,
        )?;
        let positions = (1..=usize::from(MAPPED_ROTARY_COUNT))
            .filter(|position| {
                !config
                    .controls
                    .values()
                    .any(|mapped| usize::from(*mapped) == *position)
            })
            .take(count)
            .collect::<Vec<_>>();
        for position in positions {
            let (cc, value) = capture_cc_value(
                &receiver,
                &format!("Turn ROTARY {}", position + 1),
                &used_ccs(config),
            )?;
            let relative_value = if config.encoder_relative_reverse {
                matches!(value, 1..=3 | 125..=127)
            } else {
                matches!(value, 61..=63 | 65..=67)
            };
            if !relative_value {
                bail!("ROTARY {} is not sending relative steps", position + 1);
            }
            config.controls.insert(cc, position as u8);
            println!("  ROTARY {} -> CC {cc}", position + 1);
        }
    }

    if config.encoder_relative_cc.is_none()
        && ask_yes_no("Learn a relative ROTARY 1 navigation turn? [y/N]: ")?
    {
        let (cc, value) = capture_cc_value(
            &receiver,
            "Turn the main encoder clockwise",
            &used_ccs(config),
        )?;
        let reverse = match value {
            65..=67 => false,
            1..=3 => true,
            _ => bail!("encoder is not sending Relative 1 or Relative 2 steps"),
        };
        config.encoder_relative_cc = Some(cc);
        config.encoder_relative_reverse = reverse;
        println!("  encoder CC {cc}; direction convention detected");
    }

    if config.encoder_press_cc.is_none()
        && config.encoder_press_note.is_none()
        && ask_yes_no("Learn the main encoder press/select? [y/N]: ")?
    {
        match capture_button(
            &receiver,
            "Press ROTARY 1",
            &used_ccs(config),
            &used_notes(config),
        )? {
            Button::Cc { cc, channel } => {
                config.encoder_press_cc = Some(cc);
                config.encoder_press_channel = Some(channel);
            }
            Button::Note { note, channel } => {
                config.encoder_press_note = Some(note);
                config.encoder_press_channel = Some(channel);
            }
        }
    }

    if config.secondary_encoder_press_cc.is_none()
        && config.secondary_encoder_press_note.is_none()
        && ask_yes_no("Learn the ROTARY 9 press? [y/N]: ")?
    {
        match capture_button(
            &receiver,
            "Press ROTARY 9",
            &used_ccs(config),
            &used_notes(config),
        )? {
            Button::Cc { cc, channel } => {
                config.secondary_encoder_press_cc = Some(cc);
                config.secondary_encoder_press_channel = Some(channel);
                config.synth_press_cc = Some(cc);
                config.synth_press_channel = Some(channel);
            }
            Button::Note { note, channel } => {
                config.secondary_encoder_press_note = Some(note);
                config.secondary_encoder_press_channel = Some(channel);
                config.synth_press_note = Some(note);
                config.synth_press_channel = Some(channel);
            }
        }
    }

    if config.encoder_modifier.is_none()
        && ask_yes_no("Learn an optional Shift button for the main encoder? [y/N]: ")?
    {
        let input = capture_button(
            &receiver,
            "Hold the encoder Shift button",
            &used_ccs(config),
            &used_notes(config),
        )?;
        config.encoder_modifier = Some(match input {
            Button::Cc { cc, channel } => ControllerButton::Cc { channel, cc },
            Button::Note { note, channel } => ControllerButton::Note { channel, note },
        });
        let mut shifted_turn_conflicts = used_ccs(config);
        if let Some(ordinary_cc) = config.encoder_relative_cc {
            shifted_turn_conflicts.remove(&ordinary_cc);
        }
        let (cc, value) = capture_cc_value(
            &receiver,
            "Keep holding Shift and turn the main encoder counterclockwise",
            &shifted_turn_conflicts,
        )?;
        let reverse = match value {
            61..=63 => false,
            125..=127 => true,
            _ => bail!("shifted encoder is not sending Relative 1 or Relative 2 steps"),
        };
        if Some(cc) == config.encoder_relative_cc {
            config.encoder_modified_relative_cc = None;
            config.encoder_modified_relative_reverse = false;
            println!("  shifted encoder keeps ordinary CC {cc}");
        } else {
            config.encoder_modified_relative_cc = Some(cc);
            config.encoder_modified_relative_reverse = reverse;
            println!("  shifted encoder CC {cc}; direction convention detected");
        }
        println!("  release encoder Shift");
    }

    let layout = ask_number("Physical pads available (0, 4, 5, or 8) [0]: ", 0, 8)?;
    if !matches!(layout, 0 | 4 | 5 | 8) {
        bail!("physical PAD count must be 0, 4, 5, or 8");
    }
    if layout == 0 {
        config.layout = ControllerLayout::Four;
        config.pads.clear();
        config.pad_channels.clear();
        config.cc_buttons.clear();
        config.cc_button_channels.clear();
        config.page_cycle_modifier = None;
        config.page_cycle_trigger = None;
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
        config.page_cycle_modifier = None;
        config.page_cycle_trigger = None;
        for position in 1..=layout {
            let pad = PadAction::physical(position as u8).expect("validated physical PAD count");
            let binding = capture_button(
                &receiver,
                &format!("Press PAD {position}"),
                &used_ccs(config),
                &used_notes(config),
            )?;
            match binding {
                Button::Cc { cc, channel } => {
                    config.cc_buttons.insert(cc, pad);
                    config.cc_button_channels.insert(cc, channel);
                }
                Button::Note { note, channel } => {
                    config.pads.insert(note, pad);
                    config.pad_channels.insert(note, channel);
                }
            }
        }
    }

    if config.lock_cc.is_none() && ask_yes_no("Learn an optional PAD lock CC? [y/N]: ")? {
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

fn restore_file(path: &Path, contents: Option<&[u8]>) {
    match contents {
        Some(contents) => {
            let _ = crate::fsutil::atomic_write(path, contents);
        }
        None if path.is_file() => {
            let _ = fs::remove_file(path);
        }
        None => {}
    }
}

/// Saves the active controller and, for reviewed hardware, the model-owned
/// learned mapping as one recoverable operation. Automatic model switching
/// writes only `controller.conf`; only an explicit Learn save updates the
/// model-owned copy.
pub fn save_learned_for_state(state: &Path, config: &PadConfig) -> Result<Option<PathBuf>> {
    let active = state.join("controller.conf");
    let model = config
        .profile
        .as_deref()
        .filter(|profile| *profile != "learned")
        .map(|profile| crate::controller_profile::private_mapping_path(state, profile))
        .transpose()?;
    let old_active = fs::read(&active).ok();
    let old_model = model.as_ref().and_then(|path| fs::read(path).ok());

    let result = (|| {
        backup(&active)?;
        if let Some(path) = &model {
            backup(path)?;
            config.save(path)?;
        }
        config.save(&active)
    })();
    if let Err(error) = result {
        restore_file(&active, old_active.as_deref());
        if let Some(path) = &model {
            restore_file(path, old_model.as_deref());
        }
        return Err(error);
    }
    Ok(model)
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
                config.encoder_modified_relative_cc,
                config.encoder_press_cc,
                config.synth_press_cc,
                config.secondary_encoder_press_cc,
                config.lock_cc,
            ]
            .into_iter()
            .flatten(),
        )
        .chain(
            [
                config.encoder_modifier,
                config.page_cycle_modifier,
                config.page_cycle_trigger,
            ]
            .into_iter()
            .flatten()
            .filter_map(|button| match button {
                ControllerButton::Cc { cc, .. } => Some(cc),
                ControllerButton::Note { .. } => None,
            }),
        )
        .collect()
}

fn used_notes(config: &PadConfig) -> HashSet<u8> {
    config
        .pads
        .keys()
        .copied()
        .chain(config.encoder_press_note)
        .chain(config.synth_press_note)
        .chain(config.secondary_encoder_press_note)
        .chain(
            [
                config.encoder_modifier,
                config.page_cycle_modifier,
                config.page_cycle_trigger,
            ]
            .into_iter()
            .flatten()
            .filter_map(|button| match button {
                ControllerButton::Note { note, .. } => Some(note),
                ControllerButton::Cc { .. } => None,
            }),
        )
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
    fn learn_labels_expose_physical_rotary_and_pad_identity() {
        assert_eq!(LearnRole::RotaryTurn(0).label(), "TURN ROTARY 2");
        assert_eq!(LearnRole::RotaryTurn(14).label(), "TURN ROTARY 16");
        assert_eq!(LearnRole::Pad(0).label(), "PRESS PAD 1");
        assert_eq!(
            LearnRole::EncoderModifier.label(),
            "SHIFT + TURN ROTARY 1 LEFT"
        );
        for forbidden in ["Flt", "Volume", "Dly", "STOP", "PLAY", "REC", "TAP"] {
            assert!(!LearnRole::RotaryTurn(0).label().contains(forbidden));
            assert!(!LearnRole::Pad(0).label().contains(forbidden));
        }
    }

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
    fn explicit_learn_save_keeps_active_and_model_owned_copies() {
        let base = std::env::temp_dir().join(format!(
            "shsynth-controller-model-save-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let old = PadConfig {
            input_match: Some("MiniLab3 MIDI".into()),
            profile: Some("arturia-minilab-3".into()),
            encoder_relative_cc: Some(114),
            encoder_press_cc: Some(115),
            ..PadConfig::default()
        };
        old.save(&base.join("controller.conf")).unwrap();
        let learned = PadConfig {
            input_match: Some("Arturia MiniLab mkII MIDI".into()),
            profile: Some("arturia-minilab-mkii".into()),
            encoder_relative_cc: Some(28),
            encoder_press_cc: Some(118),
            secondary_encoder_press_cc: Some(117),
            ..PadConfig::default()
        };

        let model = save_learned_for_state(&base, &learned).unwrap().unwrap();

        assert_eq!(
            PadConfig::load(&base.join("controller.conf")).unwrap(),
            learned
        );
        assert_eq!(PadConfig::load(&model).unwrap(), learned);
        let backups = std::fs::read_dir(&base)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
            .count();
        assert_eq!(backups, 1);
        let _ = std::fs::remove_dir_all(base);
    }

    struct Harness {
        learn: LearnSession,
        now: Instant,
    }

    impl Harness {
        fn new() -> Self {
            let start = Instant::now();
            let mut learn = LearnSession::new_at("Test Controller MIDI 44:0", start);
            let now = start + ENTRY_QUIET;
            learn.tick(now);
            Self { learn, now }
        }

        fn send(&mut self, message: &[u8]) -> LearnAction {
            self.now += Duration::from_millis(1);
            self.learn.receive(message, self.now)
        }

        fn settle(&mut self) {
            self.now += GESTURE_SETTLE + Duration::from_millis(1);
            self.learn.tick(self.now);
        }

        fn learn_master(&mut self, rotary: u8, click: u8, high_low: bool) {
            let (left, right, neutral) = if high_low { (125, 1, 0) } else { (63, 65, 64) };
            self.send(&[0xb0, rotary, left]);
            self.send(&[0xb0, rotary, neutral]);
            self.settle();
            assert_eq!(self.learn.role(), LearnRole::EncoderClockwise);
            self.send(&[0xb0, rotary, right]);
            self.send(&[0xb0, rotary, neutral]);
            self.settle();
            assert_eq!(self.learn.role(), LearnRole::EncoderClick);
            self.send(&[0xb0, click, 127]);
            assert!(self.learn.feedback().contains("OK"));
            self.send(&[0xb0, click, 0]);
            assert_eq!(self.learn.role(), LearnRole::EncoderModifier);
            assert!(self.learn.skip());
            assert_eq!(self.learn.role(), LearnRole::RotaryTurn(0));
        }

        fn learn_rotary(&mut self, cc: u8) {
            let (left, right) = if self.learn.draft().encoder_relative_reverse {
                (127, 1)
            } else {
                (63, 65)
            };
            for _ in 0..3 {
                self.send(&[0xb0, cc, left]);
            }
            assert!(self.learn.draft().controls.get(&cc).is_none());
            self.settle();
            for _ in 0..3 {
                self.send(&[0xb0, cc, right]);
            }
            assert!(self.learn.draft().controls.get(&cc).is_some());
            self.settle();
        }

        fn skip_controls(&mut self) {
            while self.learn.role() != LearnRole::Pad(0) {
                match self.learn.role() {
                    LearnRole::RotaryTurn(_) => assert!(self.learn.skip()),
                    LearnRole::SecondaryEncoderClick => {
                        self.send(&[0xb0, 117, 127]);
                        self.send(&[0xb0, 117, 0]);
                    }
                    role => panic!("unexpected control role: {role:?}"),
                }
            }
            assert_eq!(self.learn.role(), LearnRole::Pad(0));
        }

        fn skip_to_confirm(&mut self) {
            while self.learn.role() != LearnRole::Confirm {
                if self.learn.role() == LearnRole::SecondaryEncoderClick {
                    self.send(&[0xb0, 117, 127]);
                    self.send(&[0xb0, 117, 0]);
                } else {
                    assert!(self.learn.skip());
                }
            }
        }
    }

    #[test]
    fn opening_click_release_is_quarantined_before_rotary_left_arms() {
        let start = Instant::now();
        let mut learn = LearnSession::new_at("Controller", start);
        learn.receive(&[0xb0, 118, 0], start + Duration::from_millis(20));
        learn.tick(start + ENTRY_QUIET);
        assert_eq!(learn.role(), LearnRole::EncoderCounterClockwise);
        assert_eq!(learn.draft().encoder_relative_cc, None);
        learn.tick(start + Duration::from_millis(20) + ENTRY_QUIET);
        learn.receive(&[0xb0, 28, 63], start + Duration::from_millis(141));
        assert_eq!(learn.draft().encoder_relative_cc, Some(28));
    }

    #[test]
    fn delayed_opening_click_release_is_filtered_after_rotary_left_arms() {
        let start = Instant::now();
        let mut learn = LearnSession::new_at("Controller", start);
        learn.tick(start + ENTRY_QUIET);

        learn.receive(
            &[0xb0, 118, 0],
            start + ENTRY_QUIET + Duration::from_millis(1),
        );

        assert_eq!(learn.role(), LearnRole::EncoderCounterClockwise);
        assert_eq!(learn.draft().encoder_relative_cc, None);
        learn.receive(
            &[0xb0, 28, 63],
            start + ENTRY_QUIET + Duration::from_millis(2),
        );
        assert_eq!(learn.draft().encoder_relative_cc, Some(28));
    }

    #[test]
    fn last_learn_trace_replaces_old_content_and_records_steps_and_all_inputs() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-midi-learn-last-{}.log",
            std::process::id()
        ));
        fs::write(&path, "stale previous session\n").unwrap();
        let start = Instant::now();
        let mut learn = LearnSession::new_for_profile_at_with_trace(
            "Controller",
            Some("test-profile"),
            start,
            Some(&path),
        );
        learn.tick(start + ENTRY_QUIET);
        learn.receive(
            &[0xb0, 118, 0],
            start + ENTRY_QUIET + Duration::from_millis(1),
        );
        learn.receive(
            &[0xb0, 28, 63],
            start + ENTRY_QUIET + Duration::from_millis(2),
        );

        let trace = fs::read_to_string(&path).unwrap();
        assert!(!trace.contains("stale previous session"));
        assert!(trace.contains("START input=\"Controller\" profile=\"test-profile\""));
        assert!(trace.contains("ASK step=\"TURN ROTARY 1 LEFT\""));
        assert!(trace.contains("INPUT step=\"TURN ROTARY 1 LEFT\" state=armed bytes=B0 76 00"));
        assert!(trace.contains("INPUT step=\"TURN ROTARY 1 LEFT\" state=armed bytes=B0 1C 3F"));
        assert_eq!(
            trace
                .lines()
                .filter(|line| line.contains(" INPUT "))
                .count(),
            2
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn left_neutral_cannot_satisfy_right_and_right_waits_for_settle() {
        let mut h = Harness::new();
        h.send(&[0xb0, 28, 63]);
        let success = h.learn.feedback().to_owned();
        h.send(&[0xb0, 28, 64]);
        h.send(&[0xb0, 28, 65]);
        assert_eq!(h.learn.role(), LearnRole::EncoderCounterClockwise);
        assert_eq!(h.learn.feedback(), success);
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::EncoderClockwise);
        h.send(&[0xb0, 28, 65]);
        assert!(h.learn.feedback().contains("right"));
    }

    #[test]
    fn relative_one_without_neutral_packets_is_accepted() {
        let mut h = Harness::new();
        h.send(&[0xb0, 112, 63]);
        h.settle();
        h.send(&[0xb0, 112, 65]);

        assert!(h.learn.feedback().contains("relative 1 navigation"));
        assert!(!h.learn.draft().encoder_relative_reverse);
    }

    #[test]
    fn relative_two_without_neutral_packets_is_accepted() {
        let mut h = Harness::new();
        h.send(&[0xb0, 112, 127]);
        h.settle();
        h.send(&[0xb0, 112, 1]);

        assert!(h.learn.feedback().contains("relative 2 navigation"));
        assert!(h.learn.draft().encoder_relative_reverse);
    }

    #[test]
    fn positional_rotary_one_is_rejected() {
        let mut h = Harness::new();
        h.send(&[0xb0, 28, 80]);
        h.send(&[0xb0, 28, 79]);
        h.settle();
        h.send(&[0xb0, 28, 80]);
        assert!(h.learn.feedback().contains("not sending relative steps"));
        assert_eq!(h.learn.role(), LearnRole::EncoderClockwise);
    }

    #[test]
    fn both_click_press_release_sequences_advance_once() {
        let mut h = Harness::new();
        h.send(&[0xb0, 28, 63]);
        h.settle();
        h.send(&[0xb0, 28, 65]);
        h.settle();
        h.send(&[0xb0, 118, 127]);
        assert_eq!(h.learn.role(), LearnRole::EncoderClick);
        h.send(&[0xb0, 118, 127]);
        assert_eq!(h.learn.role(), LearnRole::EncoderClick);
        h.send(&[0xb0, 118, 0]);
        assert_eq!(h.learn.role(), LearnRole::EncoderModifier);
        assert!(h.learn.skip());
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        assert_eq!(h.learn.draft().encoder_press_cc, Some(118));
    }

    #[test]
    fn learn_scans_rotaries_one_through_sixteen_with_special_actions_in_place() {
        let mut h = Harness::new();
        h.send(&[0xb0, 28, 63]);
        h.settle();
        h.send(&[0xb0, 28, 65]);
        h.settle();
        h.send(&[0xb0, 118, 127]);
        h.send(&[0xb0, 118, 0]);
        assert!(h.learn.skip());

        for expected in 0..=7 {
            assert_eq!(h.learn.role(), LearnRole::RotaryTurn(expected));
            assert!(h.learn.skip());
        }
        assert_eq!(h.learn.role(), LearnRole::SecondaryEncoderClick);
        h.send(&[0xb0, 117, 127]);
        h.send(&[0xb0, 117, 0]);
        for expected in 8..=14 {
            assert_eq!(h.learn.role(), LearnRole::RotaryTurn(expected));
            assert!(h.learn.skip());
        }
        assert_eq!(h.learn.role(), LearnRole::Pad(0));
    }

    #[test]
    fn revisited_rotary_nine_click_can_be_recorded_again() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));

        for _ in 0..8 {
            h.send(&[0xb0, 28, 65]);
            h.settle();
        }
        assert_eq!(h.learn.role(), LearnRole::SecondaryEncoderClick);
        h.send(&[0xb0, 117, 127]);
        h.send(&[0xb0, 117, 0]);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(8));
        h.send(&[0xb0, 28, 63]);
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::SecondaryEncoderClick);
        h.send(&[0xb0, 117, 127]);

        assert!(h.learn.feedback().contains("rotary 9 synth click"));
        assert_eq!(h.learn.draft().secondary_encoder_press_cc, Some(117));
        assert_eq!(h.learn.draft().synth_press_cc, Some(117));
        h.send(&[0xb0, 117, 0]);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(8));
    }

    #[test]
    fn optional_encoder_shift_learns_shift_plus_left_before_controls() {
        let mut h = Harness::new();
        h.send(&[0xb0, 28, 63]);
        h.send(&[0xb0, 28, 64]);
        h.settle();
        h.send(&[0xb0, 28, 65]);
        h.send(&[0xb0, 28, 64]);
        h.settle();
        h.send(&[0xb0, 118, 127]);
        h.send(&[0xb0, 118, 0]);
        assert_eq!(h.learn.role(), LearnRole::EncoderModifier);

        h.send(&[0xb0, 27, 127]);
        assert!(h.learn.feedback().contains("turn rotary 1 left"));
        for value in [63, 62, 61] {
            h.send(&[0xb0, 29, value]);
        }
        assert_eq!(h.learn.draft().encoder_modifier, None);
        assert_eq!(h.learn.role_label(), "RELEASE SHIFT");
        h.send(&[0xb0, 27, 0]);
        assert!(h.learn.role_label().contains("RIGHT"));
        assert_eq!(h.learn.role_label(), "SHIFT + TURN ROTARY 1 RIGHT");
        h.send(&[0xb0, 27, 127]);
        for value in [65, 66, 67] {
            h.send(&[0xb0, 29, value]);
        }
        assert!(h.learn.feedback().contains("release Shift"));
        h.send(&[0xb0, 27, 0]);

        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        for _ in 0..8 {
            assert!(h.learn.skip());
        }
        assert_eq!(h.learn.role(), LearnRole::SecondaryEncoderClick);
        assert_eq!(
            h.learn.draft().encoder_modifier,
            Some(ControllerButton::Cc { channel: 0, cc: 27 })
        );
        assert_eq!(h.learn.draft().encoder_modified_relative_cc, Some(29));
        assert!(!h.learn.draft().encoder_modified_relative_reverse);
        h.send(&[0xb0, 27, 127]);
        assert_eq!(h.learn.draft().secondary_encoder_press_cc, None);
        assert!(h.learn.feedback().contains("Expected an unused"));
        h.send(&[0xb0, 117, 127]);
        h.send(&[0xb0, 117, 0]);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(8));
    }

    #[test]
    fn optional_encoder_shift_accepts_the_ordinary_relative_cc() {
        let mut h = Harness::new();
        h.send(&[0xb0, 28, 63]);
        h.send(&[0xb0, 28, 64]);
        h.settle();
        h.send(&[0xb0, 28, 65]);
        h.send(&[0xb0, 28, 64]);
        h.settle();
        h.send(&[0xb0, 118, 127]);
        h.send(&[0xb0, 118, 0]);

        h.send(&[0xb0, 27, 127]);
        for _ in 0..3 {
            h.send(&[0xb0, 28, 63]);
        }
        h.send(&[0xb0, 27, 0]);
        h.send(&[0xb0, 27, 127]);
        for _ in 0..3 {
            h.send(&[0xb0, 28, 65]);
        }
        h.send(&[0xb0, 27, 0]);

        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        assert_eq!(
            h.learn.draft().encoder_modifier,
            Some(ControllerButton::Cc { channel: 0, cc: 27 })
        );
        assert_eq!(h.learn.draft().encoder_modified_relative_cc, None);
    }

    #[test]
    fn minilab_mkii_shift_layer_is_direct_relative_without_a_shift_packet() {
        let start = Instant::now();
        let mut h = Harness {
            learn: LearnSession::new_for_profile_at(
                "Arturia MiniLab mkII MIDI 1",
                Some("arturia-minilab-mkii"),
                start,
            ),
            now: start + ENTRY_QUIET,
        };
        h.learn.tick(h.now);
        h.send(&[0xb0, 112, 63]);
        h.settle();
        h.send(&[0xb0, 112, 65]);
        h.settle();
        h.send(&[0xb0, 113, 127]);
        h.send(&[0xb0, 113, 0]);
        assert_eq!(h.learn.role(), LearnRole::EncoderModifier);

        for _ in 0..3 {
            h.send(&[0xb0, 7, 63]);
        }
        assert_eq!(h.learn.draft().encoder_modified_relative_cc, None);
        h.settle();
        assert!(h.learn.role_label().contains("RIGHT"));
        for _ in 0..3 {
            h.send(&[0xb0, 7, 65]);
        }

        assert!(h.learn.feedback().contains("Shift rotary CC 7"));
        assert_eq!(h.learn.draft().encoder_modifier, None);
        assert_eq!(h.learn.draft().encoder_modified_relative_cc, Some(7));
        assert!(!h.learn.draft().encoder_modified_relative_reverse);
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
    }

    #[test]
    fn direct_shift_layer_proves_relative_two_left_and_right() {
        let start = Instant::now();
        let mut h = Harness {
            learn: LearnSession::new_for_profile_at(
                "Arturia MiniLab mkII MIDI 1",
                Some("arturia-minilab-mkii"),
                start,
            ),
            now: start + ENTRY_QUIET,
        };
        h.learn.tick(h.now);
        h.send(&[0xb0, 114, 127]);
        h.settle();
        h.send(&[0xb0, 114, 1]);
        h.settle();
        h.send(&[0xb0, 115, 127]);
        h.send(&[0xb0, 115, 0]);
        assert_eq!(h.learn.role(), LearnRole::EncoderModifier);

        for _ in 0..3 {
            h.send(&[0xb0, 7, 127]);
        }
        assert_eq!(h.learn.draft().encoder_modified_relative_cc, None);
        assert_eq!(h.learn.role_label(), "RELEASE SHIFT");
        h.settle();
        assert!(h.learn.role_label().contains("RIGHT"));
        for _ in 0..3 {
            h.send(&[0xb0, 7, 1]);
        }
        assert_eq!(h.learn.draft().encoder_modified_relative_cc, Some(7));
        assert!(h.learn.draft().encoder_modified_relative_reverse);
    }

    #[test]
    fn direct_shift_layer_may_reverse_the_ordinary_rotary_direction_encoding() {
        let start = Instant::now();
        let mut h = Harness {
            learn: LearnSession::new_for_profile_at(
                "Arturia MiniLab mkII MIDI 1",
                Some("arturia-minilab-mkii"),
                start,
            ),
            now: start + ENTRY_QUIET,
        };
        h.learn.tick(h.now);
        h.send(&[0xb0, 112, 63]);
        h.settle();
        h.send(&[0xb0, 112, 65]);
        h.settle();
        h.send(&[0xb0, 113, 127]);
        h.send(&[0xb0, 113, 0]);

        for _ in 0..3 {
            h.send(&[0xb0, 7, 65]);
        }
        assert_eq!(h.learn.draft().encoder_modified_relative_cc, None);
        h.settle();
        assert!(h.learn.role_label().contains("RIGHT"));
        for _ in 0..3 {
            h.send(&[0xb0, 7, 63]);
        }
        assert_eq!(h.learn.draft().encoder_modified_relative_cc, Some(7));
        assert!(h.learn.draft().encoder_modified_relative_reverse);
    }

    #[test]
    fn minilab_mkii_shift_press_is_replaced_by_the_shifted_turn() {
        let start = Instant::now();
        let mut h = Harness {
            learn: LearnSession::new_for_profile_at(
                "Arturia MiniLab mkII MIDI 1",
                Some("arturia-minilab-mkii"),
                start,
            ),
            now: start + ENTRY_QUIET,
        };
        h.learn.tick(h.now);
        h.send(&[0xb0, 112, 63]);
        h.settle();
        h.send(&[0xb0, 112, 65]);
        h.settle();
        h.send(&[0xb0, 113, 127]);
        h.send(&[0xb0, 113, 0]);
        assert_eq!(h.learn.role(), LearnRole::EncoderModifier);

        h.send(&[0xb0, 27, 127]);
        h.send(&[0xb0, 27, 0]);

        assert_eq!(h.learn.role(), LearnRole::EncoderModifier);
        assert_eq!(h.learn.draft().encoder_modified_relative_cc, None);

        h.send(&[0xb0, 27, 127]);
        for _ in 0..3 {
            h.send(&[0xb0, 7, 63]);
        }
        h.settle();
        for _ in 0..3 {
            h.send(&[0xb0, 7, 65]);
        }

        assert_eq!(h.learn.draft().encoder_modified_relative_cc, Some(7));
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
    }

    #[test]
    fn relative_stream_stays_on_control_one_then_advances_once() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        for _ in 0..3 {
            h.send(&[0xb0, 10, 63]);
        }
        h.settle();
        for _ in 0..3 {
            h.send(&[0xb0, 10, 65]);
        }
        let success = h.learn.feedback().to_owned();
        for value in [66, 67, 65] {
            h.send(&[0xb0, 10, value]);
            assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
            assert_eq!(h.learn.feedback(), success);
        }
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
    }

    #[test]
    fn performance_rotary_requires_proven_left_and_right_streams() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);

        h.send(&[0xb0, 10, 63]);
        assert!(!h
            .learn
            .draft()
            .controls
            .values()
            .any(|position| *position == 1));
        assert!(h.learn.prompt_line().contains("left"));

        h.send(&[0xb0, 10, 62]);
        assert!(!h
            .learn
            .draft()
            .controls
            .values()
            .any(|position| *position == 1));
        h.send(&[0xb0, 10, 61]);
        assert!(!h
            .learn
            .draft()
            .controls
            .values()
            .any(|position| *position == 1));
        h.settle();
        assert!(h.learn.role_label().contains("RIGHT"));
        h.send(&[0xb0, 10, 65]);
        h.send(&[0xb0, 10, 66]);
        assert!(!h
            .learn
            .draft()
            .controls
            .values()
            .any(|position| *position == 1));
        h.send(&[0xb0, 10, 67]);
        assert_eq!(h.learn.draft().controls.get(&10), Some(&1));
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
    }

    #[test]
    fn rotary_feedback_waits_for_quiet_and_cannot_flash_into_the_next_direction() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        for value in [63, 62, 61] {
            h.send(&[0xb0, 10, value]);
        }
        assert_eq!(h.learn.role_label(), "TURN ROTARY 2 LEFT");

        h.now += GESTURE_SETTLE - Duration::from_millis(1);
        h.learn.tick(h.now);
        h.send(&[0xb0, 10, 63]);
        assert_eq!(h.learn.role_label(), "TURN ROTARY 2 LEFT");
        h.settle();
        assert_eq!(h.learn.role_label(), "TURN ROTARY 2 RIGHT");

        for value in [65, 66, 67] {
            h.send(&[0xb0, 10, value]);
        }
        let success = h.learn.feedback().to_owned();
        h.now += GESTURE_SETTLE - Duration::from_millis(1);
        h.learn.tick(h.now);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        assert_eq!(h.learn.feedback(), success);
        h.now += Duration::from_millis(2);
        h.learn.tick(h.now);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
    }

    #[test]
    fn positional_rotary_crossing_the_relative_range_is_never_learned() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);

        for value in [61, 62, 63, 64, 65, 66, 67, 68] {
            h.send(&[0xb0, 10, value]);
        }
        h.settle();
        h.send(&[0xb0, 10, 63]);

        assert!(!h
            .learn
            .draft()
            .controls
            .values()
            .any(|position| *position == 1));
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        assert!(h.learn.feedback().starts_with("DIRECTION"));
        for value in [64, 65, 66, 67] {
            h.send(&[0xb0, 10, value]);
        }
        assert!(!h
            .learn
            .draft()
            .controls
            .values()
            .any(|position| *position == 1));
    }

    #[test]
    fn opposite_direction_rejects_but_different_cc_is_ignored() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        for value in [63, 62, 61] {
            h.send(&[0xb0, 10, value]);
        }
        h.settle();

        h.send(&[0xb0, 10, 63]);
        assert!(h.learn.feedback().starts_with("DIRECTION"));
        assert!(h.learn.feedback_is_error());
        assert!(h.learn.prompt_line().contains("turn right"));

        h.settle();
        for value in [63, 62, 61] {
            h.send(&[0xb0, 10, value]);
        }
        h.settle();
        h.send(&[0xb0, 11, 65]);
        assert!(!h.learn.feedback_is_error());
        assert!(!h
            .learn
            .draft()
            .controls
            .values()
            .any(|position| *position == 1));
        for value in [65, 66, 67] {
            h.send(&[0xb0, 10, value]);
        }
        assert_eq!(h.learn.draft().controls.get(&10), Some(&1));
    }

    #[test]
    fn revisited_mapped_step_accepts_a_replacement_without_a_keyboard() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        h.learn_rotary(10);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));

        h.send(&[0xb0, 28, 63]);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        h.settle();
        h.learn_rotary(11);

        assert!(!h.learn.draft().controls.contains_key(&10));
        assert_eq!(h.learn.draft().controls.get(&11), Some(&1));
    }

    #[test]
    fn rejected_rotary_rearms_itself_and_prompts_for_no_keys() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        h.send(&[0xb0, 10, 63]);
        h.send(&[0xb0, 10, 62]);
        h.send(&[0xb0, 10, 67]);

        assert!(h.learn.feedback_is_error());
        for key_name in [" R ", " S ", "Esc", "Enter"] {
            assert!(!h.learn.prompt_line().contains(key_name));
        }
        h.settle();
        h.learn_rotary(10);
        assert_eq!(h.learn.draft().controls.get(&10), Some(&1));
    }

    #[test]
    fn unrelated_absolute_stream_does_not_poison_partial_rotary_proof() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);

        h.send(&[0xb0, 10, 63]);
        h.send(&[0xb0, 10, 62]);
        let stable_feedback = h.learn.feedback().to_owned();
        for value in 1..=86 {
            h.send(&[0xb0, 1, value]);
        }
        assert!(!h.learn.feedback_is_error());
        assert_eq!(h.learn.feedback(), stable_feedback);

        h.send(&[0xb0, 10, 61]);
        h.settle();
        for value in [65, 66, 67] {
            h.send(&[0xb0, 10, value]);
        }
        assert_eq!(h.learn.draft().controls.get(&10), Some(&1));
    }

    #[test]
    fn high_low_relative_rotary_proves_both_directions() {
        let mut h = Harness::new();
        h.learn_master(114, 115, true);
        h.learn_rotary(10);
        assert_eq!(h.learn.draft().controls.get(&10), Some(&1));
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
    }

    #[test]
    fn next_control_packet_during_settle_is_not_taken_by_control_one() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        for _ in 0..3 {
            h.send(&[0xb0, 10, 63]);
        }
        h.settle();
        for _ in 0..3 {
            h.send(&[0xb0, 10, 65]);
        }
        h.send(&[0xb0, 11, 65]);
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
        assert_eq!(h.learn.draft().controls.len(), 1);
        h.learn_rotary(11);
        assert_eq!(h.learn.draft().controls.len(), 2);
        assert_eq!(h.learn.draft().controls[&11], 2);
    }

    #[test]
    fn cc_button_press_and_release_advance_exactly_once() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        h.skip_controls();
        h.send(&[0xb2, 44, 127]);
        h.send(&[0xb2, 44, 127]);
        assert_eq!(h.learn.role(), LearnRole::Pad(0));
        h.send(&[0xb2, 44, 0]);
        assert_eq!(h.learn.role(), LearnRole::Pad(1));
        h.send(&[0xb2, 44, 0]);
        assert_eq!(h.learn.role(), LearnRole::Pad(1));
    }

    #[test]
    fn note_off_and_velocity_zero_release_each_advance_once() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        h.skip_controls();
        h.send(&[0x99, 36, 100]);
        h.send(&[0x89, 36, 0]);
        assert_eq!(h.learn.role(), LearnRole::Pad(1));
        h.send(&[0x99, 37, 100]);
        h.send(&[0x99, 37, 0]);
        assert_eq!(h.learn.role(), LearnRole::Pad(2));
        h.send(&[0x99, 37, 0]);
        assert_eq!(h.learn.role(), LearnRole::Pad(2));
    }

    #[test]
    fn learned_master_rotary_changes_steps_minus_plus_without_a_click() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);

        h.send(&[0xb0, 28, 65]);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
        h.send(&[0xb0, 28, 65]);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
        h.settle();
        h.send(&[0xb0, 28, 63]);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));

        h.settle();
        h.send(&[0xb0, 10, 63]);
        h.send(&[0xb0, 10, 62]);
        h.send(&[0xb0, 1, 1]);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        h.send(&[0xb0, 28, 63]);
        assert_eq!(h.learn.role(), LearnRole::EncoderModifier);
    }

    #[test]
    fn waiting_never_advances_an_unlearned_shift_click_or_pad() {
        let mut h = Harness::new();
        h.send(&[0xb0, 28, 63]);
        h.settle();
        h.send(&[0xb0, 28, 65]);
        h.settle();
        h.send(&[0xb0, 118, 127]);
        h.send(&[0xb0, 118, 0]);
        assert_eq!(h.learn.role(), LearnRole::EncoderModifier);
        h.now += Duration::from_secs(30);
        h.learn.tick(h.now);
        assert_eq!(h.learn.role(), LearnRole::EncoderModifier);

        assert!(h.learn.skip());
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        for _ in 0..8 {
            assert!(h.learn.skip());
        }
        assert_eq!(h.learn.role(), LearnRole::SecondaryEncoderClick);
        h.now += Duration::from_secs(30);
        h.learn.tick(h.now);
        assert_eq!(h.learn.role(), LearnRole::SecondaryEncoderClick);

        h.send(&[0xb0, 117, 127]);
        h.send(&[0xb0, 117, 0]);
        h.skip_controls();
        assert_eq!(h.learn.role(), LearnRole::Pad(0));
        h.now += Duration::from_secs(30);
        h.learn.tick(h.now);
        assert_eq!(h.learn.role(), LearnRole::Pad(0));
    }

    #[test]
    fn save_click_repeats_and_release_produce_one_action() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        assert_eq!(h.send(&[0xb0, 118, 127]), LearnAction::None);
        assert_eq!(h.send(&[0xb0, 118, 0]), LearnAction::None);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(0));
        h.settle();
        h.skip_to_confirm();
        assert_eq!(h.send(&[0xb0, 118, 127]), LearnAction::Save);
        assert_eq!(h.send(&[0xb0, 118, 127]), LearnAction::None);
        h.learn.mark_save_result(true);
        assert_eq!(h.send(&[0xb0, 118, 0]), LearnAction::FinishSaved);
        assert_eq!(h.send(&[0xb0, 118, 0]), LearnAction::None);
    }

    #[test]
    fn retry_clears_only_current_role_and_reentry_has_fresh_quarantine() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        h.send(&[0xb0, 10, 63]);
        h.learn.retry_at(h.now);
        assert_eq!(h.learn.draft().encoder_relative_cc, Some(28));
        assert_eq!(h.learn.draft().encoder_press_cc, Some(118));
        assert!(!h
            .learn
            .draft()
            .controls
            .values()
            .any(|position| *position == 1));
        h.send(&[0xb0, 10, 63]);
        assert!(!h
            .learn
            .draft()
            .controls
            .values()
            .any(|position| *position == 1));
        h.now += ENTRY_QUIET;
        h.learn.tick(h.now);
        h.learn_rotary(11);
        assert_eq!(h.learn.draft().controls[&11], 1);

        let mut reentered = LearnSession::new_at("Controller", h.now);
        reentered.receive(&[0xb0, 118, 0], h.now + Duration::from_millis(1));
        assert_eq!(reentered.draft().encoder_relative_cc, None);
        assert_eq!(reentered.role(), LearnRole::EncoderCounterClockwise);
    }

    #[test]
    fn minilab_daw_and_user_mode_encoder_pairs_both_learn() {
        for (rotary, click) in [(28, 118), (114, 115)] {
            let mut h = Harness::new();
            h.learn_master(rotary, click, false);
            h.skip_controls();
            let config = h.learn.validated_config().unwrap();
            assert_eq!(config.encoder_relative_cc, Some(rotary));
            assert_eq!(config.encoder_press_cc, Some(click));
        }
    }

    #[test]
    fn high_low_encoder_reset_zero_is_part_of_the_gesture() {
        let mut h = Harness::new();
        h.learn_master(114, 115, true);
        assert!(h.learn.draft().encoder_relative_reverse);
        h.learn_rotary(10);
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::RotaryTurn(1));
    }

    #[test]
    fn trailing_traffic_cannot_replace_accepted_success_with_conflict() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        for _ in 0..3 {
            h.send(&[0xb0, 10, 63]);
        }
        h.settle();
        for _ in 0..3 {
            h.send(&[0xb0, 10, 65]);
        }
        let success = h.learn.feedback().to_owned();
        h.send(&[0xb0, 10, 66]);
        h.send(&[0xb0, 28, 64]);
        assert_eq!(h.learn.feedback(), success);
        assert!(success.contains("OK"));
    }

    #[test]
    fn optional_command_roles_still_infer_five_button_layout() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        h.skip_controls();
        for _ in 0..4 {
            assert!(h.learn.skip());
        }
        assert_eq!(h.learn.role(), LearnRole::Pad(4));
        h.send(&[0x99, 40, 100]);
        h.send(&[0x89, 40, 0]);
        h.send(&[0x99, 40, 100]);
        assert!(h.learn.draft().pads.is_empty());
        h.send(&[0x89, 40, 0]);
        assert_eq!(h.learn.draft().layout, ControllerLayout::Five);
        assert_eq!(h.learn.draft().pads[&40], PadAction::Pad1);
    }

    #[test]
    fn four_dedicated_page_buttons_bypass_page_cycle_role() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        h.skip_controls();
        for note in 36..=39 {
            h.send(&[0x99, note, 100]);
            h.send(&[0x89, note, 0]);
        }
        assert_eq!(h.learn.role(), LearnRole::Pad(5));
        assert_eq!(h.learn.draft().layout, ControllerLayout::Eight);
        assert!(!h
            .learn
            .draft()
            .pads
            .values()
            .any(|action| *action == PadAction::CyclePage));
    }

    #[test]
    fn page_cycle_appears_only_after_all_four_page_buttons_are_skipped() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        h.skip_controls();
        for _ in 0..4 {
            assert!(h.learn.skip());
        }
        assert_eq!(h.learn.role(), LearnRole::Pad(4));

        h.send(&[0xb0, 44, 127]);
        h.send(&[0xb0, 44, 0]);
        assert_eq!(h.learn.role(), LearnRole::Pad(4));
        assert!(h.learn.draft().cc_buttons.is_empty());
        h.send(&[0xb0, 44, 127]);
        h.send(&[0xb0, 44, 0]);
        h.settle();
        assert_eq!(h.learn.role(), LearnRole::Pad(5));
        assert_eq!(h.learn.draft().cc_buttons[&44], PadAction::Pad1);
    }

    #[test]
    fn page_cycle_chord_ignores_modifier_press_and_may_reuse_a_control() {
        let mut h = Harness::new();
        h.learn_master(28, 118, false);
        h.learn_rotary(10);
        h.skip_controls();
        for _ in 0..4 {
            assert!(h.learn.skip());
        }
        assert_eq!(h.learn.role(), LearnRole::Pad(4));

        h.send(&[0xb0, 27, 127]);
        h.send(&[0xb0, 27, 0]);
        assert_eq!(h.learn.draft().page_cycle_modifier, None);
        h.send(&[0xb0, 27, 127]);
        assert_eq!(h.learn.draft().page_cycle_modifier, None);
        h.send(&[0xb0, 10, 65]);
        assert_eq!(
            h.learn.draft().page_cycle_modifier,
            Some(ControllerButton::Cc { channel: 0, cc: 27 })
        );
        assert_eq!(
            h.learn.draft().page_cycle_trigger,
            Some(ControllerButton::Cc { channel: 0, cc: 10 })
        );
        assert!(h.learn.feedback().contains("OK"));
        h.send(&[0xb0, 10, 66]);
        assert_eq!(h.learn.role(), LearnRole::Pad(4));
        h.send(&[0xb0, 27, 0]);
        assert_eq!(h.learn.role(), LearnRole::Pad(5));
        h.learn.validated_config().unwrap();
    }
}
