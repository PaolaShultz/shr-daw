//! Screen-specific four-page controller menus.
//!
//! Labels and dispatch actions deliberately live in the same table.  Physical
//! controller profiles select pages/items; they never encode screen actions.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Screen {
    Presets,
    Playback,
    Ideas,
    Tracker,
    TrackerFiles,
    TrackerPages,
    AudioRecorder,
}

impl Screen {
    pub const COUNT: usize = 7;
    #[cfg(test)]
    pub const ALL: [Self; 7] = [
        Self::Presets,
        Self::Playback,
        Self::Ideas,
        Self::Tracker,
        Self::TrackerFiles,
        Self::TrackerPages,
        Self::AudioRecorder,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Presets => 0,
            Self::Playback => 1,
            Self::Ideas => 2,
            Self::Tracker => 3,
            Self::TrackerFiles => 4,
            Self::TrackerPages => 5,
            Self::AudioRecorder => 6,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Presets => "PRESETS",
            Self::Playback => "PLAYBACK",
            Self::Ideas => "IDEAS",
            Self::Tracker => "FT2",
            Self::TrackerFiles => "FILES",
            Self::TrackerPages => "TRACKS",
            Self::AudioRecorder => "AUDIO",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Action {
    Noop,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    PreviousEngine,
    NextEngine,
    Activate,
    Back,
    Quit,
    StopAll,
    OpenPresets,
    OpenIdeas,
    OpenTracker,
    OpenTrackerFiles,
    OpenTrackerPages,
    OpenAudioRecorder,
    TapTempo,
    ResetParameters,
    BeginRecord,
    StopRecord,
    FinishSaveRecord,
    SaveNew,
    InspectIdea,
    DeleteIdea,
    LoadIdea,
    PlaybackRecording,
    StopPlayback,
    TrackerEdit,
    TrackerSkip,
    TrackerErase,
    TrackerNoteOff,
    OpenNoteEditor,
    NoteField,
    GateField,
    VelocityField,
    ProgramField,
    EffectField,
    EffectParameterField,
    NoteEditorClearField,
    NoteEditorPreviousField,
    NoteEditorNextField,
    NoteEditorDecrease,
    NoteEditorIncrease,
    NoteEditorConfirm,
    NoteEditorCancel,
    TrackerPlayCursor,
    TrackerPlayStart,
    TrackerStop,
    TrackerMute,
    TrackerPageMute,
    NextTrackerPage,
    PreviousTrack,
    NextTrack,
    PreviousProgram,
    NextProgram,
    TempoDown,
    TempoUp,
    SaveSong,
    LoadSong,
    PreviewSong,
    DeleteSong,
    NewPattern,
    ClonePattern,
    ClearPattern,
    ClearPatternNow,
    PreviousOrder,
    NextOrder,
    RepeatOrder,
    DeleteOrder,
    AddPage,
    EditPageTarget,
    EditPageChannel,
    ConfirmPageManager,
    SelectThreeFour,
    SelectFourFour,
    ConfirmPatternClear,
    AudioRecord,
    AudioStop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotState {
    Enabled,
    Disabled,
    Planned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MenuSlot {
    pub label: &'static str,
    pub action: Action,
    pub state: SlotState,
}

impl MenuSlot {
    pub const fn enabled(label: &'static str, action: Action) -> Self {
        Self {
            label,
            action,
            state: SlotState::Enabled,
        }
    }
    pub const fn disabled(label: &'static str) -> Self {
        Self {
            label,
            action: Action::Noop,
            state: SlotState::Disabled,
        }
    }
    pub const fn planned(label: &'static str) -> Self {
        Self {
            label,
            action: Action::Noop,
            state: SlotState::Planned,
        }
    }
    pub const fn dispatch(self) -> Option<Action> {
        match self.state {
            SlotState::Enabled => Some(self.action),
            SlotState::Disabled | SlotState::Planned => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MenuPage {
    pub label: &'static str,
    pub slots: [MenuSlot; 4],
}

const fn page(label: &'static str, slots: [MenuSlot; 4]) -> MenuPage {
    MenuPage { label, slots }
}
const fn on(label: &'static str, action: Action) -> MenuSlot {
    MenuSlot::enabled(label, action)
}
const fn off(label: &'static str) -> MenuSlot {
    MenuSlot::disabled(label)
}
const fn future(label: &'static str) -> MenuSlot {
    MenuSlot::planned(label)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MenuContext {
    #[default]
    Normal,
    TrackerEdit,
    TrackerNoteEdit,
    PageTarget,
    PageChannel,
    PatternClear,
}

const PRESETS: [MenuPage; 4] = [
    page(
        "BROWSE",
        [
            on("UP", Action::Up),
            on("DOWN", Action::Down),
            on("PG UP", Action::PageUp),
            on("PG DOWN", Action::PageDown),
        ],
    ),
    page(
        "ENGINE",
        [
            on("ENGINE-", Action::PreviousEngine),
            on("ENGINE+", Action::NextEngine),
            on("FIRST", Action::Home),
            on("LAST", Action::End),
        ],
    ),
    page(
        "OPEN",
        [
            on("LOAD", Action::Activate),
            on("IDEAS", Action::OpenIdeas),
            on("FT2", Action::OpenTracker),
            on("AUDIO", Action::OpenAudioRecorder),
        ],
    ),
    page(
        "SAFETY",
        [
            on("PANIC", Action::StopAll),
            on("EXIT", Action::Quit),
            off("--"),
            off("--"),
        ],
    ),
];
const PLAYBACK: [MenuPage; 4] = [
    page(
        "IDEA",
        [
            on("RECORD", Action::BeginRecord),
            on("STOP REC", Action::StopRecord),
            on("PLAY TAKE", Action::PlaybackRecording),
            on("SAVE IDEA", Action::SaveNew),
        ],
    ),
    page(
        "SOUND",
        [
            on("RESET", Action::ResetParameters),
            on("PRESETS", Action::OpenPresets),
            on("IDEAS", Action::OpenIdeas),
            future("ARP"),
        ],
    ),
    page(
        "OPEN",
        [
            on("FT2", Action::OpenTracker),
            on("AUDIO", Action::OpenAudioRecorder),
            on("TAP", Action::TapTempo),
            on("BACK", Action::Back),
        ],
    ),
    page(
        "SAFETY",
        [
            on("STOP TAKE", Action::StopPlayback),
            on("FINISH+SAVE", Action::FinishSaveRecord),
            on("PANIC", Action::StopAll),
            off("--"),
        ],
    ),
];
const IDEAS: [MenuPage; 4] = [
    page(
        "BROWSE",
        [
            on("UP", Action::Up),
            on("DOWN", Action::Down),
            on("FIRST", Action::Home),
            on("LAST", Action::End),
        ],
    ),
    page(
        "IDEA",
        [
            on("INSPECT", Action::InspectIdea),
            on("LOAD", Action::LoadIdea),
            on("PLAY", Action::PlaybackRecording),
            on("DELETE", Action::DeleteIdea),
        ],
    ),
    page(
        "CAPTURE",
        [
            on("RECORD", Action::BeginRecord),
            on("STOP REC", Action::StopRecord),
            on("SAVE NEW", Action::SaveNew),
            on("PLAYBACK", Action::OpenPresets),
        ],
    ),
    page(
        "OPEN",
        [
            on("BACK", Action::Back),
            on("FT2", Action::OpenTracker),
            on("AUDIO", Action::OpenAudioRecorder),
            on("PANIC", Action::StopAll),
        ],
    ),
];
const TRACKER: [MenuPage; 4] = [
    page(
        "CURSOR",
        [
            on("ROW-", Action::Up),
            on("ROW+", Action::Down),
            on("LANE-", Action::PreviousTrack),
            on("LANE+", Action::NextTrack),
        ],
    ),
    page(
        "TRANSP",
        [
            on("PLAY HERE", Action::TrackerPlayCursor),
            on("PLAY START", Action::TrackerPlayStart),
            on("STOP/BACK", Action::TrackerStop),
            on("CELL EDIT", Action::OpenNoteEditor),
        ],
    ),
    page(
        "MANAGE",
        [
            on("PAGES", Action::OpenTrackerPages),
            on("FILES", Action::OpenTrackerFiles),
            on("MUTE LANE", Action::TrackerMute),
            on("TAP", Action::TapTempo),
        ],
    ),
    page(
        "ADJUST",
        [
            on("PROG-", Action::PreviousProgram),
            on("PROG+", Action::NextProgram),
            on("TEMPO-", Action::TempoDown),
            on("TEMPO+", Action::TempoUp),
        ],
    ),
];
const TRACKER_EDIT: [MenuPage; 4] = [
    TRACKER[0],
    page(
        "ENTRY",
        [
            on("BLANK", Action::TrackerSkip),
            on("ERASE", Action::TrackerErase),
            on("NOTE OFF", Action::TrackerNoteOff),
            on("EDIT DONE", Action::TrackerEdit),
        ],
    ),
    page(
        "TRANSP",
        [
            on("PLAY HERE", Action::TrackerPlayCursor),
            on("PLAY START", Action::TrackerPlayStart),
            on("STOP", Action::TrackerStop),
            on("NEXT PAGE", Action::NextTrackerPage),
        ],
    ),
    TRACKER[3],
];
const TRACKER_NOTE_EDIT: [MenuPage; 4] = [
    page(
        "FIELDS",
        [
            on("NOTE", Action::NoteField),
            on("GATE", Action::GateField),
            on("VELOCITY", Action::VelocityField),
            on("PROGRAM", Action::ProgramField),
        ],
    ),
    page(
        "EFFECT",
        [
            on("EFFECT", Action::EffectField),
            on("PARAM", Action::EffectParameterField),
            on("CLEAR FLD", Action::NoteEditorClearField),
            on("STEP EDIT", Action::TrackerEdit),
        ],
    ),
    page(
        "ADJUST",
        [
            on("FIELD-", Action::NoteEditorPreviousField),
            on("FIELD+", Action::NoteEditorNextField),
            on("VALUE-", Action::NoteEditorDecrease),
            on("VALUE+", Action::NoteEditorIncrease),
        ],
    ),
    page(
        "FINISH",
        [
            on("CONFIRM", Action::NoteEditorConfirm),
            on("CANCEL/BACK", Action::NoteEditorCancel),
            on("STOP", Action::TrackerStop),
            on("PANIC", Action::StopAll),
        ],
    ),
];
const FILES: [MenuPage; 4] = [
    page(
        "BROWSE",
        [
            on("UP", Action::Up),
            on("DOWN", Action::Down),
            on("LOAD", Action::LoadSong),
            on("BACK", Action::Back),
        ],
    ),
    page(
        "SONG",
        [
            on("SAVE", Action::SaveSong),
            on("PREVIEW", Action::PreviewSong),
            on("DELETE", Action::DeleteSong),
            on("PANIC", Action::StopAll),
        ],
    ),
    page(
        "PATTERN",
        [
            on("NEW", Action::NewPattern),
            on("CLONE", Action::ClonePattern),
            on("CLEAR", Action::ClearPattern),
            future("WAV LOOP"),
        ],
    ),
    page(
        "ORDER",
        [
            on("ORDER-", Action::PreviousOrder),
            on("ORDER+", Action::NextOrder),
            on("REPEAT", Action::RepeatOrder),
            on("REMOVE", Action::DeleteOrder),
        ],
    ),
];
const PAGES: [MenuPage; 4] = [
    page(
        "PAGES",
        [
            on("PAGE-", Action::PreviousTrack),
            on("PAGE+", Action::NextTrack),
            on("ADD", Action::AddPage),
            on("CANCEL", Action::Back),
        ],
    ),
    page(
        "ROUTE",
        [
            on("TARGET", Action::EditPageTarget),
            on("CHANNEL", Action::EditPageChannel),
            on("DONE", Action::ConfirmPageManager),
            on("FILES", Action::OpenTrackerFiles),
        ],
    ),
    page(
        "STATUS",
        [
            on("MUTE PAGE", Action::TrackerPageMute),
            off("--"),
            off("--"),
            off("--"),
        ],
    ),
    page("FUTURE", [off("--"), off("--"), off("--"), off("--")]),
];
const PAGE_FIELD: [MenuPage; 4] = [
    page(
        "EDIT",
        [
            on("PREVIOUS", Action::Up),
            on("NEXT", Action::Down),
            on("CONFIRM", Action::ConfirmPageManager),
            on("CANCEL", Action::Back),
        ],
    ),
    page(
        "LOCKED",
        [off("FIELD MODE"), off("--"), off("--"), off("--")],
    ),
    page(
        "LOCKED",
        [off("FIELD MODE"), off("--"), off("--"), off("--")],
    ),
    page(
        "LOCKED",
        [off("FIELD MODE"), off("--"), off("--"), off("--")],
    ),
];
const PATTERN_CLEAR: [MenuPage; 4] = [
    page(
        "METER",
        [
            on("3/4", Action::SelectThreeFour),
            on("4/4", Action::SelectFourFour),
            on("CONFIRM", Action::ConfirmPatternClear),
            on("CANCEL", Action::Back),
        ],
    ),
    page(
        "CURRENT",
        [
            on("CLEAR SIZE", Action::ClearPatternNow),
            off("--"),
            off("--"),
            off("--"),
        ],
    ),
    page("LOCKED", [off("CONFIRM"), off("--"), off("--"), off("--")]),
    page("LOCKED", [off("CONFIRM"), off("--"), off("--"), off("--")]),
];
const AUDIO: [MenuPage; 4] = [
    page(
        "RECORD",
        [
            on("RECORD", Action::AudioRecord),
            on("STOP REC", Action::AudioStop),
            on("BACK", Action::Back),
            on("PANIC", Action::StopAll),
        ],
    ),
    page(
        "OPEN",
        [
            on("PRESETS", Action::OpenPresets),
            on("IDEAS", Action::OpenIdeas),
            on("FT2", Action::OpenTracker),
            off("--"),
        ],
    ),
    page(
        "STATUS",
        [off("24-BIT WAV"), off("STEREO"), off("--"), off("--")],
    ),
    page("FUTURE", [off("--"), off("--"), off("--"), off("--")]),
];

pub fn pages(screen: Screen, context: MenuContext) -> &'static [MenuPage; 4] {
    match (screen, context) {
        (Screen::Presets, _) => &PRESETS,
        (Screen::Playback, _) => &PLAYBACK,
        (Screen::Ideas, _) => &IDEAS,
        (Screen::Tracker, MenuContext::TrackerNoteEdit) => &TRACKER_NOTE_EDIT,
        (Screen::Tracker, MenuContext::TrackerEdit) => &TRACKER_EDIT,
        (Screen::Tracker, _) => &TRACKER,
        (Screen::TrackerFiles, MenuContext::PatternClear) => &PATTERN_CLEAR,
        (Screen::TrackerFiles, _) => &FILES,
        (Screen::TrackerPages, MenuContext::PageTarget | MenuContext::PageChannel) => &PAGE_FIELD,
        (Screen::TrackerPages, _) => &PAGES,
        (Screen::AudioRecorder, _) => &AUDIO,
    }
}

pub fn slot(screen: Screen, context: MenuContext, page: usize, item: usize) -> Option<MenuSlot> {
    pages(screen, context).get(page)?.slots.get(item).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_screen_and_context_has_exactly_four_pages_of_four_slots() {
        for screen in Screen::ALL {
            for context in [
                MenuContext::Normal,
                MenuContext::TrackerEdit,
                MenuContext::TrackerNoteEdit,
                MenuContext::PageTarget,
                MenuContext::PageChannel,
                MenuContext::PatternClear,
            ] {
                let menu = pages(screen, context);
                assert_eq!(menu.len(), 4);
                assert!(menu.iter().all(|page| page.slots.len() == 4));
                assert!(menu.iter().all(|page| !page.label.is_empty()));
                assert!(menu
                    .iter()
                    .flat_map(|page| page.slots)
                    .all(|slot| !slot.label.is_empty()));
            }
        }
    }

    #[test]
    fn planned_slots_never_dispatch() {
        let arp = slot(Screen::Playback, MenuContext::Normal, 1, 3).unwrap();
        let wav = slot(Screen::TrackerFiles, MenuContext::Normal, 2, 3).unwrap();
        assert_eq!(
            (arp.label, arp.state, arp.dispatch()),
            ("ARP", SlotState::Planned, None)
        );
        assert_eq!(
            (wav.label, wav.state, wav.dispatch()),
            ("WAV LOOP", SlotState::Planned, None)
        );
    }

    #[test]
    fn note_editor_is_exactly_four_pages_and_every_item_dispatches() {
        let menu = pages(Screen::Tracker, MenuContext::TrackerNoteEdit);
        assert_eq!(menu.len(), 4);
        assert!(menu.iter().all(|page| page.slots.len() == 4));
        assert!(menu
            .iter()
            .flat_map(|page| page.slots)
            .all(|slot| slot.dispatch().is_some()));
    }

    #[test]
    fn contextual_menus_replace_ambiguous_actions() {
        assert_eq!(
            slot(Screen::Tracker, MenuContext::TrackerNoteEdit, 3, 0)
                .unwrap()
                .action,
            Action::NoteEditorConfirm
        );
        assert_eq!(
            slot(Screen::Tracker, MenuContext::TrackerEdit, 1, 1)
                .unwrap()
                .action,
            Action::TrackerErase
        );
        assert_eq!(
            slot(Screen::TrackerPages, MenuContext::PageTarget, 0, 2)
                .unwrap()
                .action,
            Action::ConfirmPageManager
        );
        assert_eq!(
            slot(Screen::TrackerFiles, MenuContext::PatternClear, 0, 3)
                .unwrap()
                .action,
            Action::Back
        );
    }

    #[test]
    fn inventoried_controller_workflow_actions_are_all_reachable() {
        let contexts = [
            (Screen::Presets, MenuContext::Normal),
            (Screen::Playback, MenuContext::Normal),
            (Screen::Ideas, MenuContext::Normal),
            (Screen::Tracker, MenuContext::Normal),
            (Screen::Tracker, MenuContext::TrackerEdit),
            (Screen::Tracker, MenuContext::TrackerNoteEdit),
            (Screen::TrackerFiles, MenuContext::Normal),
            (Screen::TrackerFiles, MenuContext::PatternClear),
            (Screen::TrackerPages, MenuContext::Normal),
            (Screen::TrackerPages, MenuContext::PageTarget),
            (Screen::TrackerPages, MenuContext::PageChannel),
            (Screen::AudioRecorder, MenuContext::Normal),
        ];
        let reachable = contexts
            .into_iter()
            .flat_map(|(screen, context)| pages(screen, context))
            .flat_map(|page| page.slots)
            .filter_map(MenuSlot::dispatch)
            .collect::<HashSet<_>>();
        let inventory = [
            Action::Up,
            Action::Down,
            Action::PageUp,
            Action::PageDown,
            Action::Home,
            Action::End,
            Action::PreviousEngine,
            Action::NextEngine,
            Action::Activate,
            Action::Back,
            Action::Quit,
            Action::StopAll,
            Action::OpenPresets,
            Action::OpenIdeas,
            Action::OpenTracker,
            Action::OpenTrackerFiles,
            Action::OpenTrackerPages,
            Action::OpenAudioRecorder,
            Action::TapTempo,
            Action::ResetParameters,
            Action::BeginRecord,
            Action::StopRecord,
            Action::FinishSaveRecord,
            Action::SaveNew,
            Action::InspectIdea,
            Action::DeleteIdea,
            Action::LoadIdea,
            Action::PlaybackRecording,
            Action::StopPlayback,
            Action::TrackerEdit,
            Action::TrackerSkip,
            Action::TrackerErase,
            Action::TrackerNoteOff,
            Action::OpenNoteEditor,
            Action::NoteField,
            Action::GateField,
            Action::VelocityField,
            Action::ProgramField,
            Action::EffectField,
            Action::EffectParameterField,
            Action::NoteEditorClearField,
            Action::NoteEditorPreviousField,
            Action::NoteEditorNextField,
            Action::NoteEditorDecrease,
            Action::NoteEditorIncrease,
            Action::NoteEditorConfirm,
            Action::NoteEditorCancel,
            Action::TrackerPlayCursor,
            Action::TrackerPlayStart,
            Action::TrackerStop,
            Action::TrackerMute,
            Action::TrackerPageMute,
            Action::NextTrackerPage,
            Action::PreviousTrack,
            Action::NextTrack,
            Action::PreviousProgram,
            Action::NextProgram,
            Action::TempoDown,
            Action::TempoUp,
            Action::SaveSong,
            Action::LoadSong,
            Action::PreviewSong,
            Action::DeleteSong,
            Action::NewPattern,
            Action::ClonePattern,
            Action::ClearPattern,
            Action::ClearPatternNow,
            Action::PreviousOrder,
            Action::NextOrder,
            Action::RepeatOrder,
            Action::DeleteOrder,
            Action::AddPage,
            Action::EditPageTarget,
            Action::EditPageChannel,
            Action::ConfirmPageManager,
            Action::SelectThreeFour,
            Action::SelectFourFour,
            Action::ConfirmPatternClear,
            Action::AudioRecord,
            Action::AudioStop,
        ];
        for action in inventory {
            assert!(
                reachable.contains(&action),
                "missing controller action {action:?}"
            );
        }
    }
}
