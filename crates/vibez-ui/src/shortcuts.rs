//! Customizable keyboard shortcuts.
//!
//! Every bindable action has one or more default key combos (matching the
//! historically hardcoded bindings). Users can rebind any action in
//! Settings → Shortcuts; the overrides persist in ui.json and the global
//! key handler routes through [`Shortcuts::action_for`] first, so a rebind
//! takes effect immediately.

use std::collections::HashMap;

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use serde::{Deserialize, Serialize};

use crate::domains::arrangement::ArrangementMsg;
use crate::domains::piano_roll::PianoRollMsg;
use crate::domains::project::ProjectMsg;
use crate::domains::transport::TransportMsg;
use crate::domains::view::ViewMsg;
use crate::message::Message;

/// The command modifier: Cmd on macOS, Ctrl everywhere else.
pub fn command_held(modifiers: Modifiers, on_macos: bool) -> bool {
    if on_macos {
        modifiers.logo()
    } else {
        modifiers.control()
    }
}

/// Key names for mouse-wheel bindings. Keyboard events never produce
/// these; they are built by [`KeyCombo::wheel`] and matched by
/// [`KeyCombo::matches_wheel`].
pub const WHEEL_UP: &str = "WheelUp";
pub const WHEEL_DOWN: &str = "WheelDown";

/// One key plus modifiers. `key` is a lowercase character or the `Debug`
/// name of a [`Named`] key ("Space", "Delete", "ArrowUp", ...). `ctrl`
/// means the platform command modifier (Cmd on macOS).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyCombo {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
}

impl KeyCombo {
    pub fn new(key: &str, ctrl: bool, shift: bool, alt: bool) -> Self {
        Self {
            key: key.to_string(),
            ctrl,
            shift,
            alt,
        }
    }

    /// Build a combo from a key event, or `None` for pure modifier presses
    /// (they change modifiers, they are not shortcuts).
    pub fn from_event(key: &Key, modifiers: Modifiers, on_macos: bool) -> Option<KeyCombo> {
        let key = match key {
            Key::Character(character) => character.to_lowercase(),
            Key::Named(named) => match named {
                Named::Control | Named::Shift | Named::Alt | Named::Super => return None,
                other => format!("{other:?}"),
            },
            _ => return None,
        };
        Some(KeyCombo {
            key,
            ctrl: command_held(modifiers, on_macos),
            shift: modifiers.shift(),
            alt: modifiers.alt(),
        })
    }

    /// Build a combo from a mouse-wheel turn: `direction_up` is wheel-up
    /// (zoom in) vs wheel-down, plus the modifiers held at the time.
    pub fn wheel(direction_up: bool, modifiers: Modifiers, on_macos: bool) -> KeyCombo {
        KeyCombo {
            key: if direction_up { WHEEL_UP } else { WHEEL_DOWN }.to_string(),
            ctrl: command_held(modifiers, on_macos),
            shift: modifiers.shift(),
            alt: modifiers.alt(),
        }
    }

    /// Whether this combo is exactly what the wheel turn carries, with the
    /// same command-modifier rules as [`Self::matches_event`].
    pub fn matches_wheel(&self, direction_up: bool, modifiers: Modifiers, on_macos: bool) -> bool {
        let expected = if direction_up { WHEEL_UP } else { WHEEL_DOWN };
        if self.key != expected
            || self.ctrl != command_held(modifiers, on_macos)
            || self.shift != modifiers.shift()
            || self.alt != modifiers.alt()
        {
            return false;
        }
        let other_command = if on_macos {
            modifiers.control()
        } else {
            modifiers.logo()
        };
        !other_command
    }

    /// Whether this combo is exactly what the event carries. The command
    /// modifier must match, and the *other* command modifier (Ctrl on a
    /// Mac, the Super key elsewhere) must not be held, so e.g. Ctrl+Z on a
    /// Mac does not also trigger the plain-Z binding.
    pub fn matches_event(&self, key: &Key, modifiers: Modifiers, on_macos: bool) -> bool {
        self.matches_event_with(key, modifiers, on_macos, false)
    }

    /// As [`Self::matches_event`], but a command-modified single-character
    /// combo without Shift also answers to the event's Shift (so
    /// Ctrl+Shift+S still saves). Bare (no-command) combos never do — Shift+Z
    /// stays unbound — and shift-sensitive combos (Ctrl+Shift+T, …) are
    /// matched exactly first, so this fallback never shadows them.
    fn matches_event_with(
        &self,
        key: &Key,
        modifiers: Modifiers,
        on_macos: bool,
        shift_loose: bool,
    ) -> bool {
        let Some(combo) = KeyCombo::from_event(key, modifiers, on_macos) else {
            return false;
        };
        let shift_matches = combo.shift == self.shift
            || (shift_loose && self.ctrl && !self.shift && self.key.chars().count() == 1);
        if combo.key != self.key
            || combo.ctrl != self.ctrl
            || combo.alt != self.alt
            || !shift_matches
        {
            return false;
        }
        let other_command = if on_macos {
            modifiers.control()
        } else {
            modifiers.logo()
        };
        !other_command
    }

    pub fn label(&self, on_macos: bool) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push(if on_macos { "Cmd" } else { "Ctrl" }.to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        parts.push(pretty_key(&self.key));
        parts.join("+")
    }
}

fn pretty_key(key: &str) -> String {
    match key {
        "Space" => "Space".to_string(),
        "Delete" => "Del".to_string(),
        "Backspace" => "Bksp".to_string(),
        "ArrowUp" => "↑".to_string(),
        "ArrowDown" => "↓".to_string(),
        "ArrowLeft" => "←".to_string(),
        "ArrowRight" => "→".to_string(),
        "Enter" => "Enter".to_string(),
        "Escape" => "Esc".to_string(),
        WHEEL_UP => "Wheel ↑".to_string(),
        WHEEL_DOWN => "Wheel ↓".to_string(),
        other => other.to_string(),
    }
}

/// A bindable action. Serialized kebab-case, so ui.json keys stay stable
/// and readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShortcutAction {
    Undo,
    Redo,
    Save,
    Copy,
    Cut,
    Paste,
    Duplicate,
    SelectAll,
    Delete,
    AddTrack,
    AddInstrumentTrack,
    MoveTrackUp,
    MoveTrackDown,
    SplitAtPlayhead,
    JoinClips,
    ToggleArrangementLoop,
    ToggleClipLoop,
    CreateClipFromSelection,
    ZoomToFit,
    ZoomToSelection,
    ZoomBack,
    TogglePlayback,
    TogglePianoRollEditMode,
    QuickAdd,
    OverviewZoomIn,
    OverviewZoomOut,
    ToggleMetronome,
    MetronomeLouder,
    MetronomeQuieter,
    MidiLearn,
    GroupTracks,
    UngroupTracks,
    FreezeTrack,
    BounceInPlace,
    RenameTrack,
    ShowTakeLanes,
    NextTake,
    PreviousTake,
    AddToGroup,
    RemoveFromGroup,
}

impl ShortcutAction {
    pub const ALL: &'static [ShortcutAction] = &[
        ShortcutAction::Undo,
        ShortcutAction::Redo,
        ShortcutAction::Save,
        ShortcutAction::Copy,
        ShortcutAction::Cut,
        ShortcutAction::Paste,
        ShortcutAction::Duplicate,
        ShortcutAction::SelectAll,
        ShortcutAction::Delete,
        ShortcutAction::AddTrack,
        ShortcutAction::AddInstrumentTrack,
        ShortcutAction::MoveTrackUp,
        ShortcutAction::MoveTrackDown,
        ShortcutAction::SplitAtPlayhead,
        ShortcutAction::JoinClips,
        ShortcutAction::ToggleArrangementLoop,
        ShortcutAction::ToggleClipLoop,
        ShortcutAction::CreateClipFromSelection,
        ShortcutAction::ZoomToFit,
        ShortcutAction::ZoomToSelection,
        ShortcutAction::ZoomBack,
        ShortcutAction::TogglePlayback,
        ShortcutAction::TogglePianoRollEditMode,
        ShortcutAction::QuickAdd,
        ShortcutAction::OverviewZoomIn,
        ShortcutAction::OverviewZoomOut,
        ShortcutAction::ToggleMetronome,
        ShortcutAction::MetronomeLouder,
        ShortcutAction::MetronomeQuieter,
        ShortcutAction::MidiLearn,
        ShortcutAction::GroupTracks,
        ShortcutAction::UngroupTracks,
        ShortcutAction::FreezeTrack,
        ShortcutAction::BounceInPlace,
        ShortcutAction::RenameTrack,
        ShortcutAction::ShowTakeLanes,
        ShortcutAction::NextTake,
        ShortcutAction::PreviousTake,
        ShortcutAction::AddToGroup,
        ShortcutAction::RemoveFromGroup,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            ShortcutAction::Undo => "undo",
            ShortcutAction::Redo => "redo",
            ShortcutAction::Save => "save",
            ShortcutAction::Copy => "copy",
            ShortcutAction::Cut => "cut",
            ShortcutAction::Paste => "paste",
            ShortcutAction::Duplicate => "duplicate",
            ShortcutAction::SelectAll => "select-all",
            ShortcutAction::Delete => "delete",
            ShortcutAction::AddTrack => "add-track",
            ShortcutAction::AddInstrumentTrack => "add-instrument-track",
            ShortcutAction::MoveTrackUp => "move-track-up",
            ShortcutAction::MoveTrackDown => "move-track-down",
            ShortcutAction::SplitAtPlayhead => "split-at-playhead",
            ShortcutAction::JoinClips => "join-clips",
            ShortcutAction::ToggleArrangementLoop => "toggle-arrangement-loop",
            ShortcutAction::ToggleClipLoop => "toggle-clip-loop",
            ShortcutAction::CreateClipFromSelection => "create-clip-from-selection",
            ShortcutAction::ZoomToFit => "zoom-to-fit",
            ShortcutAction::ZoomToSelection => "zoom-to-selection",
            ShortcutAction::ZoomBack => "zoom-back",
            ShortcutAction::TogglePlayback => "toggle-playback",
            ShortcutAction::TogglePianoRollEditMode => "toggle-piano-roll-edit-mode",
            ShortcutAction::QuickAdd => "quick-add",
            ShortcutAction::OverviewZoomIn => "overview-zoom-in",
            ShortcutAction::OverviewZoomOut => "overview-zoom-out",
            ShortcutAction::ToggleMetronome => "toggle-metronome",
            ShortcutAction::MetronomeLouder => "metronome-louder",
            ShortcutAction::MetronomeQuieter => "metronome-quieter",
            ShortcutAction::MidiLearn => "midi-learn",
            ShortcutAction::GroupTracks => "group-tracks",
            ShortcutAction::UngroupTracks => "ungroup-tracks",
            ShortcutAction::FreezeTrack => "freeze-track",
            ShortcutAction::BounceInPlace => "bounce-in-place",
            ShortcutAction::RenameTrack => "rename-track",
            ShortcutAction::ShowTakeLanes => "show-take-lanes",
            ShortcutAction::NextTake => "next-take",
            ShortcutAction::PreviousTake => "previous-take",
            ShortcutAction::AddToGroup => "add-to-group",
            ShortcutAction::RemoveFromGroup => "remove-from-group",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ShortcutAction::Undo => "Undo",
            ShortcutAction::Redo => "Redo",
            ShortcutAction::Save => "Save project",
            ShortcutAction::Copy => "Copy",
            ShortcutAction::Cut => "Cut",
            ShortcutAction::Paste => "Paste",
            ShortcutAction::Duplicate => "Duplicate (notes, else clips)",
            ShortcutAction::SelectAll => "Select all",
            ShortcutAction::Delete => "Delete (notes, else clips)",
            ShortcutAction::AddTrack => "Add audio track",
            ShortcutAction::AddInstrumentTrack => "Add instrument track",
            ShortcutAction::MoveTrackUp => "Move track up",
            ShortcutAction::MoveTrackDown => "Move track down",
            ShortcutAction::SplitAtPlayhead => "Split at playhead",
            ShortcutAction::JoinClips => "Join clips",
            ShortcutAction::ToggleArrangementLoop => "Toggle arrangement loop",
            ShortcutAction::ToggleClipLoop => "Toggle clip loop",
            ShortcutAction::CreateClipFromSelection => "Create clip from selection",
            ShortcutAction::ZoomToFit => "Zoom to fit",
            ShortcutAction::ZoomToSelection => "Zoom to selection",
            ShortcutAction::ZoomBack => "Zoom back",
            ShortcutAction::TogglePlayback => "Play / stop",
            ShortcutAction::TogglePianoRollEditMode => "Piano roll draw mode",
            ShortcutAction::QuickAdd => "Quick add palette",
            ShortcutAction::OverviewZoomIn => "Overview zoom in (wheel)",
            ShortcutAction::OverviewZoomOut => "Overview zoom out (wheel)",
            ShortcutAction::ToggleMetronome => "Metronome on / off",
            ShortcutAction::MetronomeLouder => "Metronome louder",
            ShortcutAction::MetronomeQuieter => "Metronome quieter",
            ShortcutAction::MidiLearn => "MIDI learn",
            ShortcutAction::GroupTracks => "Group selected tracks",
            ShortcutAction::UngroupTracks => "Ungroup",
            ShortcutAction::FreezeTrack => "Freeze / unfreeze track",
            ShortcutAction::BounceInPlace => "Bounce track in place",
            ShortcutAction::RenameTrack => "Rename track or group",
            ShortcutAction::ShowTakeLanes => "Show / hide take lanes",
            ShortcutAction::NextTake => "Next take for the selected clip",
            ShortcutAction::PreviousTake => "Previous take for the selected clip",
            ShortcutAction::AddToGroup => "Move selection into selected group",
            ShortcutAction::RemoveFromGroup => "Move selection out of its group",
        }
    }

    pub fn group(&self) -> &'static str {
        match self {
            ShortcutAction::Undo
            | ShortcutAction::Redo
            | ShortcutAction::Save
            | ShortcutAction::Copy
            | ShortcutAction::Cut
            | ShortcutAction::Paste
            | ShortcutAction::Duplicate
            | ShortcutAction::SelectAll
            | ShortcutAction::Delete => "Editing",
            ShortcutAction::AddTrack
            | ShortcutAction::AddInstrumentTrack
            | ShortcutAction::MoveTrackUp
            | ShortcutAction::MoveTrackDown => "Tracks",
            ShortcutAction::SplitAtPlayhead
            | ShortcutAction::JoinClips
            | ShortcutAction::ToggleArrangementLoop
            | ShortcutAction::ToggleClipLoop
            | ShortcutAction::CreateClipFromSelection => "Clips",
            ShortcutAction::ZoomToFit
            | ShortcutAction::ZoomToSelection
            | ShortcutAction::ZoomBack
            | ShortcutAction::TogglePlayback
            | ShortcutAction::QuickAdd
            | ShortcutAction::OverviewZoomIn
            | ShortcutAction::OverviewZoomOut => "View and transport",
            ShortcutAction::TogglePianoRollEditMode => "Piano roll",
            ShortcutAction::ToggleMetronome
            | ShortcutAction::MetronomeLouder
            | ShortcutAction::MetronomeQuieter => "Metronome",
            ShortcutAction::MidiLearn => "MIDI",
            ShortcutAction::GroupTracks
            | ShortcutAction::UngroupTracks
            | ShortcutAction::AddToGroup
            | ShortcutAction::RemoveFromGroup => "Groups",
            ShortcutAction::FreezeTrack
            | ShortcutAction::BounceInPlace
            | ShortcutAction::RenameTrack => "Tracks",
            ShortcutAction::ShowTakeLanes
            | ShortcutAction::NextTake
            | ShortcutAction::PreviousTake => "Takes",
        }
    }

    /// Whether this action is bound to the mouse wheel rather than keys.
    pub fn is_wheel(&self) -> bool {
        matches!(
            self,
            ShortcutAction::OverviewZoomIn | ShortcutAction::OverviewZoomOut
        )
    }

    /// The bindings this action ships with.
    pub fn default_combos(&self) -> Vec<KeyCombo> {
        let c = |key: &str| KeyCombo::new(key, true, false, false);
        let cs = |key: &str| KeyCombo::new(key, true, true, false);
        match self {
            ShortcutAction::Undo => vec![c("z")],
            ShortcutAction::Redo => vec![cs("z"), c("y")],
            ShortcutAction::Save => vec![c("s")],
            ShortcutAction::Copy => vec![c("c")],
            ShortcutAction::Cut => vec![c("x")],
            ShortcutAction::Paste => vec![c("v")],
            ShortcutAction::Duplicate => vec![c("d")],
            ShortcutAction::SelectAll => vec![c("a")],
            ShortcutAction::Delete => {
                vec![
                    KeyCombo::new("Delete", false, false, false),
                    KeyCombo::new("Backspace", false, false, false),
                ]
            }
            ShortcutAction::AddTrack => vec![c("t")],
            ShortcutAction::AddInstrumentTrack => vec![cs("t")],
            ShortcutAction::MoveTrackUp => vec![c("ArrowUp")],
            ShortcutAction::MoveTrackDown => vec![c("ArrowDown")],
            ShortcutAction::SplitAtPlayhead => vec![c("e")],
            ShortcutAction::JoinClips => vec![c("j")],
            ShortcutAction::ToggleArrangementLoop => vec![c("l")],
            ShortcutAction::ToggleClipLoop => vec![cs("l")],
            ShortcutAction::CreateClipFromSelection => vec![c("m")],
            ShortcutAction::ZoomToFit => vec![c("0")],
            ShortcutAction::ZoomToSelection => vec![KeyCombo::new("z", false, false, false)],
            ShortcutAction::ZoomBack => vec![KeyCombo::new("x", false, false, false)],
            ShortcutAction::TogglePlayback => vec![KeyCombo::new("Space", false, false, false)],
            ShortcutAction::TogglePianoRollEditMode => {
                vec![KeyCombo::new("b", false, false, false)]
            }
            ShortcutAction::QuickAdd => vec![c("p")],
            ShortcutAction::OverviewZoomIn => vec![KeyCombo::new(WHEEL_UP, true, false, false)],
            ShortcutAction::OverviewZoomOut => vec![KeyCombo::new(WHEEL_DOWN, true, false, false)],
            ShortcutAction::ToggleMetronome => vec![c("k")],
            ShortcutAction::MetronomeLouder => vec![cs("ArrowUp")],
            ShortcutAction::MetronomeQuieter => vec![cs("ArrowDown")],
            ShortcutAction::MidiLearn => vec![cs("k")],
            ShortcutAction::GroupTracks => vec![c("g")],
            ShortcutAction::UngroupTracks => vec![cs("g")],
            ShortcutAction::FreezeTrack => vec![cs("f")],
            // Ctrl+B stays free: bare B draws in the piano roll.
            ShortcutAction::BounceInPlace => vec![KeyCombo::new("b", true, false, true)],
            ShortcutAction::RenameTrack => vec![c("r")],
            ShortcutAction::ShowTakeLanes => vec![KeyCombo::new("u", true, false, true)],
            ShortcutAction::NextTake => vec![KeyCombo::new("ArrowDown", true, false, true)],
            ShortcutAction::PreviousTake => vec![KeyCombo::new("ArrowUp", true, false, true)],
            ShortcutAction::AddToGroup => vec![KeyCombo::new("g", true, false, true)],
            ShortcutAction::RemoveFromGroup => vec![KeyCombo::new("g", true, true, true)],
        }
    }

    /// The message this action dispatches to.
    pub fn message(&self) -> Message {
        match self {
            ShortcutAction::Undo => Message::Project(ProjectMsg::Undo),
            ShortcutAction::Redo => Message::Project(ProjectMsg::Redo),
            ShortcutAction::Save => Message::SaveProject,
            ShortcutAction::Copy => Message::Arrangement(ArrangementMsg::CopySelectedClips),
            ShortcutAction::Cut => Message::Arrangement(ArrangementMsg::CutSelectedClips),
            ShortcutAction::Paste => Message::Arrangement(ArrangementMsg::PasteClips),
            ShortcutAction::Duplicate => Message::DuplicatePressed,
            ShortcutAction::SelectAll => Message::SelectAllPressed,
            ShortcutAction::Delete => Message::DeleteKeyPressed,
            ShortcutAction::AddTrack => Message::Arrangement(ArrangementMsg::AddTrack),
            ShortcutAction::AddInstrumentTrack => {
                Message::Arrangement(ArrangementMsg::AddInstrumentTrack)
            }
            ShortcutAction::MoveTrackUp => {
                Message::Arrangement(ArrangementMsg::MoveSelectedTrackUp)
            }
            ShortcutAction::MoveTrackDown => {
                Message::Arrangement(ArrangementMsg::MoveSelectedTrackDown)
            }
            ShortcutAction::SplitAtPlayhead => Message::split_selected_at_playhead(),
            ShortcutAction::JoinClips => Message::join_selected_clips(),
            ShortcutAction::ToggleArrangementLoop => {
                Message::Transport(TransportMsg::ToggleArrangementLoop)
            }
            ShortcutAction::ToggleClipLoop => {
                Message::Arrangement(ArrangementMsg::ToggleSelectedClipLoop)
            }
            ShortcutAction::CreateClipFromSelection => Message::create_clip_from_selection(),
            ShortcutAction::ZoomToFit => Message::View(ViewMsg::ZoomToFit),
            ShortcutAction::ZoomToSelection => Message::View(ViewMsg::ZoomToSelection),
            ShortcutAction::ZoomBack => Message::View(ViewMsg::ZoomBack),
            ShortcutAction::TogglePlayback => Message::Transport(TransportMsg::TogglePlayback),
            ShortcutAction::TogglePianoRollEditMode => {
                Message::PianoRoll(PianoRollMsg::ToggleEditMode)
            }
            ShortcutAction::QuickAdd => Message::OpenQuickAdd,
            // Wheel actions are matched from wheel events (see
            // Shortcuts::wheel_action_for); bound to a key instead, they zoom
            // the arrangement around its centre.
            ShortcutAction::OverviewZoomIn => Message::View(ViewMsg::ZoomIn),
            ShortcutAction::OverviewZoomOut => Message::View(ViewMsg::ZoomOut),
            ShortcutAction::ToggleMetronome => Message::Transport(TransportMsg::ToggleMetronome),
            ShortcutAction::MetronomeLouder => {
                Message::Transport(TransportMsg::NudgeMetronomeVolume(0.1))
            }
            ShortcutAction::MetronomeQuieter => {
                Message::Transport(TransportMsg::NudgeMetronomeVolume(-0.1))
            }
            ShortcutAction::GroupTracks => {
                Message::Arrangement(ArrangementMsg::GroupSelectedTracks)
            }
            ShortcutAction::UngroupTracks => Message::Arrangement(ArrangementMsg::UngroupSelected),
            ShortcutAction::AddToGroup => Message::AddSelectionToSelectedGroup,
            ShortcutAction::RemoveFromGroup => {
                Message::Arrangement(ArrangementMsg::RemoveSelectionFromGroup)
            }
            ShortcutAction::FreezeTrack => Message::ToggleSelectedTrackFreeze,
            ShortcutAction::BounceInPlace => Message::BounceSelectedTrackInPlace,
            ShortcutAction::RenameTrack => Message::RenameSelectedTrack,
            ShortcutAction::ShowTakeLanes => Message::ToggleSelectedTrackTakeLanes,
            ShortcutAction::NextTake => {
                Message::Arrangement(ArrangementMsg::CycleTake { forward: true })
            }
            ShortcutAction::PreviousTake => {
                Message::Arrangement(ArrangementMsg::CycleTake { forward: false })
            }
            ShortcutAction::MidiLearn => {
                Message::MidiMap(crate::app::midi_mapping::MidiMapMsg::ToggleLearn)
            }
        }
    }
}

/// The effective shortcut map: defaults plus the user's overrides. An
/// action with an override uses only the override's combos; resetting an
/// action drops its override.
#[derive(Debug, Clone, Default)]
pub struct Shortcuts {
    overrides: HashMap<ShortcutAction, Vec<KeyCombo>>,
}

impl Shortcuts {
    pub fn from_overrides(overrides: HashMap<ShortcutAction, Vec<KeyCombo>>) -> Self {
        Self { overrides }
    }

    /// Which action, if any, the key event triggers. Exact matches win
    /// over shift-loose ones (see [`KeyCombo::matches_event_with`]).
    pub fn action_for(
        &self,
        key: &Key,
        modifiers: Modifiers,
        on_macos: bool,
    ) -> Option<ShortcutAction> {
        let exact = ShortcutAction::ALL.iter().copied().find(|action| {
            self.combos_for(*action)
                .iter()
                .any(|combo| combo.matches_event_with(key, modifiers, on_macos, false))
        });
        exact.or_else(|| {
            ShortcutAction::ALL.iter().copied().find(|action| {
                self.combos_for(*action)
                    .iter()
                    .any(|combo| combo.matches_event_with(key, modifiers, on_macos, true))
            })
        })
    }

    /// Which wheel-bound action, if any, this wheel turn triggers.
    pub fn wheel_action_for(
        &self,
        direction_up: bool,
        modifiers: Modifiers,
        on_macos: bool,
    ) -> Option<ShortcutAction> {
        ShortcutAction::ALL.iter().copied().find(|action| {
            action.is_wheel()
                && self
                    .combos_for(*action)
                    .iter()
                    .any(|combo| combo.matches_wheel(direction_up, modifiers, on_macos))
        })
    }

    pub fn combos_for(&self, action: ShortcutAction) -> Vec<KeyCombo> {
        self.overrides
            .get(&action)
            .cloned()
            .unwrap_or_else(|| action.default_combos())
    }

    /// Replace an action's combos. Passing the defaults (or an empty list)
    /// resets the action. A combo already bound elsewhere is removed from
    /// that other action first, so one combo cannot trigger two actions.
    pub fn set_binding(&mut self, action: ShortcutAction, combos: Vec<KeyCombo>) {
        for (other, other_combos) in self.overrides.iter_mut() {
            if *other != action {
                other_combos.retain(|combo| !combos.contains(combo));
            }
        }
        if combos.is_empty() || combos == action.default_combos() {
            self.overrides.remove(&action);
        } else {
            self.overrides.insert(action, combos);
        }
    }

    pub fn is_custom(&self, action: ShortcutAction) -> bool {
        self.overrides.contains_key(&action)
    }

    pub fn overrides(&self) -> &HashMap<ShortcutAction, Vec<KeyCombo>> {
        &self.overrides
    }
}

/// The wheel-zoom bindings plus the modifiers currently held, handed to
/// timeline widgets (overview, ruler, lanes, clip editors) so one wheel
/// gesture — Ctrl+WheelUp/Down by default, rebindable in Settings →
/// Shortcuts — zooms everywhere.
#[derive(Debug, Clone, Default)]
pub struct WheelZoom {
    pub combos: Vec<KeyCombo>,
    pub modifiers: Modifiers,
}

impl WheelZoom {
    /// Whether a wheel turn in `direction_up` matches a bound zoom combo.
    pub fn matches(&self, direction_up: bool, on_macos: bool) -> bool {
        self.combos
            .iter()
            .any(|combo| combo.matches_wheel(direction_up, self.modifiers, on_macos))
    }
}

/// Both wheel actions' combos plus the modifiers currently held.
pub fn wheel_zoom(shortcuts: &Shortcuts, held: Modifiers) -> WheelZoom {
    let mut combos = shortcuts.combos_for(ShortcutAction::OverviewZoomIn);
    combos.extend(shortcuts.combos_for(ShortcutAction::OverviewZoomOut));
    WheelZoom {
        combos,
        modifiers: held,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl() -> Modifiers {
        Modifiers::CTRL
    }

    #[test]
    fn defaults_match_the_historic_bindings() {
        let shortcuts = Shortcuts::default();
        let key = |c: &str| Key::Character(c.into());
        assert_eq!(
            shortcuts.action_for(&key("d"), ctrl(), false),
            Some(ShortcutAction::Duplicate)
        );
        assert_eq!(
            shortcuts.action_for(&Key::Named(Named::Delete), Modifiers::empty(), false),
            Some(ShortcutAction::Delete)
        );
        assert_eq!(
            shortcuts.action_for(&Key::Named(Named::Space), Modifiers::empty(), false),
            Some(ShortcutAction::TogglePlayback)
        );
        assert_eq!(
            shortcuts.action_for(&key("z"), Modifiers::empty(), false),
            Some(ShortcutAction::ZoomToSelection)
        );
        // Ctrl+Z is undo, not zoom.
        assert_eq!(
            shortcuts.action_for(&key("z"), ctrl(), false),
            Some(ShortcutAction::Undo)
        );
        // An unrelated key triggers nothing.
        assert_eq!(shortcuts.action_for(&key("q"), ctrl(), false), None);
    }

    #[test]
    fn a_rebound_action_stops_answering_its_default_key() {
        let mut shortcuts = Shortcuts::default();
        shortcuts.set_binding(
            ShortcutAction::Duplicate,
            vec![KeyCombo::new("p", true, false, false)],
        );
        assert_eq!(
            shortcuts.action_for(&Key::Character("p".into()), ctrl(), false),
            Some(ShortcutAction::Duplicate)
        );
        assert_eq!(
            shortcuts.action_for(&Key::Character("d".into()), ctrl(), false),
            None
        );
        assert!(shortcuts.is_custom(ShortcutAction::Duplicate));
    }

    #[test]
    fn resetting_restores_the_default() {
        let mut shortcuts = Shortcuts::default();
        shortcuts.set_binding(
            ShortcutAction::Duplicate,
            vec![KeyCombo::new("p", true, false, false)],
        );
        shortcuts.set_binding(
            ShortcutAction::Duplicate,
            ShortcutAction::Duplicate.default_combos(),
        );
        assert!(!shortcuts.is_custom(ShortcutAction::Duplicate));
        assert_eq!(
            shortcuts.action_for(&Key::Character("d".into()), ctrl(), false),
            Some(ShortcutAction::Duplicate)
        );
    }

    #[test]
    fn one_combo_cannot_trigger_two_actions() {
        let mut shortcuts = Shortcuts::default();
        // Bind Copy to Ctrl+D, which Duplicate owns by default.
        shortcuts.set_binding(
            ShortcutAction::Copy,
            vec![KeyCombo::new("d", true, false, false)],
        );
        assert_eq!(
            shortcuts.action_for(&Key::Character("d".into()), ctrl(), false),
            Some(ShortcutAction::Copy)
        );
    }

    #[test]
    fn pure_modifier_presses_are_not_shortcuts() {
        assert!(KeyCombo::from_event(&Key::Named(Named::Control), ctrl(), false).is_none());
        assert!(KeyCombo::from_event(&Key::Named(Named::Shift), Modifiers::SHIFT, false).is_none());
    }

    #[test]
    fn wheel_actions_default_to_ctrl_wheel() {
        let shortcuts = Shortcuts::default();
        assert!(ShortcutAction::OverviewZoomIn.is_wheel());
        assert!(ShortcutAction::OverviewZoomOut.is_wheel());
        assert!(!ShortcutAction::Duplicate.is_wheel());
        assert_eq!(
            shortcuts.wheel_action_for(true, ctrl(), false),
            Some(ShortcutAction::OverviewZoomIn)
        );
        assert_eq!(
            shortcuts.wheel_action_for(false, ctrl(), false),
            Some(ShortcutAction::OverviewZoomOut)
        );
        // Plain wheel and Shift+Ctrl+wheel are not bound by default.
        assert_eq!(
            shortcuts.wheel_action_for(true, Modifiers::empty(), false),
            None
        );
        assert_eq!(
            shortcuts.wheel_action_for(true, ctrl() | Modifiers::SHIFT, false),
            None
        );
        // Wheel combos never answer key events.
        assert_eq!(
            shortcuts.action_for(&Key::Character(WHEEL_UP.into()), ctrl(), false),
            None
        );
    }

    #[test]
    fn wheel_binding_can_be_rebound_and_labels_pretty_print() {
        let mut shortcuts = Shortcuts::default();
        shortcuts.set_binding(
            ShortcutAction::OverviewZoomIn,
            vec![KeyCombo::new(WHEEL_UP, false, false, false)],
        );
        assert_eq!(
            shortcuts.wheel_action_for(true, Modifiers::empty(), false),
            Some(ShortcutAction::OverviewZoomIn)
        );
        assert_eq!(shortcuts.wheel_action_for(true, ctrl(), false), None);
        assert!(shortcuts.is_custom(ShortcutAction::OverviewZoomIn));
        assert_eq!(
            KeyCombo::new(WHEEL_UP, true, false, false).label(false),
            "Ctrl+Wheel ↑"
        );
        assert_eq!(
            KeyCombo::new(WHEEL_DOWN, true, false, false).label(false),
            "Ctrl+Wheel ↓"
        );
        // A wheel combo survives a serde round-trip (ui.json overrides).
        let json = serde_json::to_string(&KeyCombo::new(WHEEL_UP, true, false, false)).unwrap();
        let back: KeyCombo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, KeyCombo::new(WHEEL_UP, true, false, false));
    }
}
