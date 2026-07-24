//! Reusable master-encoder overlays which leave their caller workspace alive.
//!
//! The overlay owns transient navigation and edit state only. Project, engine,
//! transport, recorder, and persistence ownership remains with the caller.

use crate::navigation::{self, Action, MenuContext, Screen};
use crate::sequencer::Page;
use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayKind {
    TrackerPage,
    TrackerPattern,
    TrackerSong,
    TrackerRoute,
    TrackerPatternLength,
    TrackerNoteLength,
    TrackerAdvance,
    LoopLibrary,
    MixEffects,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseBehavior {
    /// Every overlay close path discards unconfirmed transient state. A
    /// workflow must use its explicit confirmation path to update its owner.
    CancelDraft,
}

impl OverlayKind {
    pub const fn from_action(action: Action) -> Option<Self> {
        match action {
            Action::OpenPageOverlay => Some(Self::TrackerPage),
            Action::OpenPatternOverlay => Some(Self::TrackerPattern),
            Action::OpenSongOverlay => Some(Self::TrackerSong),
            Action::OpenRouteOverlay => Some(Self::TrackerRoute),
            Action::OpenPatternLengthOverlay => Some(Self::TrackerPatternLength),
            Action::OpenNoteLengthOverlay => Some(Self::TrackerNoteLength),
            Action::OpenTrackerAdvanceOverlay => Some(Self::TrackerAdvance),
            Action::LoopImport | Action::OpenLoopLibrary => Some(Self::LoopLibrary),
            Action::OpenEffectsOverlay => Some(Self::MixEffects),
            _ => None,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::TrackerPage => "PAGE NAVIGATION",
            Self::TrackerPattern => "PATTERN NAVIGATION",
            Self::TrackerSong => "SONG NAVIGATION",
            Self::TrackerRoute => "PAGE ROUTING",
            Self::TrackerPatternLength => "PATTERN LENGTH",
            Self::TrackerNoteLength => "NOTE LENGTH",
            Self::TrackerAdvance => "EDIT ADD",
            Self::LoopLibrary => "LOOP BROWSER",
            Self::MixEffects => "EFFECTS ROUTING",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverlayLauncher {
    pub action: Action,
    pub label: &'static str,
    pub page: usize,
    pub item: usize,
}

impl OverlayLauncher {
    /// Resolve the launcher from the canonical controller table so its label,
    /// physical position, and dispatch action cannot drift apart.
    pub fn resolve(caller: Screen, context: MenuContext, action: Action) -> Option<Self> {
        navigation::pages(caller, context)
            .iter()
            .enumerate()
            .find_map(|(page_index, page)| {
                page.slots.iter().enumerate().find_map(|(item, slot)| {
                    (slot.dispatch() == Some(action)).then_some(Self {
                        action,
                        label: slot.label,
                        page: page_index,
                        item,
                    })
                })
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteField {
    Target,
    Engine,
    Instrument,
    MidiOutput,
    DeviceProfile,
    Channel(usize),
    BankMsb(usize),
    BankLsb(usize),
    Program(usize),
}

impl RouteField {
    pub const ROWS: usize = 22;

    pub const fn from_row(row: usize) -> Option<Self> {
        if row == 0 {
            return Some(Self::Target);
        }
        if row == 1 {
            return Some(Self::Engine);
        }
        if row == 2 {
            return Some(Self::Instrument);
        }
        if row == 3 {
            return Some(Self::MidiOutput);
        }
        if row == 4 {
            return Some(Self::DeviceProfile);
        }
        if row >= 21 {
            return None;
        }
        let column = (row - 5) / 4;
        match (row - 5) % 4 {
            0 => Some(Self::Channel(column)),
            1 => Some(Self::BankMsb(column)),
            2 => Some(Self::BankLsb(column)),
            _ => Some(Self::Program(column)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteDraft {
    pub pattern: u16,
    pub page_index: usize,
    pub original: Page,
    pub page: Page,
    /// Restores only the current field when Back is pressed during editing.
    pub field_original: Option<Page>,
}

impl RouteDraft {
    pub fn new(pattern: u16, page_index: usize, page: Page) -> Self {
        Self {
            pattern,
            page_index,
            original: page.clone(),
            page,
            field_original: None,
        }
    }

    pub fn dirty(&self) -> bool {
        self.page != self.original
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverlayDraft {
    None,
    Route(Box<RouteDraft>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverlayState {
    pub kind: OverlayKind,
    pub caller: Screen,
    pub title: &'static str,
    pub launcher: OverlayLauncher,
    pub selection: usize,
    pub scroll: usize,
    pub active_field: Option<RouteField>,
    pub draft: OverlayDraft,
    pub close_behavior: CloseBehavior,
    pub caller_menu_page: usize,
    pub caller_page_select_mode: bool,
}

impl OverlayState {
    pub fn new(
        kind: OverlayKind,
        caller: Screen,
        launcher: OverlayLauncher,
        selection: usize,
        draft: OverlayDraft,
        caller_menu_page: usize,
        caller_page_select_mode: bool,
    ) -> Self {
        Self {
            kind,
            caller,
            title: kind.title(),
            launcher,
            selection,
            scroll: 0,
            active_field: None,
            draft,
            close_behavior: CloseBehavior::CancelDraft,
            caller_menu_page,
            caller_page_select_mode,
        }
    }

    pub fn route(&self) -> Option<&RouteDraft> {
        match &self.draft {
            OverlayDraft::Route(route) => Some(route),
            OverlayDraft::None => None,
        }
    }

    pub fn route_mut(&mut self) -> Option<&mut RouteDraft> {
        match &mut self.draft {
            OverlayDraft::Route(route) => Some(route),
            OverlayDraft::None => None,
        }
    }

    /// Controller actions that remain available while this overlay owns
    /// transient selection. The launcher's original position is preserved;
    /// Loop Browser adds contextual stop/play anchors and Song Navigation adds
    /// TAP at the tempo anchor.
    pub fn controller_action(&self, item: usize) -> Option<(&'static str, Action)> {
        if self.launcher.item == item {
            return Some((self.launcher.label, self.launcher.action));
        }
        match (self.kind, item) {
            (OverlayKind::LoopLibrary, 0) => Some(("STOP", Action::LoopPreviewStop)),
            (OverlayKind::LoopLibrary, 1) => Some(("PLAY", Action::LoopPreview)),
            (OverlayKind::TrackerSong, 3) => Some(("TAP", Action::TapTempo)),
            _ => None,
        }
    }

    pub fn begin_route_field(&mut self, field: RouteField) {
        if self.active_field.is_some() {
            return;
        }
        if let Some(route) = self.route_mut() {
            route.field_original = Some(route.page.clone());
            self.active_field = Some(field);
        }
    }

    pub fn confirm_route_field(&mut self) {
        if let Some(route) = self.route_mut() {
            route.field_original = None;
        }
        self.active_field = None;
    }

    pub fn cancel_route_field(&mut self) -> bool {
        let Some(original) = self
            .route_mut()
            .and_then(|route| route.field_original.take())
        else {
            return false;
        };
        if let Some(route) = self.route_mut() {
            route.page = original;
        }
        self.active_field = None;
        true
    }

    pub fn move_selection(&mut self, direction: i8, rows: usize) {
        if rows == 0 {
            self.selection = 0;
            self.scroll = 0;
            return;
        }
        self.selection = self.selection.min(rows - 1);
        self.selection = match direction.cmp(&0) {
            std::cmp::Ordering::Less => (self.selection + rows - 1) % rows,
            std::cmp::Ordering::Greater => (self.selection + 1) % rows,
            std::cmp::Ordering::Equal => self.selection,
        };
    }

    pub fn keep_selection_visible(&mut self, visible_rows: usize, rows: usize) {
        let visible_rows = visible_rows.max(1);
        self.scroll = self.scroll.min(
            rows.saturating_sub(visible_rows)
                .min(rows.saturating_sub(1)),
        );
        if self.selection < self.scroll {
            self.scroll = self.selection;
        } else if self.selection >= self.scroll.saturating_add(visible_rows) {
            self.scroll = self.selection + 1 - visible_rows;
        }
    }
}

#[cfg(test)]
mod controller_tests {
    use super::*;

    fn overlay(kind: OverlayKind, launcher: OverlayLauncher) -> OverlayState {
        OverlayState::new(
            kind,
            Screen::TrackerLoop,
            launcher,
            0,
            OverlayDraft::None,
            3,
            false,
        )
    }

    #[test]
    fn loop_browser_keeps_launcher_and_exposes_stop_play_anchors() {
        let state = overlay(
            OverlayKind::LoopLibrary,
            OverlayLauncher {
                action: Action::OpenLoopLibrary,
                label: "LIBRARY",
                page: 3,
                item: 2,
            },
        );
        assert_eq!(
            state.controller_action(0),
            Some(("STOP", Action::LoopPreviewStop))
        );
        assert_eq!(
            state.controller_action(1),
            Some(("PLAY", Action::LoopPreview))
        );
        assert_eq!(
            state.controller_action(2),
            Some(("LIBRARY", Action::OpenLoopLibrary))
        );
        assert_eq!(state.controller_action(3), None);
    }

    #[test]
    fn song_overlay_keeps_launcher_and_puts_tap_at_position_eight() {
        let state = overlay(
            OverlayKind::TrackerSong,
            OverlayLauncher {
                action: Action::OpenSongOverlay,
                label: "SONG",
                page: 1,
                item: 2,
            },
        );
        assert_eq!(
            state.controller_action(2),
            Some(("SONG", Action::OpenSongOverlay))
        );
        assert_eq!(state.controller_action(3), Some(("TAP", Action::TapTempo)));
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OverlayGeometry {
    pub outer: Rect,
    pub inner: Rect,
}

/// Keep a one-cell reveal and one-cell border whenever the terminal has room.
/// Zero-sized rectangles are valid and let tiny-terminal callers skip drawing.
pub fn geometry(area: Rect) -> OverlayGeometry {
    let outer = if area.width >= 3 && area.height >= 3 {
        let width = area.width.saturating_sub(2).min(38);
        let height = area.height.saturating_sub(2).min(18);
        Rect::new(
            area.x.saturating_add(area.width.saturating_sub(width) / 2),
            area.y
                .saturating_add(area.height.saturating_sub(height) / 2),
            width,
            height,
        )
    } else {
        Rect::new(area.x, area.y, 0, 0)
    };
    let inner = if outer.width >= 2 && outer.height >= 2 {
        Rect::new(
            outer.x.saturating_add(1),
            outer.y.saturating_add(1),
            outer.width.saturating_sub(2),
            outer.height.saturating_sub(2),
        )
    } else {
        Rect::new(outer.x, outer.y, 0, 0)
    };
    OverlayGeometry { outer, inner }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_forty_by_twenty_geometry_has_one_cell_reveal() {
        let geometry = geometry(Rect::new(0, 0, 40, 20));
        assert_eq!(geometry.outer, Rect::new(1, 1, 38, 18));
        assert_eq!(geometry.inner, Rect::new(2, 2, 36, 16));
        assert_eq!(geometry.outer.right(), 39);
        assert_eq!(geometry.outer.bottom(), 19);
    }

    #[test]
    fn compact_geometry_clamps_without_underflow() {
        assert_eq!(geometry(Rect::new(7, 9, 2, 2)).outer.width, 0);
        let compact = geometry(Rect::new(0, 0, 12, 6));
        assert_eq!(compact.outer, Rect::new(1, 1, 10, 4));
        assert_eq!(compact.inner, Rect::new(2, 2, 8, 2));
    }

    #[test]
    fn larger_terminals_keep_the_fixed_overlay_size_centered() {
        let geometry = geometry(Rect::new(0, 0, 80, 24));
        assert_eq!(geometry.outer, Rect::new(21, 3, 38, 18));
        assert_eq!(geometry.inner, Rect::new(22, 4, 36, 16));
    }

    #[test]
    fn loop_library_uses_the_shared_overlay_kind() {
        assert_eq!(
            OverlayKind::from_action(Action::LoopImport),
            Some(OverlayKind::LoopLibrary)
        );
        assert_eq!(
            OverlayKind::from_action(Action::OpenLoopLibrary),
            Some(OverlayKind::LoopLibrary)
        );
        assert_eq!(OverlayKind::LoopLibrary.title(), "LOOP BROWSER");
    }

    #[test]
    fn field_cancel_restores_only_the_field_snapshot() {
        let page = Page::new("MIDI", 0, false, 0);
        let launcher = OverlayLauncher {
            action: Action::OpenRouteOverlay,
            label: "ROUTE",
            page: 2,
            item: 3,
        };
        let mut state = OverlayState::new(
            OverlayKind::TrackerRoute,
            Screen::Tracker,
            launcher,
            1,
            OverlayDraft::Route(Box::new(RouteDraft::new(0, 0, page))),
            2,
            false,
        );
        state.begin_route_field(RouteField::Channel(0));
        state.route_mut().unwrap().page.columns[0].channel = 7;
        assert!(state.cancel_route_field());
        assert_eq!(state.route().unwrap().page.columns[0].channel, 0);
        assert_eq!(state.active_field, None);
    }
}
