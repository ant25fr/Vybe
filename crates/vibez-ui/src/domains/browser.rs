//! Browser domain: local sample browsing state, and drag-and-drop
//! from the browser into the arrangement.
//!
//! Only the synchronous state transitions live here. Anything that
//! spawns work (file dialogs, decode) stays in app.rs as an iced Task
//! and routes results back through these messages.

use std::path::PathBuf;

use crate::domains::device_panel_drop::DevicePanelDrop;
use crate::message::{LocalRootWatchEvent, SampleLibraryScanResult};
use crate::state::{BrowserState, LocalRootCatalogState, SampleBrowserMode};
use vibez_core::id::TrackId;
use vibez_core::track::MediaSourceRef;

/// Messages the browser domain handles (sync tranche).
#[derive(Debug, Clone)]
pub enum BrowserMsg {
    ToggleSampleBrowser,
    SetKeyboardFocus(bool),
    BeginDockResize,
    ResizeDock(f32),
    EndDockResize,
    NudgeDockWidth(f32),
    SampleBrowserSearchChanged(String),
    SelectLocalFolder(Option<PathBuf>),
    ToggleLocalFolder(PathBuf),
    CycleSearchScope,
    ShowMoreLocalResults,
    SelectSampleBrowserEntry(MediaSourceRef),
    SetSampleBrowserMode(SampleBrowserMode),
    /// Shows a device category (Instruments, Plugins) in the results pane,
    /// or `None` to go back to browsing samples.
    SetBrowserCategory(Option<crate::state::BrowserCategory>),
    BeginPendingDrag {
        source: MediaSourceRef,
        label: String,
        origin_x: f32,
        origin_y: f32,
    },
    /// Like BeginPendingDrag, but the whole selected rows move together.
    BeginPendingDragMany {
        sources: Vec<MediaSourceRef>,
        label: String,
        origin_x: f32,
        origin_y: f32,
    },
    PendingDragMoved {
        x: f32,
        y: f32,
    },
    DragHoverTrack {
        track_id: TrackId,
        beat: f64,
        compatible: bool,
    },
    /// The pointer entered a tag row of the TAGS section while a drag is
    /// in flight: the row lights up and a release there tags the drag.
    DragHoverTagRow {
        tag_id: u32,
    },
    /// A release over a tag row: set the tag on every dragged item.
    DropOnTagRow {
        tag_id: u32,
    },
    /// A press on a device row (instrument, effect, plug-in), which becomes
    /// a tag drag once the pointer moves.
    BeginPendingTaggableDrag {
        item: crate::state::BrowserTaggable,
        label: String,
        origin_x: f32,
        origin_y: f32,
    },
    DragHoverEmptyArrangement {
        beat: f64,
    },
    DragHoverSampler {
        track_id: TrackId,
    },
    DragHoverDrumRackPad {
        track_id: TrackId,
        pad_index: usize,
    },
    DragHoverDrumRack {
        track_id: TrackId,
        pad_index: usize,
    },
    DragHoverDevicePanel {
        track_id: TrackId,
        drop: DevicePanelDrop,
    },
    DragHoverClipSlot {
        track_id: TrackId,
        row: u32,
    },
    DragHoverNewTrackSlot {
        row: u32,
    },
    /// A file from another app entered the window.
    DesktopFileHovered(PathBuf),
    DesktopFileDropped(PathBuf),
    /// The file from another app left the window, or its drag was cancelled.
    DesktopDragLeft,
    /// The pointer moved while a file from another app is dragged, after the
    /// widgets under it reported any target.
    DesktopPointerMoved,
    /// A poll found the pointer of another app's drag over no tagged target,
    /// which may still be over an untagged device card.
    DesktopPointerOffTargets,
    ClearDragTarget,
    /// The cursor left `target`; another target it entered meanwhile stays.
    LeaveDragTarget(crate::state::BrowserDropTarget),
    CancelDrag(String),
    EndDragSample,
    /// What was read of the dragged file for its ghost; a failed read shows
    /// no ghost, and the drop still says why if it fails again.
    DragGhostRead(Result<Box<crate::domains::drop_ghost::DragGhostRead>, String>),
    LocalRootWatchEvent(LocalRootWatchEvent),
    ReconcileLocalRoot {
        root: PathBuf,
        revision: u64,
    },
    LocalRootCatalogReconciled {
        root: PathBuf,
        revision: u64,
        result: Result<SampleLibraryScanResult, String>,
    },
    RemoveSampleLibraryRoot(PathBuf),
    /// Show a special place (Pinned / Recent) in the results pane, or go
    /// back to folders with `None`.
    SelectSpecialPlace(Option<crate::state::SpecialPlace>),
    /// Collapse or expand a sidebar section ("PINNED", "LIBRARY", "FOLDERS").
    TogglePlacesSection(&'static str),
    /// Pin or unpin one entry.
    TogglePin(MediaSourceRef),
    /// Pin or unpin every source in the given selection.
    TogglePinSelection(Vec<MediaSourceRef>),
    /// Select the folder holding `source`, expanding it in the sidebar.
    ShowEntryInFolder(MediaSourceRef),
    /// Show the items carrying any of these tags in the results pane.
    SelectTagPlace(Vec<u32>),
    /// Ctrl-click on a tag row: add/remove it in the current tag place.
    ToggleTagPlace(u32),
    /// Collapse or expand a tag folder in the sidebar.
    ToggleTagFolder(u32),
    /// Toggle one tag of the results header-dot filter (any-of).
    ToggleTagFilter(u32),
    /// Set or clear one tag on every source given (the row, or the whole
    /// selection when the row is part of it).
    ToggleItemTag {
        sources: Vec<MediaSourceRef>,
        tag_id: u32,
    },
    /// Make a tag (optionally inside a folder) and tag `apply_to` with it.
    NewTag {
        apply_to: Vec<MediaSourceRef>,
        folder: Option<u32>,
    },
    /// Set or clear one tag on a device (instrument, effect, plug-in).
    ToggleTaggableTag {
        item: crate::state::BrowserTaggable,
        tag_id: u32,
    },
    /// Make a tag (optionally inside a folder) and tag the device with it.
    NewTagForTaggable {
        item: crate::state::BrowserTaggable,
        folder: Option<u32>,
    },
    /// Make a tag folder (inside `parent`, top level when `None`).
    NewTagFolder {
        parent: Option<u32>,
    },
    /// Start an inline rename of a tag / folder in the context menu.
    StartRenameTag(u32),
    StartRenameFolder(u32),
    RenameTextChanged(String),
    /// Apply the inline rename (or drop it when the name is refused).
    CommitRename,
    CancelRename,
    RecolorTag {
        id: u32,
        color: u8,
    },
    /// Switch the results pane between the list and the sample map.
    SetResultsView(crate::state::BrowserResultsView),
    /// The sample map legend: show only this kind, or all when it is the
    /// active one again.
    ToggleSampleMapKindFilter(crate::state::SampleMapKind),
    /// Two-step removal: the first press arms the confirm, the second runs.
    RequestRemoveTag(u32),
    RequestRemoveTagFolder(u32),
    RemoveTag(u32),
    RemoveTagFolder(u32),
}

impl BrowserMsg {
    /// Whether this hovers an instrument or device target, which take
    /// samples only.
    fn hovers_a_device_target(&self) -> bool {
        matches!(
            self,
            Self::DragHoverSampler { .. }
                | Self::DragHoverDrumRackPad { .. }
                | Self::DragHoverDrumRack { .. }
                | Self::DragHoverDevicePanel { .. }
        )
    }
}

/// Cross-domain effects requested by a browser update.
#[derive(Debug, Default, PartialEq)]
pub struct BrowserAction {
    /// Status bar text.
    pub status: Option<String>,
    /// UI settings changed (browser visibility, library roots).
    pub persist_settings: bool,
    /// Decode a Local selection for the truthful Audition waveform without
    /// starting playback.
    pub load_waveform: Option<MediaSourceRef>,
    /// Debounce filesystem activity before asking the domain to reconcile.
    pub debounce_root_scans: Vec<(PathBuf, u64)>,
    /// The newest debounce token for this root matured and may be scanned.
    pub scan_root: Option<(PathBuf, u64)>,
    /// Release the drag here, as a button release over it would.
    pub release: Option<crate::state::BrowserDropTarget>,
    /// Read the file a drag just started carrying, off the UI thread, for
    /// its ghost.
    pub read_drag_ghost: Option<MediaSourceRef>,
}

/// What a drag carries, which decides the tracks it lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragMedia {
    Audio,
    Midi,
}

impl DragMedia {
    pub fn of(source: &MediaSourceRef) -> Self {
        match source {
            MediaSourceRef::LocalFile { path }
                if vibez_core::midi_file::is_midi_file_path(path) =>
            {
                Self::Midi
            }
            _ => Self::Audio,
        }
    }

    /// Whether a lane of `kind` takes this media.
    pub fn lands_on(self, kind: vibez_core::midi::TrackKind) -> bool {
        kind.is_midi() == (self == Self::Midi)
    }

    /// The media's name as a sentence starts it.
    fn title(self) -> &'static str {
        match self {
            Self::Audio => "Audio",
            Self::Midi => "MIDI",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Midi => "MIDI",
        }
    }

    /// What a lane of the other kind says when it turns this media away.
    fn refused_lane_status(self) -> String {
        match self {
            Self::Audio => "Audio can't go on a MIDI lane".into(),
            Self::Midi => "MIDI can't go on an audio lane".into(),
        }
    }
}

impl crate::state::BrowserDropTarget {
    /// What a release of `media` here does, as the status bar says it.
    pub fn status(&self, media: DragMedia) -> String {
        use crate::state::BrowserDropTarget as Target;
        let kind = media.noun();
        match self {
            Target::ArrangementLane {
                beat,
                compatible: true,
                ..
            } => format!("{} lane at beat {beat:.2}", media.title()),
            Target::ArrangementLane { .. } => media.refused_lane_status(),
            Target::EmptyArrangement { beat } => format!("New {kind} track at beat {beat:.2}"),
            Target::Sampler { .. } => DevicePanelDrop::LoadIntoExisting.hover_status(),
            Target::DrumRackPad { pad_index, .. } => {
                format!("Load into Drum Rack pad {}", pad_index + 1)
            }
            Target::DrumRack { pad_index, .. } => {
                format!("Load into selected Drum Rack pad {}", pad_index + 1)
            }
            Target::DevicePanel { drop, .. } => drop.hover_status(),
            Target::ClipSlot { row, .. } => format!("Clip slot in row {}", row + 1),
            Target::NewTrackSlot { row } => {
                format!("New {kind} track, Clip slot in row {}", row + 1)
            }
            Target::TagRow { tag_id } => format!("Tag #{tag_id}"),
        }
    }

    /// Whether `other` names the same place under the cursor; a panel's
    /// outcome can change while the cursor stays on it.
    fn is_place_of(&self, other: &Self) -> bool {
        use crate::state::BrowserDropTarget as Target;
        match (self, other) {
            (Target::DevicePanel { track_id, .. }, Target::DevicePanel { track_id: left, .. }) => {
                track_id == left
            }
            (Target::EmptyArrangement { .. }, Target::EmptyArrangement { .. }) => true,
            _ => self == other,
        }
    }

    /// Whether a release here takes the audio.
    pub fn accepts_drop(&self) -> bool {
        !matches!(
            self,
            crate::state::BrowserDropTarget::ArrangementLane {
                compatible: false,
                ..
            }
        )
    }
}

impl BrowserState {
    /// What the drag in flight carries.
    pub fn drag_media(&self) -> Option<DragMedia> {
        self.drag_source.as_ref().map(DragMedia::of)
    }

    /// Whether a lane of `kind` takes the drag in flight.
    pub fn drag_lands_on(&self, kind: vibez_core::midi::TrackKind) -> bool {
        self.drag_media().is_some_and(|media| media.lands_on(kind))
    }

    /// Why a lane of `kind` turns the drag in flight away, if it does.
    pub fn lane_refusal(&self, kind: vibez_core::midi::TrackKind) -> Option<String> {
        let media = self.drag_media()?;
        (!media.lands_on(kind)).then(|| media.refused_lane_status())
    }

    /// Instrument and device targets take samples, never MIDI files; a MIDI
    /// drag passing over one says where it can go instead.
    fn refuses_device_targets(&mut self) -> Option<String> {
        if self.drag_media() != Some(DragMedia::Midi) {
            return None;
        }
        self.drag_target = None;
        Some(crate::domains::midi_file_import::MIDI_FILE_TARGETS.into())
    }

    /// Whether a release would take the audio, for the ghost's dot; with no
    /// target under the cursor there is nothing to judge.
    pub fn drag_accepted(&self) -> Option<bool> {
        self.drag_target
            .as_ref()
            .map(|target| target.accepts_drop())
    }

    /// The zone to mark where a release would land.
    pub fn drop_zone(&self) -> Option<crate::domains::drop_zone::DropZone> {
        self.drag_target
            .as_ref()?
            .drop_zone(self.drag_media().unwrap_or(DragMedia::Audio))
    }

    /// The zone `track_id`'s lane marks while a drag hovers it.
    pub fn lane_drop_zone(&self, track_id: TrackId) -> Option<crate::domains::drop_zone::DropZone> {
        match self.drag_target {
            Some(crate::state::BrowserDropTarget::ArrangementLane { track_id: lane, .. })
                if lane == track_id =>
            {
                self.drop_zone()
            }
            _ => None,
        }
    }

    /// The pad of `track_id`'s Drum Rack a release would load, to highlight it.
    pub fn drum_pad_drop_target(&self, track_id: TrackId) -> Option<usize> {
        match self.drag_target {
            Some(
                crate::state::BrowserDropTarget::DrumRackPad {
                    track_id: rack,
                    pad_index,
                }
                | crate::state::BrowserDropTarget::DrumRack {
                    track_id: rack,
                    pad_index,
                },
            ) if rack == track_id => Some(pad_index),
            _ => None,
        }
    }

    /// Makes `target` the drop target and says what a release there does; a
    /// desktop drop waiting for a target is released on it.
    fn hover(&mut self, target: crate::state::BrowserDropTarget, action: &mut BrowserAction) {
        action.status = Some(target.status(self.drag_media().unwrap_or(DragMedia::Audio)));
        self.drag_target = Some(target.clone());
        if self.desktop_drop_waits_for_target() {
            self.release_desktop_drop(target, action);
        }
    }

    /// Clears the drop target; while dragging, the status goes back to the
    /// drag itself so a left target's outcome does not linger.
    fn clear_drag_target(&mut self) -> Option<String> {
        self.drag_target = None;
        self.drag_source.is_some().then(|| self.moving_status())
    }

    fn moving_status(&self) -> String {
        let label = self.drag_label.as_deref().unwrap_or("media");
        if self.tag_drag.is_some() {
            return format!("Moving {label} - choose a tag row to tag it");
        }
        let targets = match self.drag_media() {
            Some(DragMedia::Midi) => "a MIDI lane, empty Arrange, or a MIDI track's Clip slot",
            _ => "an audio lane, empty Arrange, or a MIDI track's devices, or a tag row",
        };
        format!("Moving {label} - choose {targets}")
    }

    fn hovers_drum_rack_pad(&self, track_id: TrackId) -> bool {
        matches!(
            self.drag_target,
            Some(crate::state::BrowserDropTarget::DrumRackPad { track_id: rack, .. })
                if rack == track_id
        )
    }

    fn hovers_instrument_card(&self, track_id: TrackId) -> bool {
        self.hovers_drum_rack_pad(track_id)
            || matches!(
                self.drag_target,
                Some(
                    crate::state::BrowserDropTarget::Sampler { track_id: card }
                    | crate::state::BrowserDropTarget::DrumRack { track_id: card, .. }
                ) if card == track_id
            )
    }

    pub fn update(&mut self, msg: BrowserMsg) -> BrowserAction {
        let mut action = BrowserAction::default();
        if msg.hovers_a_device_target() {
            if let Some(status) = self.refuses_device_targets() {
                action.status = Some(status);
                return action;
            }
        }
        match msg {
            BrowserMsg::SetKeyboardFocus(focused) => self.keyboard_focus = focused && self.open,
            BrowserMsg::ToggleSampleBrowser => {
                self.open = !self.open;
                self.keyboard_focus &= self.open;
                action.persist_settings = true;
            }
            BrowserMsg::BeginDockResize => {
                self.dock_resize_active = true;
            }
            BrowserMsg::ResizeDock(width) => {
                if self.dock_resize_active {
                    self.set_dock_width(width);
                }
            }
            BrowserMsg::EndDockResize => {
                if self.dock_resize_active {
                    self.dock_resize_active = false;
                    action.persist_settings = true;
                }
            }
            BrowserMsg::NudgeDockWidth(delta) => {
                self.set_dock_width(self.dock_width + delta);
                action.persist_settings = true;
            }
            BrowserMsg::SampleBrowserSearchChanged(query) => {
                self.search = query;
                self.reset_results_window();
            }
            BrowserMsg::SelectLocalFolder(folder) => {
                self.mode = SampleBrowserMode::Local;
                self.special_place = None;
                self.select_local_folder(folder);
            }
            BrowserMsg::ToggleLocalFolder(folder) => {
                let should_expand = !self.expanded_local_folders.remove(&folder);
                if should_expand {
                    self.expanded_local_folders.insert(folder.clone());
                }
                self.mode = SampleBrowserMode::Local;
                self.select_local_folder(Some(folder.clone()));
                if !should_expand {
                    self.expanded_local_folders.remove(&folder);
                }
            }
            BrowserMsg::CycleSearchScope => {
                self.cycle_search_scope();
            }
            BrowserMsg::ShowMoreLocalResults => {
                self.results_visible_limit = self
                    .results_visible_limit
                    .saturating_add(crate::state::BROWSER_RESULTS_PAGE_SIZE);
            }
            BrowserMsg::SelectSampleBrowserEntry(source) => {
                if self.select_source(source.clone()) && DragMedia::of(&source) == DragMedia::Audio
                {
                    action.load_waveform = Some(source);
                }
            }
            BrowserMsg::SetSampleBrowserMode(mode) => {
                self.mode = mode;
                self.category = None;
                self.special_place = None;
                self.reset_results_window();
            }
            BrowserMsg::SetBrowserCategory(category) => {
                self.category = category;
                self.special_place = None;
                self.results_view = crate::state::BrowserResultsView::List;
                self.reset_results_window();
            }
            BrowserMsg::BeginPendingDrag {
                source,
                label,
                origin_x,
                origin_y,
            } => {
                self.begin_pending_drag(vec![source], label, origin_x, origin_y);
            }
            BrowserMsg::BeginPendingDragMany {
                sources,
                label,
                origin_x,
                origin_y,
            } => {
                self.begin_pending_drag(sources, label, origin_x, origin_y);
            }
            BrowserMsg::PendingDragMoved { x, y } => {
                if self.move_pending_drag(x, y) {
                    action.status = Some(self.moving_status());
                    action.read_drag_ghost = self.drag_source.clone();
                } else if self.move_pending_taggable_drag(x, y) {
                    action.status = Some(self.moving_status());
                }
            }
            BrowserMsg::DragHoverTrack {
                track_id,
                beat,
                compatible,
            } => {
                if self.drag_source.is_none() {
                    return action;
                }
                self.hover(
                    crate::state::BrowserDropTarget::ArrangementLane {
                        track_id,
                        beat,
                        compatible,
                    },
                    &mut action,
                );
            }
            BrowserMsg::DragHoverEmptyArrangement { beat } => {
                if self.drag_source.is_none() {
                    return action;
                }
                self.hover(
                    crate::state::BrowserDropTarget::EmptyArrangement { beat },
                    &mut action,
                );
            }
            BrowserMsg::DragHoverSampler { track_id } => {
                if self.drag_source.is_none() {
                    return action;
                }
                self.hover(
                    crate::state::BrowserDropTarget::Sampler { track_id },
                    &mut action,
                );
            }
            BrowserMsg::DragHoverDrumRackPad {
                track_id,
                pad_index,
            } => {
                if self.drag_source.is_none() {
                    return action;
                }
                self.hover(
                    crate::state::BrowserDropTarget::DrumRackPad {
                        track_id,
                        pad_index,
                    },
                    &mut action,
                );
            }
            BrowserMsg::DragHoverDrumRack {
                track_id,
                pad_index,
            } => {
                if self.drag_source.is_none() || self.hovers_drum_rack_pad(track_id) {
                    return action;
                }
                self.hover(
                    crate::state::BrowserDropTarget::DrumRack {
                        track_id,
                        pad_index,
                    },
                    &mut action,
                );
            }
            BrowserMsg::DragHoverDevicePanel { track_id, drop } => {
                // The panel's hover arrives after that of any card inside it,
                // so the card under the cursor keeps the target.
                if self.drag_source.is_none() || self.hovers_instrument_card(track_id) {
                    return action;
                }
                self.hover(
                    crate::state::BrowserDropTarget::DevicePanel { track_id, drop },
                    &mut action,
                );
            }
            BrowserMsg::LeaveDragTarget(target) => {
                if self
                    .drag_target
                    .as_ref()
                    .is_some_and(|current| current.is_place_of(&target))
                {
                    action.status = self.clear_drag_target();
                }
            }
            BrowserMsg::DragHoverClipSlot { track_id, row } => {
                if self.drag_source.is_none() {
                    return action;
                }
                self.hover(
                    crate::state::BrowserDropTarget::ClipSlot { track_id, row },
                    &mut action,
                );
            }
            BrowserMsg::DragHoverNewTrackSlot { row } => {
                if self.drag_source.is_none() {
                    return action;
                }
                self.hover(
                    crate::state::BrowserDropTarget::NewTrackSlot { row },
                    &mut action,
                );
            }
            BrowserMsg::DesktopFileHovered(path) => {
                let hovering = Some(crate::state::DesktopDrop::Hovering);
                let was_hovering = self.desktop_drop == hovering;
                action.status = self.hover_desktop_file(path);
                // A hover that starts the drag, or restarts a waiting drop
                // that cleared its ghost, reads the file.
                if self.desktop_drop == hovering && !was_hovering {
                    action.read_drag_ghost = self.drag_source.clone();
                }
            }
            BrowserMsg::DesktopFileDropped(path) => self.drop_desktop_file(&path, &mut action),
            BrowserMsg::DesktopDragLeft => {
                if self.drags_from_desktop() {
                    self.cancel_media_drag();
                    action.status = Some("Drag cancelled".into());
                }
            }
            BrowserMsg::DesktopPointerMoved => match self.desktop_drop {
                Some(crate::state::DesktopDrop::Hovering) => {
                    self.desktop_drop = Some(crate::state::DesktopDrop::PointerSeen);
                }
                Some(crate::state::DesktopDrop::Dropped) => self.miss_desktop_drop(&mut action),
                _ => {}
            },
            BrowserMsg::DesktopPointerOffTargets => {
                if self.desktop_drop == Some(crate::state::DesktopDrop::PointerSeen) {
                    self.desktop_drop = Some(crate::state::DesktopDrop::Hovering);
                }
                if self.drag_target.is_some() {
                    action.status = self.clear_drag_target();
                }
            }
            BrowserMsg::ClearDragTarget => {
                action.status = self.clear_drag_target();
            }
            BrowserMsg::CancelDrag(reason) => {
                self.cancel_media_drag();
                action.status = Some(reason);
            }
            BrowserMsg::DragGhostRead(Ok(read)) => self.install_ghost_cache(*read),
            BrowserMsg::DragGhostRead(Err(_)) => {}
            BrowserMsg::EndDragSample => {
                if self.drag_source.is_some() || self.tag_drag.is_some() {
                    self.cancel_media_drag();
                    action.status = Some("Drag cancelled".to_string());
                } else {
                    self.cancel_pending_drag();
                }
            }
            BrowserMsg::BeginPendingTaggableDrag {
                item,
                label,
                origin_x,
                origin_y,
            } => {
                self.begin_pending_taggable_drag(item, label, origin_x, origin_y);
            }
            BrowserMsg::DragHoverTagRow { tag_id } => {
                if self.drag_in_flight() {
                    self.drag_target = Some(crate::state::BrowserDropTarget::TagRow { tag_id });
                }
            }
            BrowserMsg::DropOnTagRow { tag_id } => {
                let mut items: Vec<crate::state::BrowserTaggable> = Vec::new();
                if let Some(source) = self.drag_source.clone() {
                    items.push(crate::state::BrowserTaggable::Sample(source));
                }
                for extra in self.drag_extra.clone() {
                    items.push(crate::state::BrowserTaggable::Sample(extra));
                }
                if let Some(item) = self.tag_drag.clone() {
                    items.push(item);
                }
                let name = self
                    .tag(tag_id)
                    .map(|tag| tag.name.clone())
                    .unwrap_or_else(|| format!("#{tag_id}"));
                self.cancel_media_drag();
                if items.is_empty() {
                    action.status = Some("Nothing is being dragged".to_string());
                } else {
                    for item in &items {
                        self.set_taggable_tag(item, tag_id);
                    }
                    action.status = Some(format!(
                        "Tagged {} item{} with {name}",
                        items.len(),
                        if items.len() == 1 { "" } else { "s" }
                    ));
                    action.persist_settings = true;
                }
            }
            BrowserMsg::LocalRootWatchEvent(event) => match event {
                LocalRootWatchEvent::Changed(roots) => {
                    for root in roots {
                        if self.roots.iter().any(|configured| configured == &root) {
                            let revision = self.begin_root_scan(&root, true);
                            action.debounce_root_scans.push((root, revision));
                        }
                    }
                }
                LocalRootWatchEvent::Watching(roots) => {
                    for root in roots {
                        self.root_watch_errors.remove(&root);
                    }
                    self.refresh_scan_diagnostics();
                }
                LocalRootWatchEvent::Failed { roots, message } => {
                    for root in roots {
                        if self.roots.iter().any(|configured| configured == &root) {
                            self.root_watch_errors.insert(root, message.clone());
                        }
                    }
                    self.refresh_scan_diagnostics();
                    action.status = Some(format!("Local watch error: {message}"));
                }
            },
            BrowserMsg::ReconcileLocalRoot { root, revision } => {
                if self.root_refresh_is_current(&root, revision) {
                    action.scan_root = Some((root, revision));
                }
            }
            BrowserMsg::LocalRootCatalogReconciled {
                root,
                revision,
                result,
            } => {
                if !self.root_refresh_is_current(&root, revision) {
                    return action;
                }
                // Watcher-driven refreshes (Updating) happen behind the
                // user's back; they must not collapse an expanded
                // results window the way user-initiated scans do.
                let from_watcher = matches!(
                    self.root_catalog_states.get(&root),
                    Some(LocalRootCatalogState::Updating)
                );
                match result {
                    Ok(scan) => {
                        self.entries.retain(|entry| entry.root_path != root);
                        self.entries.extend(scan.entries);
                        self.entries.sort_by(|a, b| {
                            a.root_path
                                .cmp(&b.root_path)
                                .then_with(|| a.relative_path.cmp(&b.relative_path))
                        });
                        self.folders.retain(|folder| folder.root_path != root);
                        self.folders.extend(scan.folders);
                        self.folders.sort_by(|a, b| {
                            a.root_path
                                .cmp(&b.root_path)
                                .then_with(|| a.relative_path.cmp(&b.relative_path))
                        });
                        self.bump_catalog_revision();
                        let warning_count = scan.warnings.len();
                        self.root_catalog_states.insert(
                            root.clone(),
                            LocalRootCatalogState::Ready {
                                warnings: scan.warnings,
                            },
                        );
                        if !from_watcher {
                            self.reset_results_window();
                        }
                        let current_exists = self.current_folder.as_ref().is_none_or(|current| {
                            self.roots.iter().any(|root| root == current)
                                || self.folders.iter().any(|folder| &folder.path == current)
                        });
                        if !current_exists {
                            self.select_local_folder(Some(root.clone()));
                        }
                        if self
                            .selected_source
                            .as_ref()
                            .and_then(|selected| {
                                self.entries.iter().find(|entry| &entry.source == selected)
                            })
                            .is_none()
                        {
                            self.clear_selection();
                        }
                        self.refresh_scan_diagnostics();
                        let root_count = self
                            .entries
                            .iter()
                            .filter(|entry| entry.root_path == root)
                            .count();
                        action.status = Some(if warning_count == 0 {
                            format!("Indexed {root_count} samples in {}", root.display())
                        } else {
                            format!(
                                "Indexed {root_count} samples in {} with {warning_count} warning(s)",
                                root.display()
                            )
                        });
                    }
                    Err(error) => {
                        self.root_catalog_states.insert(
                            root.clone(),
                            LocalRootCatalogState::Stale {
                                error: error.clone(),
                            },
                        );
                        self.refresh_scan_diagnostics();
                        action.status = Some(format!(
                            "Local root unavailable; kept stale catalog for {} ({error})",
                            root.display()
                        ));
                    }
                }
            }
            BrowserMsg::RemoveSampleLibraryRoot(path) => {
                self.roots.retain(|root| root != &path);
                if self
                    .current_folder
                    .as_ref()
                    .is_some_and(|folder| folder.starts_with(&path))
                {
                    self.select_local_folder(None);
                }
                self.entries.retain(|entry| entry.root_path != path);
                self.folders.retain(|folder| folder.root_path != path);
                self.bump_catalog_revision();
                self.expanded_local_folders
                    .retain(|folder| !folder.starts_with(&path));
                self.root_catalog_states.remove(&path);
                self.root_refresh_revisions.remove(&path);
                self.root_watch_errors.remove(&path);
                self.scan_warnings.clear();
                self.scan_error = None;
                self.refresh_scan_diagnostics();
                if self
                    .selected_source
                    .as_ref()
                    .and_then(|selected| {
                        self.entries.iter().find(|entry| &entry.source == selected)
                    })
                    .is_none()
                {
                    self.clear_selection();
                }
                action.persist_settings = true;
                action.status = Some("Removed sample root".to_string());
            }
            BrowserMsg::SelectSpecialPlace(place) => {
                self.select_special_place(place);
                self.reset_results_window();
            }
            BrowserMsg::TogglePlacesSection(section) => {
                self.toggle_section(section);
            }
            BrowserMsg::TogglePin(source) => {
                let pinned = self.toggle_pin(source);
                action.status = Some(if pinned {
                    "Pinned".into()
                } else {
                    "Unpinned".into()
                });
                action.persist_settings = true;
            }
            BrowserMsg::TogglePinSelection(sources) => {
                let count = sources.len();
                let any_unpinned = sources.iter().any(|source| !self.is_pinned(source));
                for source in sources {
                    if any_unpinned {
                        if !self.is_pinned(&source) {
                            self.pins.push(source);
                        }
                    } else if let Some(index) = self.pins.iter().position(|p| p == &source) {
                        self.pins.remove(index);
                    }
                }
                action.status = Some(format!(
                    "{} {} item{}",
                    if any_unpinned { "Pinned" } else { "Unpinned" },
                    count,
                    if count == 1 { "" } else { "s" }
                ));
                action.persist_settings = true;
            }
            BrowserMsg::SetResultsView(view) => {
                self.results_view = view;
                self.reset_results_window();
            }
            BrowserMsg::ToggleSampleMapKindFilter(kind) => {
                self.sample_map_kind_filter = if self.sample_map_kind_filter == Some(kind) {
                    None
                } else {
                    Some(kind)
                };
            }
            BrowserMsg::SelectTagPlace(ids) => {
                self.select_tag_place(ids);
                self.reset_results_window();
            }
            BrowserMsg::ToggleTagPlace(id) => {
                self.toggle_tag_in_place(id);
                self.reset_results_window();
            }
            BrowserMsg::ToggleTagFolder(id) => {
                self.toggle_tag_folder_expanded(id);
            }
            BrowserMsg::ToggleTagFilter(id) => {
                let active = self.toggle_tag_filter(id);
                let name = self
                    .tag(id)
                    .map(|tag| tag.name.clone())
                    .unwrap_or_else(|| format!("#{id}"));
                action.status = Some(if active {
                    format!("Filtering by {name}")
                } else {
                    format!("No longer filtering by {name}")
                });
            }
            BrowserMsg::ToggleItemTag { sources, tag_id } => {
                if let Some(tag) = self.tag(tag_id) {
                    let name = tag.name.clone();
                    let add = sources.iter().any(|s| !self.tags_of(s).contains(&tag_id));
                    for source in &sources {
                        let now = self.toggle_item_tag(source, tag_id);
                        debug_assert_eq!(now, add);
                    }
                    action.status = Some(if add {
                        format!(
                            "Tagged {} item{} with {name}",
                            sources.len(),
                            if sources.len() == 1 { "" } else { "s" }
                        )
                    } else {
                        format!(
                            "Removed {name} from {} item{}",
                            sources.len(),
                            if sources.len() == 1 { "" } else { "s" }
                        )
                    });
                    action.persist_settings = true;
                }
            }
            BrowserMsg::NewTag { apply_to, folder } => {
                let name = self.default_tag_name();
                let color = self.next_tag_color();
                let made = match folder {
                    None => self.make_tag(&name, color),
                    Some(folder_id) => self.make_tag_in(Some(folder_id), &name, color),
                };
                match made {
                    Ok(id) => {
                        for source in &apply_to {
                            self.toggle_item_tag(source, id);
                        }
                        if apply_to.is_empty() {
                            // Made from the sidebar: rename it inline.
                            self.renaming_tag = Some(id);
                            self.rename_text = name.clone();
                        }
                        action.status = Some(format!(
                            "Made tag {name}, tagged {} item{}",
                            apply_to.len(),
                            if apply_to.len() == 1 { "" } else { "s" }
                        ));
                        action.persist_settings = true;
                    }
                    Err(err) => action.status = Some(err),
                }
            }
            BrowserMsg::ToggleTaggableTag { item, tag_id } => {
                let name = self
                    .tag(tag_id)
                    .map(|tag| tag.name.clone())
                    .unwrap_or_else(|| format!("#{tag_id}"));
                let label = item.label();
                if self.toggle_taggable_tag(&item, tag_id) {
                    action.status = Some(format!("Tagged {label} with {name}"));
                } else {
                    action.status = Some(format!("Removed {name} from {label}"));
                }
                action.persist_settings = true;
            }
            BrowserMsg::NewTagForTaggable { item, folder } => {
                let name = self.default_tag_name();
                let color = self.next_tag_color();
                let made = match folder {
                    None => self.make_tag(&name, color),
                    Some(folder_id) => self.make_tag_in(Some(folder_id), &name, color),
                };
                match made {
                    Ok(id) => {
                        self.set_taggable_tag(&item, id);
                        // Made from a device row's menu: rename it inline.
                        self.renaming_tag = Some(id);
                        self.rename_text = name.clone();
                        action.status = Some(format!("Made tag {name}, tagged {}", item.label()));
                        action.persist_settings = true;
                    }
                    Err(err) => action.status = Some(err),
                }
            }
            BrowserMsg::NewTagFolder { parent } => {
                let name = self.default_folder_name();
                match self.make_folder(parent, &name) {
                    Ok(id) => {
                        self.expanded_tag_folders.insert(id);
                        self.renaming_folder = Some(id);
                        self.rename_text = name;
                        action.persist_settings = true;
                    }
                    Err(err) => action.status = Some(err),
                }
            }
            BrowserMsg::StartRenameTag(id) => {
                let name = self.tag(id).map(|tag| tag.name.clone());
                if let Some(name) = name {
                    self.renaming_tag = Some(id);
                    self.renaming_folder = None;
                    self.rename_text = name;
                }
            }
            BrowserMsg::StartRenameFolder(id) => {
                fn find_name(nodes: &[crate::state::TagFolderNode], id: u32) -> Option<String> {
                    for node in nodes {
                        if node.id == id {
                            return Some(node.name.clone());
                        }
                        if let Some(found) = find_name(&node.folders, id) {
                            return Some(found);
                        }
                    }
                    None
                }
                if let Some(name) = find_name(&self.tag_tree, id) {
                    self.renaming_folder = Some(id);
                    self.renaming_tag = None;
                    self.rename_text = name;
                }
            }
            BrowserMsg::RenameTextChanged(text) => {
                self.rename_text = text;
            }
            BrowserMsg::CommitRename => {
                let renaming_tag = self.renaming_tag.take();
                let renaming_folder = self.renaming_folder.take();
                let text = std::mem::take(&mut self.rename_text);
                let result = if let Some(id) = renaming_tag {
                    self.rename_tag(id, &text)
                } else if let Some(id) = renaming_folder {
                    self.rename_folder(id, &text)
                } else {
                    Ok(())
                };
                if let Err(err) = result {
                    action.status = Some(err);
                } else if renaming_tag.is_some() || renaming_folder.is_some() {
                    action.persist_settings = true;
                }
            }
            BrowserMsg::CancelRename => {
                self.renaming_tag = None;
                self.renaming_folder = None;
                self.rename_text.clear();
            }
            BrowserMsg::RecolorTag { id, color } => {
                if self.recolor_tag(id, color) {
                    action.persist_settings = true;
                }
            }
            BrowserMsg::RequestRemoveTag(id) => {
                self.confirm_remove_tag = if self.confirm_remove_tag == Some(id) {
                    None
                } else {
                    Some(id)
                };
                self.confirm_remove_folder = None;
            }
            BrowserMsg::RequestRemoveTagFolder(id) => {
                self.confirm_remove_folder = if self.confirm_remove_folder == Some(id) {
                    None
                } else {
                    Some(id)
                };
                self.confirm_remove_tag = None;
            }
            BrowserMsg::RemoveTag(id) => {
                self.confirm_remove_tag = None;
                let name = self
                    .tag(id)
                    .map(|tag| tag.name.clone())
                    .unwrap_or_else(|| format!("#{id}"));
                let carriers = self.remove_tag(id);
                action.status = Some(format!(
                    "Removed tag {name} ({carriers} item{})",
                    if carriers == 1 { "" } else { "s" }
                ));
                action.persist_settings = true;
            }
            BrowserMsg::RemoveTagFolder(id) => {
                self.confirm_remove_folder = None;
                let moved = self.remove_folder(id);
                action.status = Some(format!(
                    "Removed tag folder, {moved} row{} lifted",
                    if moved == 1 { "" } else { "s" }
                ));
                action.persist_settings = true;
            }
            BrowserMsg::ShowEntryInFolder(source) => {
                if let MediaSourceRef::LocalFile { path } = &source {
                    if let Some(parent) = path.parent() {
                        if self.roots.iter().any(|root| parent.starts_with(root)) {
                            self.mode = SampleBrowserMode::Local;
                            self.special_place = None;
                            self.select_local_folder(Some(parent.to_path_buf()));
                        }
                    }
                }
            }
        }
        action
    }
}

#[path = "browser_desktop_drop.rs"]
mod desktop_drop;

#[cfg(test)]
#[path = "browser_desktop_drop_tests.rs"]
mod desktop_drop_tests;

#[cfg(test)]
#[path = "browser_midi_drag_tests.rs"]
mod midi_drag_tests;

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
