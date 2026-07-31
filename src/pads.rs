use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, Instant};

const DEFAULT_CONTROLLER_CONFIG: &str = include_str!("../config/controller.conf");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PadAction {
    Pad1,
    Pad2,
    Pad3,
    Pad4,
    Pad5,
    Pad6,
    Pad7,
    Pad8,
    // Legacy persisted meanings. Loading normalizes these to physical pads;
    // they remain readable so existing private profiles keep working.
    Page1,
    Page2,
    Page3,
    Page4,
    CyclePage,
    Item1,
    Item2,
    Item3,
    Item4,
    // Legacy v1 names retain the physical eight-pad order and normalize to
    // numbered PAD positions before routing or persistence.
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
pub enum ControllerLayout {
    Eight,
    Five,
    Four,
}

impl Default for ControllerLayout {
    fn default() -> Self {
        Self::Eight
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuInput {
    SelectPage(usize),
    CyclePage,
    ActivateItem(usize),
}

impl PadAction {
    pub const fn physical(number: u8) -> Option<Self> {
        match number {
            1 => Some(Self::Pad1),
            2 => Some(Self::Pad2),
            3 => Some(Self::Pad3),
            4 => Some(Self::Pad4),
            5 => Some(Self::Pad5),
            6 => Some(Self::Pad6),
            7 => Some(Self::Pad7),
            8 => Some(Self::Pad8),
            _ => None,
        }
    }

    pub const fn number(self) -> Option<u8> {
        match self {
            Self::Pad1 => Some(1),
            Self::Pad2 => Some(2),
            Self::Pad3 => Some(3),
            Self::Pad4 => Some(4),
            Self::Pad5 => Some(5),
            Self::Pad6 => Some(6),
            Self::Pad7 => Some(7),
            Self::Pad8 => Some(8),
            _ => None,
        }
    }

    pub const fn normalized(self, layout: ControllerLayout) -> Self {
        if self.number().is_some() {
            return self;
        }
        match self {
            Self::Page1 | Self::Arp => Self::Pad1,
            Self::Page2 | Self::Pad => Self::Pad2,
            Self::Page3 | Self::Prog => Self::Pad3,
            Self::Page4 | Self::Loop => Self::Pad4,
            Self::CyclePage => Self::Pad1,
            Self::Item1 | Self::Stop => match layout {
                ControllerLayout::Eight => Self::Pad5,
                ControllerLayout::Five => Self::Pad2,
                ControllerLayout::Four => Self::Pad1,
            },
            Self::Item2 | Self::Play => match layout {
                ControllerLayout::Eight => Self::Pad6,
                ControllerLayout::Five => Self::Pad3,
                ControllerLayout::Four => Self::Pad2,
            },
            Self::Item3 | Self::Rec => match layout {
                ControllerLayout::Eight => Self::Pad7,
                ControllerLayout::Five => Self::Pad4,
                ControllerLayout::Four => Self::Pad3,
            },
            Self::Item4 | Self::TapTempo => match layout {
                ControllerLayout::Eight => Self::Pad8,
                ControllerLayout::Five => Self::Pad5,
                ControllerLayout::Four => Self::Pad4,
            },
            Self::Pad1
            | Self::Pad2
            | Self::Pad3
            | Self::Pad4
            | Self::Pad5
            | Self::Pad6
            | Self::Pad7
            | Self::Pad8 => self,
        }
    }

    pub const fn menu_input_for(self, layout: ControllerLayout) -> MenuInput {
        let physical = self.normalized(layout);
        match layout {
            ControllerLayout::Eight => match physical {
                Self::Pad1 => MenuInput::SelectPage(0),
                Self::Pad2 => MenuInput::SelectPage(1),
                Self::Pad3 => MenuInput::SelectPage(2),
                Self::Pad4 => MenuInput::SelectPage(3),
                Self::Pad5 => MenuInput::ActivateItem(0),
                Self::Pad6 => MenuInput::ActivateItem(1),
                Self::Pad7 => MenuInput::ActivateItem(2),
                Self::Pad8 => MenuInput::ActivateItem(3),
                _ => unreachable!(),
            },
            ControllerLayout::Five => match physical {
                Self::Pad1 => MenuInput::CyclePage,
                Self::Pad2 => MenuInput::ActivateItem(0),
                Self::Pad3 => MenuInput::ActivateItem(1),
                Self::Pad4 => MenuInput::ActivateItem(2),
                Self::Pad5 => MenuInput::ActivateItem(3),
                _ => unreachable!(),
            },
            ControllerLayout::Four => match physical {
                Self::Pad1 => MenuInput::ActivateItem(0),
                Self::Pad2 => MenuInput::ActivateItem(1),
                Self::Pad3 => MenuInput::ActivateItem(2),
                Self::Pad4 => MenuInput::ActivateItem(3),
                _ => unreachable!(),
            },
        }
    }

    #[cfg(test)]
    pub const fn menu_input(self) -> MenuInput {
        match self {
            Self::Pad1 => MenuInput::SelectPage(0),
            Self::Pad2 => MenuInput::SelectPage(1),
            Self::Pad3 => MenuInput::SelectPage(2),
            Self::Pad4 => MenuInput::SelectPage(3),
            Self::Pad5 => MenuInput::ActivateItem(0),
            Self::Pad6 => MenuInput::ActivateItem(1),
            Self::Pad7 => MenuInput::ActivateItem(2),
            Self::Pad8 => MenuInput::ActivateItem(3),
            Self::Page1 | Self::Arp => MenuInput::SelectPage(0),
            Self::Page2 | Self::Pad => MenuInput::SelectPage(1),
            Self::Page3 | Self::Prog => MenuInput::SelectPage(2),
            Self::Page4 | Self::Loop => MenuInput::SelectPage(3),
            Self::CyclePage => MenuInput::CyclePage,
            Self::Item1 | Self::Stop => MenuInput::ActivateItem(0),
            Self::Item2 | Self::Play => MenuInput::ActivateItem(1),
            Self::Item3 | Self::Rec => MenuInput::ActivateItem(2),
            Self::Item4 | Self::TapTempo => MenuInput::ActivateItem(3),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncoderAction {
    Up,
    Down,
    Select,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControllerButton {
    Cc { channel: u8, cc: u8 },
    Note { channel: u8, note: u8 },
}

impl ControllerButton {
    fn matches(self, message: &[u8]) -> bool {
        if message.len() < 3 {
            return false;
        }
        match self {
            Self::Cc { channel, cc } => {
                message[0] & 0xf0 == 0xb0 && message[0] & 0x0f == channel && message[1] == cc
            }
            Self::Note { channel, note } => {
                matches!(message[0] & 0xf0, 0x80 | 0x90)
                    && message[0] & 0x0f == channel
                    && message[1] == note
            }
        }
    }

    fn pressed(self, message: &[u8]) -> bool {
        self.matches(message)
            && match self {
                Self::Cc { .. } => message[2] > 0,
                Self::Note { .. } => message[0] & 0xf0 == 0x90 && message[2] > 0,
            }
    }

    fn setting(self) -> String {
        match self {
            Self::Cc { channel, cc } => format!("cc.{}.{cc}", channel + 1),
            Self::Note { channel, note } => format!("note.{}.{note}", channel + 1),
        }
    }
}

impl fmt::Display for ControllerButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.setting())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PageCycleChordState {
    modifier_down: bool,
    triggered: bool,
}

impl fmt::Display for PadAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pad1 => "pad-1",
            Self::Pad2 => "pad-2",
            Self::Pad3 => "pad-3",
            Self::Pad4 => "pad-4",
            Self::Pad5 => "pad-5",
            Self::Pad6 => "pad-6",
            Self::Pad7 => "pad-7",
            Self::Pad8 => "pad-8",
            Self::Page1 => "page-1",
            Self::Page2 => "page-2",
            Self::Page3 => "page-3",
            Self::Page4 => "page-4",
            Self::CyclePage => "page-cycle",
            Self::Item1 => "item-1",
            Self::Item2 => "item-2",
            Self::Item3 => "item-3",
            Self::Item4 => "item-4",
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
            "pad-1" | "pad1" => Ok(Self::Pad1),
            "pad-2" | "pad2" => Ok(Self::Pad2),
            "pad-3" | "pad3" => Ok(Self::Pad3),
            "pad-4" | "pad4" => Ok(Self::Pad4),
            "pad-5" | "pad5" => Ok(Self::Pad5),
            "pad-6" | "pad6" => Ok(Self::Pad6),
            "pad-7" | "pad7" => Ok(Self::Pad7),
            "pad-8" | "pad8" => Ok(Self::Pad8),
            "page-1" | "page1" => Ok(Self::Page1),
            "page-2" | "page2" => Ok(Self::Page2),
            "page-3" | "page3" => Ok(Self::Page3),
            "page-4" | "page4" => Ok(Self::Page4),
            "page-cycle" | "cycle-page" | "cycle" => Ok(Self::CyclePage),
            "item-1" | "item1" => Ok(Self::Item1),
            "item-2" | "item2" => Ok(Self::Item2),
            "item-3" | "item3" => Ok(Self::Item3),
            "item-4" | "item4" => Ok(Self::Item4),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PadConfig {
    pub input_match: Option<String>,
    /// Reviewed catalog ID, `learned`, or absent for legacy/unmapped state.
    pub profile: Option<String>,
    pub pads: HashMap<u8, PadAction>,
    /// Optional zero-based MIDI channel for each note command. Missing keeps
    /// the legacy behavior of matching the note on every channel.
    pub pad_channels: HashMap<u8, u8>,
    /// Incoming controller CC buttons. Note buttons remain in `pads` for
    /// compatibility with the original profile format.
    pub cc_buttons: HashMap<u8, PadAction>,
    /// Optional zero-based MIDI channel for each CC command.
    pub cc_button_channels: HashMap<u8, u8>,
    /// Incoming controller CC -> one-based physical POT position.
    pub controls: HashMap<u8, u8>,
    pub encoder_relative_cc: Option<u8>,
    pub encoder_relative_reverse: bool,
    /// Optional relative CC emitted only while the configured encoder
    /// modifier is held. Some controllers change the encoder's CC instead of
    /// continuing to emit `encoder_relative_cc`.
    pub encoder_modified_relative_cc: Option<u8>,
    pub encoder_modified_relative_reverse: bool,
    pub encoder_press_cc: Option<u8>,
    pub encoder_press_note: Option<u8>,
    /// Optional zero-based channel qualifier for either encoder press form.
    pub encoder_press_channel: Option<u8>,
    /// Held controller modifier used to give the encoder its secondary
    /// navigation gesture.
    pub encoder_modifier: Option<ControllerButton>,
    /// Optional held modifier plus secondary gesture for page-cycle. The
    /// trigger may reuse a normally mapped control because it is active only
    /// while the modifier is held.
    pub page_cycle_modifier: Option<ControllerButton>,
    pub page_cycle_trigger: Option<ControllerButton>,
    /// Dedicated toggle control; this uses the raw Shift CC, not its shifted pad layer.
    pub lock_cc: Option<u8>,
    pub layout: ControllerLayout,
}

impl Default for PadConfig {
    fn default() -> Self {
        let mut config = Self {
            input_match: None,
            profile: None,
            pads: HashMap::new(),
            pad_channels: HashMap::new(),
            cc_buttons: HashMap::new(),
            cc_button_channels: HashMap::new(),
            controls: HashMap::new(),
            encoder_relative_cc: None,
            encoder_relative_reverse: false,
            encoder_modified_relative_cc: None,
            encoder_modified_relative_reverse: false,
            encoder_press_cc: None,
            encoder_press_note: None,
            encoder_press_channel: None,
            encoder_modifier: None,
            page_cycle_modifier: None,
            page_cycle_trigger: None,
            lock_cc: None,
            layout: ControllerLayout::Eight,
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
    pub fn unmapped(input_match: impl Into<String>) -> Self {
        Self {
            input_match: Some(input_match.into()),
            ..Self::default()
        }
    }

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
        let mut saw_cc_buttons = false;
        let mut saw_controls = false;
        let mut saw_physical_pads = false;
        let mut saw_positional_pots = false;
        let mut saw_legacy_controls = false;
        let mut saw_legacy_pads = false;
        for (line_no, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=').with_context(|| {
                format!("{}:{}: expected KEY=VALUE", path.display(), line_no + 1)
            })?;
            if key.trim() == "input" {
                self.input_match = (!value.trim().is_empty()).then(|| value.trim().to_owned());
                continue;
            }
            if key.trim() == "profile" {
                self.profile = (!value.trim().is_empty()).then(|| value.trim().to_owned());
                continue;
            }
            if key.trim() == "menu.layout" {
                self.layout = match value.trim() {
                    "8" | "eight" => ControllerLayout::Eight,
                    "5" | "five" => ControllerLayout::Five,
                    "4" | "four" => ControllerLayout::Four,
                    _ => bail!("menu.layout must be 8, 5, or 4"),
                };
                continue;
            }
            if key.trim() == "encoder.relative_cc" {
                self.encoder_relative_cc = optional_midi_number(value, "encoder relative CC")?;
                continue;
            }
            if key.trim() == "encoder.relative_reverse" {
                self.encoder_relative_reverse = match value.trim() {
                    "true" | "yes" | "1" => true,
                    "false" | "no" | "0" => false,
                    _ => bail!("encoder.relative_reverse must be true or false"),
                };
                continue;
            }
            if key.trim() == "encoder.modified_relative_cc" {
                self.encoder_modified_relative_cc =
                    optional_midi_number(value, "modified encoder relative CC")?;
                continue;
            }
            if key.trim() == "encoder.modified_relative_reverse" {
                self.encoder_modified_relative_reverse = match value.trim() {
                    "true" | "yes" | "1" => true,
                    "false" | "no" | "0" => false,
                    _ => bail!("encoder.modified_relative_reverse must be true or false"),
                };
                continue;
            }
            if key.trim() == "encoder.press_cc" {
                self.encoder_press_cc = optional_midi_number(value, "encoder press CC")?;
                continue;
            }
            if key.trim() == "encoder.press_note" {
                self.encoder_press_note = optional_midi_number(value, "encoder press note")?;
                continue;
            }
            if key.trim() == "encoder.press_channel" {
                self.encoder_press_channel = optional_midi_channel(value, "encoder press channel")?;
                continue;
            }
            if key.trim() == "encoder.modifier" {
                self.encoder_modifier = optional_controller_button(value, "encoder modifier")?;
                continue;
            }
            if key.trim() == "page_cycle.modifier" {
                self.page_cycle_modifier =
                    optional_controller_button(value, "page-cycle modifier")?;
                continue;
            }
            if key.trim() == "page_cycle.trigger" {
                self.page_cycle_trigger = optional_controller_button(value, "page-cycle trigger")?;
                continue;
            }
            if key.trim() == "lock.cc" {
                self.lock_cc = optional_midi_number(value, "pad lock CC")?;
                continue;
            }
            if let Some(position) = key.trim().strip_prefix("pot.") {
                if saw_legacy_controls {
                    bail!(
                        "{}:{}: cannot mix positional and legacy POT mappings",
                        path.display(),
                        line_no + 1
                    );
                }
                saw_positional_pots = true;
                if !saw_controls {
                    self.controls.clear();
                    saw_controls = true;
                }
                let position = physical_position(position, 12, "POT position")?;
                let incoming = midi_number(value, "controller CC")?;
                self.controls.insert(incoming, position);
                continue;
            }
            if let Some(position) = key.trim().strip_prefix("pad.") {
                if !position.contains('.')
                    && (value.trim().starts_with("cc.") || value.trim().starts_with("note."))
                {
                    if saw_legacy_pads {
                        bail!(
                            "{}:{}: cannot mix positional and legacy PAD mappings",
                            path.display(),
                            line_no + 1
                        );
                    }
                    if !saw_physical_pads {
                        self.pads.clear();
                        self.pad_channels.clear();
                        self.cc_buttons.clear();
                        self.cc_button_channels.clear();
                        saw_physical_pads = true;
                    }
                    let position = physical_position(position, 8, "PAD position")?;
                    let binding = physical_pad_binding(value)?;
                    let pad = PadAction::physical(position).expect("validated PAD position");
                    match binding {
                        PhysicalPadBinding::Cc { channel, cc } => {
                            self.cc_buttons.insert(cc, pad);
                            match channel {
                                Some(channel) => {
                                    self.cc_button_channels.insert(cc, channel);
                                }
                                None => {
                                    self.cc_button_channels.remove(&cc);
                                }
                            }
                        }
                        PhysicalPadBinding::Note { channel, note } => {
                            self.pads.insert(note, pad);
                            match channel {
                                Some(channel) => {
                                    self.pad_channels.insert(note, channel);
                                }
                                None => {
                                    self.pad_channels.remove(&note);
                                }
                            }
                        }
                    }
                    continue;
                }
            }
            if let Some(raw) = key.trim().strip_prefix("cc.") {
                if saw_positional_pots {
                    bail!(
                        "{}:{}: cannot mix positional and legacy POT mappings",
                        path.display(),
                        line_no + 1
                    );
                }
                saw_legacy_controls = true;
                if !saw_controls {
                    self.controls.clear();
                    saw_controls = true;
                }
                let raw = midi_number(raw, "controller CC")?;
                let target: u8 = value
                    .trim()
                    .parse()
                    .context("target CC must be a mapped CC number")?;
                if crate::control::by_cc(target).is_none() {
                    bail!("target CC {target} is not one of the 12 mapped controls");
                }
                let position = crate::control::CONTROLS
                    .iter()
                    .position(|control| control.cc == target)
                    .expect("validated legacy target CC") as u8
                    + 1;
                self.controls.insert(raw, position);
                continue;
            }
            if let Some(raw) = key.trim().strip_prefix("button.cc.") {
                if saw_physical_pads {
                    bail!(
                        "{}:{}: cannot mix positional and legacy PAD mappings",
                        path.display(),
                        line_no + 1
                    );
                }
                saw_legacy_pads = true;
                if !saw_cc_buttons {
                    self.cc_buttons.clear();
                    self.cc_button_channels.clear();
                    saw_cc_buttons = true;
                }
                let (channel, raw) = command_binding(raw, "controller button CC")?;
                self.cc_buttons.insert(raw, value.trim().parse()?);
                match channel {
                    Some(channel) => {
                        self.cc_button_channels.insert(raw, channel);
                    }
                    None => {
                        self.cc_button_channels.remove(&raw);
                    }
                }
                continue;
            }
            if saw_physical_pads {
                bail!(
                    "{}:{}: cannot mix positional and legacy PAD mappings",
                    path.display(),
                    line_no + 1
                );
            }
            saw_legacy_pads = true;
            if !saw_pads {
                self.pads.clear();
                self.pad_channels.clear();
                saw_pads = true;
            }
            let note_text = key.trim().strip_prefix("pad.").unwrap_or(key.trim());
            let (channel, note) = command_binding(note_text, "pad note")?;
            self.pads.insert(note, value.trim().parse()?);
            match channel {
                Some(channel) => {
                    self.pad_channels.insert(note, channel);
                }
                None => {
                    self.pad_channels.remove(&note);
                }
            }
        }
        for pad in self.pads.values_mut().chain(self.cc_buttons.values_mut()) {
            *pad = pad.normalized(self.layout);
        }
        self.validate()
    }

    pub fn validate(&self) -> Result<()> {
        if self.input_match.as_ref().is_some_and(|input| {
            input.trim().is_empty() || input.trim() != input || input.contains(['\n', '\r'])
        }) {
            bail!("controller input match must be a non-empty single-line value");
        }
        if self.profile.as_ref().is_some_and(|profile| {
            profile.trim().is_empty()
                || profile.trim() != profile
                || profile.contains(['\n', '\r', '='])
        }) {
            bail!("controller profile marker must be a non-empty single-line value");
        }
        for &cc in self.controls.keys() {
            ensure_midi_number(cc, "controller CC")?;
        }
        let mut positions = HashSet::new();
        for &position in self.controls.values() {
            if !(1..=12).contains(&position) {
                bail!("POT position must be 1..12");
            }
            if !positions.insert(position) {
                bail!("POT {position} is mapped more than once");
            }
        }
        for &cc in self.cc_buttons.keys() {
            ensure_midi_number(cc, "controller button CC")?;
        }
        for &note in self.pads.keys() {
            ensure_midi_number(note, "pad note")?;
        }
        let maximum_pad = match self.layout {
            ControllerLayout::Eight => 8,
            ControllerLayout::Five => 5,
            ControllerLayout::Four => 4,
        };
        let mut physical_pads = HashSet::new();
        for pad in self.pads.values().chain(self.cc_buttons.values()) {
            let position = pad
                .normalized(self.layout)
                .number()
                .expect("normalization produces a physical PAD");
            if position > maximum_pad {
                bail!("PAD {position} exceeds the configured {maximum_pad}-pad layout");
            }
            if !physical_pads.insert(position) {
                bail!("PAD {position} is mapped more than once");
            }
        }
        if self
            .pad_channels
            .iter()
            .any(|(note, channel)| !self.pads.contains_key(note) || *channel > 15)
        {
            bail!("pad channel qualifiers require a mapped note and channel 1..16");
        }
        if self
            .cc_button_channels
            .iter()
            .any(|(cc, channel)| !self.cc_buttons.contains_key(cc) || *channel > 15)
        {
            bail!("button CC channel qualifiers require a mapped CC and channel 1..16");
        }
        for (number, description) in [
            (self.encoder_relative_cc, "encoder relative CC"),
            (
                self.encoder_modified_relative_cc,
                "modified encoder relative CC",
            ),
            (self.encoder_press_cc, "encoder press CC"),
            (self.encoder_press_note, "encoder press note"),
            (self.lock_cc, "pad lock CC"),
        ] {
            if let Some(number) = number {
                ensure_midi_number(number, description)?;
            }
        }
        for encoder_cc in [
            self.encoder_relative_cc,
            self.encoder_modified_relative_cc,
            self.encoder_press_cc,
            self.lock_cc,
        ]
        .into_iter()
        .flatten()
        {
            if self.controls.contains_key(&encoder_cc) {
                bail!("encoder CC {encoder_cc} is also mapped as a POT");
            }
            if self.cc_buttons.contains_key(&encoder_cc) {
                bail!("encoder CC {encoder_cc} is also mapped as a PAD");
            }
        }
        if self
            .controls
            .keys()
            .any(|cc| self.cc_buttons.contains_key(cc))
        {
            bail!("a controller CC cannot be both a POT and a PAD");
        }
        if (self.encoder_relative_cc == self.encoder_press_cc && self.encoder_relative_cc.is_some())
            || (self.encoder_modified_relative_cc == self.encoder_press_cc
                && self.encoder_modified_relative_cc.is_some())
            || (self.encoder_relative_cc == self.encoder_modified_relative_cc
                && self.encoder_relative_cc.is_some())
        {
            bail!("ordinary turn, shifted turn, and encoder press CCs must be different");
        }
        if self.lock_cc.is_some()
            && [
                self.encoder_relative_cc,
                self.encoder_modified_relative_cc,
                self.encoder_press_cc,
            ]
            .contains(&self.lock_cc)
        {
            bail!("pad lock CC must differ from encoder CCs");
        }
        if self.encoder_modified_relative_cc.is_some() && self.encoder_modifier.is_none() {
            bail!("modified encoder relative CC requires an encoder modifier");
        }
        if self.encoder_modified_relative_reverse && self.encoder_modified_relative_cc.is_none() {
            bail!("modified encoder reverse requires a modified encoder relative CC");
        }
        if self.encoder_press_cc.is_some() && self.encoder_press_note.is_some() {
            bail!("encoder press must use either a CC or a note, not both");
        }
        if self
            .encoder_press_channel
            .is_some_and(|channel| channel > 15)
            || (self.encoder_press_channel.is_some()
                && self.encoder_press_cc.is_none()
                && self.encoder_press_note.is_none())
        {
            bail!("encoder press channel requires an encoder press CC or note and channel 1..16");
        }
        if let Some(modifier) = self.encoder_modifier {
            match modifier {
                ControllerButton::Cc { channel, cc } => {
                    ensure_midi_number(cc, "encoder modifier")?;
                    if channel > 15
                        || self.controls.contains_key(&cc)
                        || self.cc_buttons.contains_key(&cc)
                        || [
                            self.encoder_relative_cc,
                            self.encoder_modified_relative_cc,
                            self.encoder_press_cc,
                            self.lock_cc,
                        ]
                        .contains(&Some(cc))
                    {
                        bail!("encoder modifier must be a dedicated CC on channel 1..16");
                    }
                }
                ControllerButton::Note { channel, note } => {
                    ensure_midi_number(note, "encoder modifier")?;
                    if channel > 15
                        || self.pads.contains_key(&note)
                        || self.encoder_press_note == Some(note)
                    {
                        bail!("encoder modifier must be a dedicated note on channel 1..16");
                    }
                }
            }
        }
        if self.page_cycle_modifier.is_some() != self.page_cycle_trigger.is_some() {
            bail!("page-cycle modifier and trigger must be configured together");
        }
        for (button, description) in [
            (self.page_cycle_modifier, "page-cycle modifier"),
            (self.page_cycle_trigger, "page-cycle trigger"),
        ] {
            match button {
                Some(ControllerButton::Cc { channel, cc }) => {
                    ensure_midi_number(cc, description)?;
                    if channel > 15 {
                        bail!("{description} channel must be 1..16");
                    }
                }
                Some(ControllerButton::Note { channel, note }) => {
                    ensure_midi_number(note, description)?;
                    if channel > 15 {
                        bail!("{description} channel must be 1..16");
                    }
                }
                None => {}
            }
        }
        if self.page_cycle_modifier.is_some() && self.page_cycle_modifier == self.page_cycle_trigger
        {
            bail!("page-cycle modifier and trigger must be different messages");
        }
        if let Some(modifier) = self.page_cycle_modifier {
            let conflicts = match modifier {
                ControllerButton::Cc { cc, .. } => {
                    self.controls.contains_key(&cc)
                        || self.cc_buttons.contains_key(&cc)
                        || [
                            self.encoder_relative_cc,
                            self.encoder_press_cc,
                            self.lock_cc,
                        ]
                        .contains(&Some(cc))
                }
                ControllerButton::Note { note, .. } => {
                    self.pads.contains_key(&note) || self.encoder_press_note == Some(note)
                }
            };
            if conflicts {
                bail!("page-cycle modifier must be a dedicated, otherwise unmapped message");
            }
        }
        if self
            .encoder_press_note
            .is_some_and(|note| self.pads.contains_key(&note))
        {
            bail!("encoder press note is also mapped as a PAD");
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut entries: Vec<_> = self.pads.iter().collect();
        entries.sort_by_key(|(note, _)| **note);
        let mut text = String::from("# SHR-DAW controller profile v8\n");
        if let Some(input) = &self.input_match {
            text.push_str(&format!("input={input}\n"));
        }
        text.push_str(&format!(
            "profile={}\n",
            self.profile.as_deref().unwrap_or_default()
        ));
        text.push_str(&format!(
            "menu.layout={}\nencoder.relative_cc={}\nencoder.relative_reverse={}\nencoder.modified_relative_cc={}\nencoder.modified_relative_reverse={}\nencoder.press_cc={}\nencoder.press_note={}\nencoder.press_channel={}\nencoder.modifier={}\npage_cycle.modifier={}\npage_cycle.trigger={}\nlock.cc={}\n",
            match self.layout {
                ControllerLayout::Eight => 8,
                ControllerLayout::Five => 5,
                ControllerLayout::Four => 4,
            },
            self.encoder_relative_cc
                .map(|cc| cc.to_string())
                .unwrap_or_default(),
            self.encoder_relative_reverse,
            self.encoder_modified_relative_cc
                .map(|cc| cc.to_string())
                .unwrap_or_default(),
            self.encoder_modified_relative_reverse,
            self.encoder_press_cc
                .map(|cc| cc.to_string())
                .unwrap_or_default(),
            self.encoder_press_note
                .map(|note| note.to_string())
                .unwrap_or_default(),
            self.encoder_press_channel
                .map(|channel| (channel + 1).to_string())
                .unwrap_or_default(),
            self.encoder_modifier
                .map(ControllerButton::setting)
                .unwrap_or_default(),
            self.page_cycle_modifier
                .map(ControllerButton::setting)
                .unwrap_or_default(),
            self.page_cycle_trigger
                .map(ControllerButton::setting)
                .unwrap_or_default(),
            self.lock_cc.map(|cc| cc.to_string()).unwrap_or_default(),
        ));
        let mut controls: Vec<_> = self.controls.iter().collect();
        controls.sort_by_key(|(_, position)| **position);
        for (incoming, position) in controls {
            text.push_str(&format!("pot.{position}={incoming}\n"));
        }
        let mut pads = entries
            .into_iter()
            .map(|(note, pad)| {
                let binding = self.pad_channels.get(note).map_or_else(
                    || format!("note.any.{note}"),
                    |channel| format!("note.{}.{note}", channel + 1),
                );
                (pad.normalized(self.layout), binding)
            })
            .chain(self.cc_buttons.iter().map(|(cc, pad)| {
                let binding = self.cc_button_channels.get(cc).map_or_else(
                    || format!("cc.any.{cc}"),
                    |channel| format!("cc.{}.{cc}", channel + 1),
                );
                (pad.normalized(self.layout), binding)
            }))
            .collect::<Vec<_>>();
        pads.sort_by_key(|(pad, _)| pad.number());
        for (pad, binding) in pads {
            text.push_str(&format!(
                "pad.{}={}\n",
                pad.number().expect("validated physical PAD"),
                binding
            ));
        }
        crate::fsutil::atomic_write(path, text.as_bytes())
    }

    /// Returns an action only for note-on with non-zero velocity. Note-off is
    /// consumed too, preventing both stuck notes and double triggering.
    pub fn route(&self, message: &[u8]) -> (bool, Option<PadAction>) {
        if message.len() < 3 {
            return (false, None);
        }
        let kind = message[0] & 0xf0;
        if !matches!(kind, 0x80 | 0x90 | 0xa0) {
            return (false, None);
        }
        match self.note_action(message[0], message[1]) {
            Some(action) => (true, (kind == 0x90 && message[2] > 0).then_some(action)),
            None => (false, None),
        }
    }

    pub fn action_state(&self, message: &[u8]) -> Option<(PadAction, bool)> {
        if message.len() < 3 {
            return None;
        }
        let kind = message[0] & 0xf0;
        if kind == 0xb0 {
            return self
                .cc_action(message[0], message[1])
                .map(|action| (action, message[2] > 0));
        }
        if kind != 0x90 && kind != 0x80 {
            return None;
        }
        self.note_action(message[0], message[1]).map(|action| {
            let pressed = kind == 0x90 && message[2] > 0;
            (action, pressed)
        })
    }

    fn note_action(&self, status: u8, note: u8) -> Option<PadAction> {
        self.pads.get(&note).copied().filter(|_| {
            self.pad_channels
                .get(&note)
                .is_none_or(|channel| *channel == status & 0x0f)
        })
    }

    fn cc_action(&self, status: u8, cc: u8) -> Option<PadAction> {
        self.cc_buttons.get(&cc).copied().filter(|_| {
            self.cc_button_channels
                .get(&cc)
                .is_none_or(|channel| *channel == status & 0x0f)
        })
    }

    pub fn pot_position(&self, incoming: u8) -> Option<usize> {
        self.controls
            .get(&incoming)
            .copied()
            .filter(|position| (1..=12).contains(position))
            .map(|position| usize::from(position - 1))
    }

    /// Centered relative mode uses 64 as stationary. Reversed/high-low mode
    /// also treats its zero reset packet as stationary. Press and release are
    /// both consumed, while only a non-zero press selects.
    pub fn encoder_action(&self, message: &[u8]) -> (bool, Option<EncoderAction>) {
        let (consumed, action) = relative_encoder_action(
            message,
            self.encoder_relative_cc,
            self.encoder_relative_reverse,
        );
        if consumed {
            return (consumed, action);
        }
        self.encoder_press_action(message)
    }

    /// Classifies both ordinary and modifier-specific encoder CCs. A shifted
    /// CC is always consumed, but it navigates only while the configured
    /// modifier is actually held.
    pub fn encoder_action_with_modifier(
        &self,
        message: &[u8],
        modifier_down: bool,
    ) -> (bool, Option<EncoderAction>, bool) {
        let (modified_consumed, modified_action) = relative_encoder_action(
            message,
            self.encoder_modified_relative_cc,
            self.encoder_modified_relative_reverse,
        );
        if modified_consumed {
            return (
                true,
                modifier_down.then_some(modified_action).flatten(),
                modifier_down,
            );
        }
        let (consumed, action) = self.encoder_action(message);
        let modified = modifier_down
            && action.is_some()
            && self.encoder_relative_cc == message.get(1).copied();
        (consumed, action, modified)
    }

    fn encoder_press_action(&self, message: &[u8]) -> (bool, Option<EncoderAction>) {
        if message.len() < 3 || message[0] & 0xf0 != 0xb0 {
            return (false, None);
        }
        if self.encoder_press_cc == Some(message[1]) {
            if self
                .encoder_press_channel
                .is_some_and(|channel| channel != message[0] & 0x0f)
            {
                return (false, None);
            }
            return (true, (message[2] > 0).then_some(EncoderAction::Select));
        }
        (false, None)
    }

    pub fn encoder_note_action(&self, message: &[u8]) -> (bool, Option<EncoderAction>) {
        if message.len() < 3 || !matches!(message[0] & 0xf0, 0x80 | 0x90) {
            return (false, None);
        }
        if self.encoder_press_note != Some(message[1]) {
            return (false, None);
        }
        if self
            .encoder_press_channel
            .is_some_and(|channel| channel != message[0] & 0x0f)
        {
            return (false, None);
        }
        let pressed = message[0] & 0xf0 == 0x90 && message[2] > 0;
        (true, pressed.then_some(EncoderAction::Select))
    }

    pub fn encoder_modifier_action(&self, message: &[u8]) -> (bool, bool) {
        self.encoder_modifier
            .filter(|modifier| modifier.matches(message))
            .map_or((false, false), |modifier| (true, modifier.pressed(message)))
    }

    pub fn page_cycle_chord_action(
        &self,
        message: &[u8],
        state: &mut PageCycleChordState,
    ) -> (bool, Option<(PadAction, bool)>) {
        let (Some(modifier), Some(trigger)) = (self.page_cycle_modifier, self.page_cycle_trigger)
        else {
            *state = PageCycleChordState::default();
            return (false, None);
        };
        if modifier.matches(message) {
            state.modifier_down = modifier.pressed(message);
            if !state.modifier_down {
                state.triggered = false;
            }
            return (true, None);
        }
        if state.modifier_down && trigger.matches(message) {
            let pressed = trigger.pressed(message);
            let action = (pressed && !state.triggered).then_some((PadAction::CyclePage, true));
            if pressed {
                state.triggered = true;
            }
            return (
                true,
                action.or_else(|| (!pressed).then_some((PadAction::CyclePage, false))),
            );
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

fn relative_encoder_action(
    message: &[u8],
    configured_cc: Option<u8>,
    reverse: bool,
) -> (bool, Option<EncoderAction>) {
    if message.len() < 3 || message[0] & 0xf0 != 0xb0 || configured_cc != Some(message[1]) {
        return (false, None);
    }
    let mut action = if reverse && message[2] == 0 {
        // Two's-complement/high-low relative encoders reset to zero. Zero is
        // neutral, not another clockwise packet.
        None
    } else {
        match message[2].cmp(&64) {
            std::cmp::Ordering::Less => Some(EncoderAction::Up),
            std::cmp::Ordering::Greater => Some(EncoderAction::Down),
            std::cmp::Ordering::Equal => None,
        }
    };
    if reverse {
        action = action.map(|action| match action {
            EncoderAction::Up => EncoderAction::Down,
            EncoderAction::Down => EncoderAction::Up,
            EncoderAction::Select => EncoderAction::Select,
        });
    }
    (true, action)
}

pub(crate) fn midi_number(value: &str, description: &str) -> Result<u8> {
    let number = value
        .parse::<u8>()
        .with_context(|| format!("{description} must be 0..127"))?;
    ensure_midi_number(number, description)?;
    Ok(number)
}

pub(crate) fn ensure_midi_number(number: u8, description: &str) -> Result<()> {
    if number > 127 {
        bail!("{description} must be 0..127");
    }
    Ok(())
}

fn physical_position(value: &str, maximum: u8, description: &str) -> Result<u8> {
    let position = value
        .trim()
        .parse::<u8>()
        .with_context(|| format!("{description} must be 1..{maximum}"))?;
    if !(1..=maximum).contains(&position) {
        bail!("{description} must be 1..{maximum}");
    }
    Ok(position)
}

enum PhysicalPadBinding {
    Cc { channel: Option<u8>, cc: u8 },
    Note { channel: Option<u8>, note: u8 },
}

fn physical_pad_binding(value: &str) -> Result<PhysicalPadBinding> {
    let parts = value.trim().split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        bail!("physical PAD binding must be cc.CHANNEL.NUMBER or note.CHANNEL.NUMBER");
    }
    let channel = if parts[1] == "any" {
        None
    } else {
        let channel = parts[1]
            .parse::<u8>()
            .context("physical PAD channel must be 1..16 or any")?;
        if !(1..=16).contains(&channel) {
            bail!("physical PAD channel must be 1..16 or any");
        }
        Some(channel - 1)
    };
    let number = midi_number(parts[2], "physical PAD MIDI number")?;
    match parts[0] {
        "cc" => Ok(PhysicalPadBinding::Cc {
            channel,
            cc: number,
        }),
        "note" => Ok(PhysicalPadBinding::Note {
            channel,
            note: number,
        }),
        _ => bail!("physical PAD binding must start with cc or note"),
    }
}

fn optional_midi_number(value: &str, description: &str) -> Result<Option<u8>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    midi_number(value, description).map(Some)
}

fn optional_midi_channel(value: &str, description: &str) -> Result<Option<u8>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let channel = value
        .parse::<u8>()
        .with_context(|| format!("{description} must be 1..16"))?;
    if !(1..=16).contains(&channel) {
        bail!("{description} must be 1..16");
    }
    Ok(Some(channel - 1))
}

fn optional_controller_button(value: &str, description: &str) -> Result<Option<ControllerButton>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        bail!("{description} must be cc.CHANNEL.NUMBER or note.CHANNEL.NUMBER");
    }
    let channel = parts[1]
        .parse::<u8>()
        .with_context(|| format!("{description} channel must be 1..16"))?;
    if !(1..=16).contains(&channel) {
        bail!("{description} channel must be 1..16");
    }
    let number = midi_number(parts[2], description)?;
    match parts[0] {
        "cc" => Ok(Some(ControllerButton::Cc {
            channel: channel - 1,
            cc: number,
        })),
        "note" => Ok(Some(ControllerButton::Note {
            channel: channel - 1,
            note: number,
        })),
        _ => bail!("{description} must start with cc or note"),
    }
}

fn command_binding(value: &str, description: &str) -> Result<(Option<u8>, u8)> {
    let Some((channel, number)) = value.split_once('.') else {
        return Ok((None, midi_number(value, description)?));
    };
    if number.contains('.') {
        bail!("{description} binding must be NUMBER or CHANNEL.NUMBER");
    }
    let channel = channel
        .parse::<u8>()
        .with_context(|| format!("{description} channel must be 1..16"))?;
    if !(1..=16).contains(&channel) {
        bail!("{description} channel must be 1..16");
    }
    Ok((Some(channel - 1), midi_number(number, description)?))
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
    fn channel_qualified_note_commands_consume_press_release_zero_release_and_pressure() {
        let c = PadConfig {
            pads: HashMap::from([(36, PadAction::Page1)]),
            pad_channels: HashMap::from([(36, 9)]),
            ..PadConfig::default()
        };
        for channel in 0..16 {
            let expected_press = if channel == 9 {
                (true, Some(PadAction::Page1))
            } else {
                (false, None)
            };
            assert_eq!(c.route(&[0x90 | channel, 36, 100]), expected_press);
            for (kind, value) in [(0x80, 0), (0x90, 0), (0xa0, 72)] {
                assert_eq!(c.route(&[kind | channel, 36, value]), (channel == 9, None));
            }
        }
    }

    #[test]
    fn channel_qualified_cc_commands_match_only_the_configured_channel() {
        let c = PadConfig {
            cc_buttons: HashMap::from([(44, PadAction::Item1)]),
            cc_button_channels: HashMap::from([(44, 9)]),
            ..PadConfig::default()
        };
        for channel in 0..16 {
            let expected_press = (channel == 9).then_some((PadAction::Item1, true));
            let expected_release = (channel == 9).then_some((PadAction::Item1, false));
            assert_eq!(c.action_state(&[0xb0 | channel, 44, 127]), expected_press);
            assert_eq!(c.action_state(&[0xb0 | channel, 44, 0]), expected_release);
        }
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
    fn high_low_relative_encoder_treats_zero_as_neutral() {
        let c = PadConfig {
            encoder_relative_cc: Some(114),
            encoder_relative_reverse: true,
            ..PadConfig::default()
        };
        assert_eq!(
            c.encoder_action(&[0xb0, 114, 125]),
            (true, Some(EncoderAction::Up))
        );
        assert_eq!(
            c.encoder_action(&[0xb0, 114, 1]),
            (true, Some(EncoderAction::Down))
        );
        assert_eq!(c.encoder_action(&[0xb0, 114, 0]), (true, None));
    }

    #[test]
    fn held_encoder_modifier_is_channel_qualified_and_round_trips() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-controller-modifier-{}.conf",
            std::process::id()
        ));
        let config = PadConfig {
            encoder_relative_cc: Some(114),
            encoder_modified_relative_cc: Some(29),
            encoder_modifier: Some(ControllerButton::Cc { channel: 0, cc: 27 }),
            ..PadConfig::default()
        };
        config.save(&path).unwrap();
        let loaded = PadConfig::load(&path).unwrap();
        assert_eq!(loaded.encoder_modifier, config.encoder_modifier);
        assert_eq!(
            loaded.encoder_modifier_action(&[0xb0, 27, 127]),
            (true, true)
        );
        assert_eq!(
            loaded.encoder_modifier_action(&[0xb0, 27, 0]),
            (true, false)
        );
        assert_eq!(
            loaded.encoder_modifier_action(&[0xb1, 27, 127]),
            (false, false)
        );
        assert_eq!(
            loaded.encoder_action_with_modifier(&[0xb0, 29, 63], true),
            (true, Some(EncoderAction::Up), true)
        );
        assert_eq!(
            loaded.encoder_action_with_modifier(&[0xb0, 29, 65], true),
            (true, Some(EncoderAction::Down), true)
        );
        assert_eq!(
            loaded.encoder_action_with_modifier(&[0xb0, 29, 65], false),
            (true, None, false),
            "the Shift-only CC stays consumed after release"
        );
        assert_eq!(
            loaded.encoder_action_with_modifier(&[0xb0, 114, 65], false),
            (true, Some(EncoderAction::Down), false)
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn page_cycle_chord_latches_once_and_reuses_an_absolute_control() {
        let c = PadConfig {
            controls: HashMap::from([(10, 1)]),
            page_cycle_modifier: Some(ControllerButton::Cc { channel: 0, cc: 27 }),
            page_cycle_trigger: Some(ControllerButton::Cc { channel: 0, cc: 10 }),
            ..PadConfig::default()
        };
        c.validate().unwrap();
        let mut state = PageCycleChordState::default();
        assert_eq!(
            c.page_cycle_chord_action(&[0xb0, 10, 20], &mut state),
            (false, None)
        );
        assert_eq!(
            c.page_cycle_chord_action(&[0xb0, 27, 127], &mut state),
            (true, None)
        );
        assert_eq!(
            c.page_cycle_chord_action(&[0xb0, 10, 21], &mut state),
            (true, Some((PadAction::CyclePage, true)))
        );
        assert_eq!(
            c.page_cycle_chord_action(&[0xb0, 10, 22], &mut state),
            (true, None)
        );
        assert_eq!(
            c.page_cycle_chord_action(&[0xb0, 27, 0], &mut state),
            (true, None)
        );
        c.page_cycle_chord_action(&[0xb0, 27, 127], &mut state);
        assert_eq!(
            c.page_cycle_chord_action(&[0xb0, 10, 23], &mut state),
            (true, Some((PadAction::CyclePage, true)))
        );
    }
    #[test]
    fn older_controller_profile_keeps_unspecified_encoder_controls_unmapped() {
        let path =
            std::env::temp_dir().join(format!("shsynth-controller-{}.conf", std::process::id()));
        fs::write(&path, "input=AudioBox USB 96\ncc.86=74\npad.36=arp\n").unwrap();
        let config = PadConfig::load(&path).unwrap();
        assert_eq!(config.input_match.as_deref(), Some("AudioBox USB 96"));
        assert_eq!(config.controls, HashMap::from([(86, 1)]));
        assert_eq!(config.encoder_relative_cc, None);
        assert_eq!(config.encoder_press_cc, None);
        assert_eq!(config.layout, ControllerLayout::Eight);
        assert_eq!(PadAction::Arp.menu_input(), MenuInput::SelectPage(0));
        assert_eq!(PadAction::TapTempo.menu_input(), MenuInput::ActivateItem(3));
        let _ = fs::remove_file(path);
    }
    #[test]
    fn legacy_transport_names_keep_the_conventional_soft_button_positions() {
        assert_eq!(PadAction::Stop.menu_input(), MenuInput::ActivateItem(0));
        assert_eq!(PadAction::Play.menu_input(), MenuInput::ActivateItem(1));
        assert_eq!(PadAction::Rec.menu_input(), MenuInput::ActivateItem(2));
        assert_eq!(PadAction::TapTempo.menu_input(), MenuInput::ActivateItem(3));
    }
    #[test]
    fn five_and_four_button_profiles_are_configurable_without_device_constants() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-controller-layout-{}.conf",
            std::process::id()
        ));
        fs::write(
            &path,
            "menu.layout=5\nencoder.relative_cc=12\nencoder.press_cc=13\npad.60=page-cycle\npad.61=item-1\npad.62=item-2\npad.63=item-3\npad.64=item-4\n",
        )
        .unwrap();
        let config = PadConfig::load(&path).unwrap();
        assert_eq!(config.layout, ControllerLayout::Five);
        assert_eq!(
            config.pads[&60].menu_input_for(config.layout),
            MenuInput::CyclePage
        );
        assert_eq!(
            config.pads[&64].menu_input_for(config.layout),
            MenuInput::ActivateItem(3)
        );
        let _ = fs::remove_file(path);
    }
    #[test]
    fn qualified_and_legacy_unqualified_commands_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-controller-qualified-{}.conf",
            std::process::id()
        ));
        fs::write(
            &path,
            "pad.10.36=page-1\npad.37=page-2\nbutton.cc.10.44=item-1\nbutton.cc.45=item-2\npage_cycle.modifier=cc.1.27\npage_cycle.trigger=cc.1.10\n",
        )
        .unwrap();
        let config = PadConfig::load(&path).unwrap();
        assert_eq!(config.pad_channels, HashMap::from([(36, 9)]));
        assert_eq!(config.cc_button_channels, HashMap::from([(44, 9)]));
        config.save(&path).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("pad.1=note.10.36"));
        assert!(saved.contains("pad.5=cc.10.44"));
        assert!(!saved.contains("page-1"));
        assert!(!saved.contains("item-1"));
        let loaded = PadConfig::load(&path).unwrap();
        assert_eq!(loaded.pad_channels, config.pad_channels);
        assert_eq!(loaded.cc_button_channels, config.cc_button_channels);
        assert_eq!(loaded.page_cycle_modifier, config.page_cycle_modifier);
        assert_eq!(loaded.page_cycle_trigger, config.page_cycle_trigger);
        assert!(loaded.route(&[0x90, 37, 100]).0);
        assert!(loaded.route(&[0x9f, 37, 100]).0);
        let _ = fs::remove_file(path);
    }
    #[test]
    fn positional_pots_and_pads_round_trip_without_parameter_or_action_names() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-controller-positional-{}.conf",
            std::process::id()
        ));
        fs::write(
            &path,
            "menu.layout=8\npot.1=74\npot.12=17\npad.1=note.10.36\npad.8=cc.10.43\n",
        )
        .unwrap();
        let config = PadConfig::load(&path).unwrap();
        assert_eq!(config.controls, HashMap::from([(74, 1), (17, 12)]));
        assert_eq!(config.pads[&36], PadAction::Pad1);
        assert_eq!(config.cc_buttons[&43], PadAction::Pad8);
        config.save(&path).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("pot.1=74"));
        assert!(saved.contains("pot.12=17"));
        assert!(saved.contains("pad.1=note.10.36"));
        assert!(saved.contains("pad.8=cc.10.43"));
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

    #[test]
    fn reversed_encoder_cc_buttons_and_note_press_are_supported() {
        let c = PadConfig {
            cc_buttons: HashMap::from([(44, PadAction::Item1)]),
            encoder_relative_cc: Some(28),
            encoder_relative_reverse: true,
            encoder_press_note: Some(99),
            ..PadConfig::default()
        };
        assert_eq!(
            c.encoder_action(&[0xb0, 28, 1]),
            (true, Some(EncoderAction::Down))
        );
        assert_eq!(
            c.action_state(&[0xb0, 44, 127]),
            Some((PadAction::Item1, true))
        );
        assert_eq!(
            c.encoder_note_action(&[0x90, 99, 100]),
            (true, Some(EncoderAction::Select))
        );
    }

    #[test]
    fn controller_numbers_are_limited_to_seven_bit_midi_values() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-controller-range-{}.conf",
            std::process::id()
        ));
        for text in [
            "cc.128=74\n",
            "button.cc.128=item-1\n",
            "pad.128=item-1\n",
            "encoder.relative_cc=128\n",
            "encoder.press_cc=128\n",
            "encoder.press_note=128\n",
            "lock.cc=128\n",
            "pad.0.36=item-1\n",
            "pad.17.36=item-1\n",
            "button.cc.17.44=item-1\n",
            "pot.1=74\ncc.71=71\n",
            "pad.1=note.10.36\npad.37=page-2\n",
        ] {
            fs::write(&path, text).unwrap();
            assert!(PadConfig::load(&path).is_err(), "accepted {text:?}");
        }
        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_rejects_conflicting_cli_style_mutations() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-controller-conflict-{}.conf",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let mut config = PadConfig {
            encoder_press_note: Some(36),
            pads: HashMap::from([(36, PadAction::Item1)]),
            ..PadConfig::default()
        };
        assert!(config.save(&path).is_err());

        config = PadConfig {
            encoder_relative_cc: Some(28),
            controls: HashMap::from([(28, 1)]),
            ..PadConfig::default()
        };
        assert!(config.save(&path).is_err());

        config = PadConfig {
            pads: HashMap::from([(36, PadAction::Item1)]),
            pad_channels: HashMap::from([(37, 9)]),
            ..PadConfig::default()
        };
        assert!(config.save(&path).is_err());

        config = PadConfig {
            cc_buttons: HashMap::from([(44, PadAction::Item1)]),
            cc_button_channels: HashMap::from([(44, 16)]),
            ..PadConfig::default()
        };
        assert!(config.save(&path).is_err());

        config = PadConfig {
            encoder_press_cc: Some(118),
            encoder_press_note: Some(99),
            ..PadConfig::default()
        };
        assert!(config.save(&path).is_err());

        config = PadConfig {
            encoder_modified_relative_cc: Some(29),
            ..PadConfig::default()
        };
        assert!(config.save(&path).is_err());

        config = PadConfig {
            encoder_modified_relative_reverse: true,
            ..PadConfig::default()
        };
        assert!(config.save(&path).is_err());

        config = PadConfig {
            input_match: Some("controller\nmenu.layout=8".into()),
            ..PadConfig::default()
        };
        assert!(config.save(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn unmapped_input_drops_an_old_controller_profile() {
        let old = PadConfig {
            input_match: Some("Old controller".into()),
            pads: HashMap::from([(36, PadAction::Page1)]),
            controls: HashMap::from([(74, 1)]),
            encoder_relative_cc: Some(28),
            ..PadConfig::default()
        };
        assert!(!old.pads.is_empty());

        let selected = PadConfig::unmapped("Unknown controller");
        assert_eq!(selected.input_match.as_deref(), Some("Unknown controller"));
        assert!(selected.pads.is_empty());
        assert!(selected.cc_buttons.is_empty());
        assert!(selected.controls.is_empty());
        assert_eq!(selected.encoder_relative_cc, None);
        assert_eq!(selected.encoder_press_cc, None);
        assert_eq!(selected.encoder_press_note, None);
        assert_eq!(selected.lock_cc, None);
    }

    #[test]
    fn controller_names_can_contain_hash_characters() {
        let path = std::env::temp_dir().join(format!(
            "shsynth-controller-hash-{}.conf",
            std::process::id()
        ));
        fs::write(&path, "# comment\ninput=Controller #1\n").unwrap();
        let config = PadConfig::load(&path).unwrap();
        assert_eq!(config.input_match.as_deref(), Some("Controller #1"));
        let _ = fs::remove_file(path);
    }
}
