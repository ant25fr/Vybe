//! Sample-browser and audition UI state.
//! Split from state/mod.rs.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SampleBrowserMode {
    #[default]
    Local,
}

/// A device category the browser can show in place of samples, listed in
/// the Places pane above the sample roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserCategory {
    Instruments,
    Fx,
    Plugins,
}

impl BrowserCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Instruments => "Instruments",
            Self::Fx => "FX",
            Self::Plugins => "Plugins",
        }
    }

    /// The small-caps caption over the category's entries in the results pane.
    pub fn caption(self) -> &'static str {
        match self {
            Self::Instruments => "INSTRUMENTS",
            Self::Fx => "FX",
            Self::Plugins => "PLUGINS",
        }
    }

    pub fn icon(self) -> char {
        match self {
            Self::Instruments => crate::icons::MUSIC,
            Self::Fx => crate::icons::SLIDERS_VERTICAL,
            Self::Plugins => crate::icons::PLUG,
        }
    }
}

/// What the results pane shows for the current sample browsing scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserResultsView {
    #[default]
    List,
    /// The sample map: one coloured point per sample, on a grid.
    SampleMap,
}

/// A special sidebar place, shown in the results pane like a folder but
/// listing the library's pins or recent items instead of a folder's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecialPlace {
    Pinned,
    Recent,
    /// Items carrying any of these tag ids.
    Tags(Vec<u32>),
}

impl SpecialPlace {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pinned => "Pinned",
            Self::Recent => "Recent",
            Self::Tags(_) => "Tags",
        }
    }

    pub fn icon(&self) -> char {
        match self {
            Self::Pinned => crate::icons::PIN,
            Self::Recent => crate::icons::HISTORY,
            Self::Tags(_) => crate::icons::PALETTE,
        }
    }

    /// The small-caps caption over the place's rows in the results pane.
    pub fn caption(&self) -> &'static str {
        match self {
            Self::Pinned => "PINNED",
            Self::Recent => "RECENT",
            Self::Tags(_) => "TAGS",
        }
    }
}

/// The persistent form of a pinned or recent entry: its absolute path.
/// Only local files can be pinned or recent, so one string is enough.
pub fn browser_path_string(source: &MediaSourceRef) -> String {
    match source {
        MediaSourceRef::LocalFile { path } => path.display().to_string(),
        MediaSourceRef::StagedProjectMedia { source_path, .. } => source_path.display().to_string(),
        MediaSourceRef::StagedRemoteProjectMedia { provenance, .. } => match provenance.as_ref() {
            vibez_core::track::MediaProvenance::Local { source_path } => {
                source_path.display().to_string()
            }
            _ => String::new(),
        },
        MediaSourceRef::ProjectMedia { .. } | MediaSourceRef::DropboxFile { .. } => String::new(),
    }
}

pub const BROWSER_DOCK_MIN_WIDTH: f32 = 300.0;
pub const BROWSER_DOCK_DEFAULT_WIDTH: f32 = 410.0;
pub const BROWSER_DOCK_MAX_WIDTH: f32 = 650.0;
pub const ARRANGE_MIN_WIDTH_WITH_BROWSER: f32 = 560.0;
pub const BROWSER_PLACES_MIN_WIDTH: f32 = 124.0;
pub const BROWSER_PLACES_MAX_WIDTH: f32 = 176.0;
pub const BROWSER_RESULTS_PAGE_SIZE: usize = 200;
/// How many recent items the browser keeps.
pub const BROWSER_RECENT_CAP: usize = 50;
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrowserAuditionPlan {
    Raw,
    Warp { source_bpm: f64 },
}

/// A MIDI file handed to the Audition Bus and the track it plays on, fixed
/// when it started.
#[derive(Debug, Clone, PartialEq)]
pub struct MidiAuditionStart {
    pub source: MediaSourceRef,
    pub target: Option<TrackId>,
}

/// What the Audition footer says the Audition is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditionStatus {
    /// A Raw buffer plays while its Warp treatment is prepared.
    Warping,
    Preparing,
    Queued,
    Playing(Option<AuditionMode>),
    /// The selected source could not be read, so its strip or waveform is
    /// empty.
    Unavailable,
    /// A source is ready to audition in the selected treatment.
    Ready(AuditionMode),
    Select,
}

impl AuditionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Warping => "WARPING",
            Self::Preparing => "PREPARING",
            Self::Queued => "QUEUED",
            Self::Playing(Some(mode)) | Self::Ready(mode) => mode.label(),
            Self::Playing(None) => "PLAYING",
            Self::Unavailable => "UNAVAILABLE",
            Self::Select => "SELECT",
        }
    }

    /// Whether the status keeps the footer's fixed width, so the waveform or
    /// strip beside it never shifts. An unreadable source leaves that empty,
    /// so its longer status may push it aside.
    pub fn keeps_its_width(self) -> bool {
        self != Self::Unavailable
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserSearchScope {
    #[default]
    SelectedFolder,
    Root,
    Everywhere,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalRootCatalogState {
    Indexing,
    Updating,
    Ready { warnings: Vec<String> },
    Stale { error: String },
}

impl LocalRootCatalogState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Indexing => "INDEXING",
            Self::Updating => "UPDATING",
            Self::Ready { warnings } if warnings.is_empty() => "READY",
            Self::Ready { .. } => "WARN",
            Self::Stale { .. } => "STALE",
        }
    }

    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Indexing | Self::Updating)
    }
}

/// Browser domain slice: local sample browsing, and
/// drag-and-drop from the browser into the arrangement.
#[derive(Debug, Clone)]
pub struct BrowserState {
    pub open: bool,
    pub keyboard_focus: bool,
    /// Remembered user width. The rendered width may temporarily yield to a
    /// narrow window without overwriting this preference.
    pub dock_width: f32,
    pub dock_resize_active: bool,
    pub search: String,
    pub roots: Vec<PathBuf>,
    pub entries: Vec<SampleBrowserEntry>,
    pub folders: Vec<SampleBrowserFolder>,
    /// Bumped whenever `entries`/`folders` change so memoized result
    /// lists (see [`Self::local_results`]) know to recompute.
    pub catalog_revision: u64,
    pub(crate) local_results_cache: std::cell::RefCell<LocalResults>,
    /// Absolute Local Source Storage folder currently shown in Results. `None`
    /// is the All Roots location.
    pub current_folder: Option<PathBuf>,
    pub expanded_local_folders: HashSet<PathBuf>,
    pub search_scope: BrowserSearchScope,
    pub results_visible_limit: usize,
    pub root_catalog_states: HashMap<PathBuf, LocalRootCatalogState>,
    pub root_refresh_revisions: HashMap<PathBuf, u64>,
    pub root_watch_errors: HashMap<PathBuf, String>,
    pub scan_warnings: Vec<String>,
    pub scan_error: Option<String>,
    pub selected_source: Option<MediaSourceRef>,
    /// Extra selected rows of a multi-selection (Ctrl+click); the primary
    /// selection stays `selected_source`, so preview/audition keep working.
    pub selected_extra: std::collections::HashSet<MediaSourceRef>,
    /// Which sidebar place is shown besides folders and categories.
    pub special_place: Option<SpecialPlace>,
    /// Pinned items, most recently pinned last. Persisted in ui.json.
    pub pins: Vec<MediaSourceRef>,
    /// Recently imported items, newest first, capped at 50. Persisted.
    pub recent: Vec<MediaSourceRef>,
    /// Row under the pointer, to show its play button.
    pub hovered_source: Option<MediaSourceRef>,
    /// Sidebar section open state: "PINNED", "LIBRARY", "FOLDERS".
    pub sections_open: std::collections::HashMap<&'static str, bool>,
    /// Double-click detection for result rows.
    pub entry_double_click: crate::widgets::double_click::DoubleClick,
    /// Colour tags of the library, seven at first. Persisted in ui.json.
    pub tags: Vec<TagDef>,
    /// Tag folders tree (top level). Persisted.
    pub tag_tree: Vec<TagFolderNode>,
    /// Tag ids sitting at the top level (not inside any folder).
    pub top_tag_ids: Vec<u32>,
    /// Per-item tagging, keyed by absolute path. Persisted.
    pub item_tags: std::collections::HashMap<String, Vec<u32>>,
    /// Active footer-dot tag filters (any-of); empty shows everything.
    pub tag_filters: Vec<u32>,
    /// Whether the results pane shows the list or the sample map.
    pub results_view: BrowserResultsView,
    /// The sample map's legend filter: `Some(kind)` dims every other kind.
    pub sample_map_kind_filter: Option<SampleMapKind>,
    /// Expanded tag folders in the sidebar.
    pub expanded_tag_folders: std::collections::HashSet<u32>,
    /// Inline rename shown inside a tag context menu (transient).
    pub renaming_tag: Option<u32>,
    pub renaming_folder: Option<u32>,
    pub rename_text: String,
    /// Two-step remove confirmation inside the tag context menus.
    pub confirm_remove_tag: Option<u32>,
    pub confirm_remove_folder: Option<u32>,
    /// Decoded audio used only for the selected Browser source's visual
    /// waveform. Audition still travels through the existing engine path.
    pub waveform_source: Option<MediaSourceRef>,
    pub waveform_audio: Option<Arc<DecodedAudio>>,
    pub waveform_loop_fit: Option<crate::warp::LoopGridFit>,
    pub waveform_loading: bool,
    pub waveform_error: Option<String>,
    /// Decoded previews of recently selected local files, so selecting one
    /// again neither decodes nor re-analyses it. Bounded by entry count
    /// and total cached frames.
    pub preview_cache: HashMap<PathBuf, Arc<crate::message::AnalysedBrowserAudio>>,
    pub preview_cache_order: std::collections::VecDeque<PathBuf>,
    pub audition_enabled: bool,
    pub audition_gain: f32,
    pub audition_loading: bool,
    pub audition_playing: bool,
    pub audition_queued: bool,
    pub audition_position_frames: u64,
    /// Treatment currently handed to the Audition Bus. This can be RAW while
    /// the selected intent remains WARP and its source BPM is unresolved.
    pub audition_playback_mode: Option<AuditionMode>,
    /// Monotonic token minted by [`Self::begin_audition_load`] and
    /// invalidated by [`Self::cancel_audition_requests`]; async decode
    /// and WARP completions carry it so stale results never start
    /// playback after the user stopped or superseded the request.
    pub audition_generation: u64,
    /// The audio the Audition Bus plays, and the buffers its fading voices
    /// may still hold.
    pub audition_audio: super::EngineHeld<DecodedAudio, 4>,
    /// The selected MIDI file's notes, shown in place of a waveform.
    pub midi_preview: Option<crate::domains::midi_audition::MidiPreview>,
    /// The MIDI file the Audition Bus plays, and the ones it may still hold.
    pub audition_pattern: super::EngineHeld<vibez_engine::midi_audition::MidiAuditionPattern, 4>,
    /// The MIDI file last handed to the Audition Bus.
    pub midi_audition: Option<MidiAuditionStart>,
    pub audition_mode: AuditionMode,
    pub scan_in_progress: bool,
    pub mode: SampleBrowserMode,
    /// The device category shown in the results pane instead of samples;
    /// `None` while browsing samples.
    pub category: Option<BrowserCategory>,
    pub pending_drag: Option<PendingMediaDrag>,
    pub drag_source: Option<MediaSourceRef>,
    /// The other selected rows of a multi-row drag (primary is drag_source).
    pub drag_extra: Vec<MediaSourceRef>,
    /// A press on a device row that has not moved far enough to be a drag.
    pub pending_taggable: Option<PendingTaggableDrag>,
    /// The device (instrument / effect / plug-in) an active drag carries.
    pub tag_drag: Option<BrowserTaggable>,
    /// Transient: the track a multi-row empty-area drop's group created, so
    /// the group's other clips land on it instead of making new tracks.
    pub new_track_group: Option<(u64, vibez_core::id::TrackId)>,
    pub drag_label: Option<String>,
    pub drag_target: Option<BrowserDropTarget>,
    /// Set while the drag in flight is a file from another app.
    pub desktop_drop: Option<DesktopDrop>,
    /// What the drag in flight has read of its file, kept for the drag's
    /// life so the ghost never reads it twice.
    pub ghost_cache: Option<crate::domains::drop_ghost::GhostCache>,
}

/// Where a file dragged in from another app is in its drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopDrop {
    /// Over the window; the pointer is the other app's until the drop.
    Hovering,
    /// Over the window, and the pointer has reported where it is, so a drop
    /// with no target under it lands nowhere.
    PointerSeen,
    /// Dropped before any target was under the pointer, so the next target
    /// the pointer reports takes it.
    Dropped,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            open: true,
            keyboard_focus: false,
            dock_width: BROWSER_DOCK_DEFAULT_WIDTH,
            dock_resize_active: false,
            search: String::new(),
            roots: Vec::new(),
            entries: Vec::new(),
            folders: Vec::new(),
            catalog_revision: 0,
            local_results_cache: std::cell::RefCell::new(LocalResults::default()),
            current_folder: None,
            expanded_local_folders: HashSet::new(),
            search_scope: BrowserSearchScope::default(),
            results_visible_limit: BROWSER_RESULTS_PAGE_SIZE,
            root_catalog_states: HashMap::new(),
            root_refresh_revisions: HashMap::new(),
            root_watch_errors: HashMap::new(),
            scan_warnings: Vec::new(),
            scan_error: None,
            selected_source: None,
            selected_extra: std::collections::HashSet::new(),
            special_place: None,
            pins: Vec::new(),
            recent: Vec::new(),
            hovered_source: None,
            sections_open: [
                ("PINNED", true),
                ("TAGS", true),
                ("LIBRARY", true),
                ("FOLDERS", true),
            ]
            .into_iter()
            .collect(),
            entry_double_click: Default::default(),
            tags: Self::default_tags(),
            tag_tree: Vec::new(),
            top_tag_ids: (1..=7).collect(),
            item_tags: std::collections::HashMap::new(),
            tag_filters: Vec::new(),
            results_view: BrowserResultsView::List,
            sample_map_kind_filter: None,
            expanded_tag_folders: std::collections::HashSet::new(),
            renaming_tag: None,
            renaming_folder: None,
            rename_text: String::new(),
            confirm_remove_tag: None,
            confirm_remove_folder: None,
            waveform_source: None,
            waveform_audio: None,
            waveform_loop_fit: None,
            waveform_loading: false,
            waveform_error: None,
            preview_cache: HashMap::new(),
            preview_cache_order: std::collections::VecDeque::new(),
            audition_enabled: true,
            audition_gain: 1.0,
            audition_loading: false,
            audition_playing: false,
            audition_queued: false,
            audition_position_frames: 0,
            audition_playback_mode: None,
            audition_generation: 0,
            audition_audio: super::EngineHeld::default(),
            midi_preview: None,
            audition_pattern: super::EngineHeld::default(),
            midi_audition: None,
            audition_mode: AuditionMode::default(),
            scan_in_progress: false,
            mode: SampleBrowserMode::default(),
            category: None,
            pending_drag: None,
            drag_source: None,
            drag_extra: Vec::new(),
            pending_taggable: None,
            tag_drag: None,
            new_track_group: None,
            drag_label: None,
            drag_target: None,
            desktop_drop: None,
            ghost_cache: None,
        }
    }
}

impl BrowserState {
    pub fn begin_root_scan(&mut self, root: &Path, from_watcher: bool) -> u64 {
        let revision = *self
            .root_refresh_revisions
            .entry(root.to_path_buf())
            .and_modify(|revision| *revision = revision.saturating_add(1))
            .or_insert(1);
        self.root_catalog_states.insert(
            root.to_path_buf(),
            if from_watcher {
                LocalRootCatalogState::Updating
            } else {
                LocalRootCatalogState::Indexing
            },
        );
        self.refresh_scan_diagnostics();
        revision
    }

    pub fn root_refresh_is_current(&self, root: &Path, revision: u64) -> bool {
        self.roots.iter().any(|configured| configured == root)
            && self.root_refresh_revisions.get(root).copied() == Some(revision)
    }

    pub fn root_catalog_label(&self, root: &Path) -> &'static str {
        if self.root_watch_errors.contains_key(root) {
            "WATCH ERR"
        } else {
            self.root_catalog_states
                .get(root)
                .map(LocalRootCatalogState::label)
                .unwrap_or("PENDING")
        }
    }

    pub fn root_catalog_message(&self, root: &Path) -> Option<String> {
        if let Some(error) = self.root_watch_errors.get(root) {
            return Some(format!("WATCH ERROR · {error}"));
        }
        match self.root_catalog_states.get(root) {
            Some(LocalRootCatalogState::Indexing) => Some("INDEXING LOCAL ROOT…".into()),
            Some(LocalRootCatalogState::Updating) => Some("UPDATING LOCAL ROOT…".into()),
            Some(LocalRootCatalogState::Ready { warnings }) if !warnings.is_empty() => {
                Some(format!("WARN {} · {}", warnings.len(), warnings[0]))
            }
            Some(LocalRootCatalogState::Stale { error }) => {
                Some(format!("STALE · {error} · RESCAN TO REPAIR"))
            }
            _ => None,
        }
    }

    pub fn refresh_scan_diagnostics(&mut self) {
        // Roll up per-root states in configured-root order so the
        // global label and surfaced error are deterministic (the
        // backing map iterates in arbitrary order).
        let states: Vec<&LocalRootCatalogState> = self
            .roots
            .iter()
            .filter_map(|root| self.root_catalog_states.get(root))
            .collect();
        self.scan_in_progress = states.iter().any(|state| state.is_busy());
        self.scan_warnings = states
            .iter()
            .filter_map(|state| match state {
                LocalRootCatalogState::Ready { warnings } => Some(warnings.as_slice()),
                _ => None,
            })
            .flatten()
            .cloned()
            .collect();
        self.scan_error = states.iter().find_map(|state| match state {
            LocalRootCatalogState::Stale { error } => Some(error.clone()),
            _ => None,
        });
    }

    pub fn reset_results_window(&mut self) {
        self.results_visible_limit = BROWSER_RESULTS_PAGE_SIZE;
    }

    /// Record that `entries`/`folders` changed, invalidating any
    /// memoized result lists.
    pub fn bump_catalog_revision(&mut self) {
        self.catalog_revision = self.catalog_revision.wrapping_add(1);
    }

    pub fn select_local_folder(&mut self, folder: Option<PathBuf>) {
        self.current_folder = folder;
        self.category = None;
        if let Some(folder) = &self.current_folder {
            self.expanded_local_folders.insert(folder.clone());
        }
        self.search_scope = BrowserSearchScope::SelectedFolder;
        self.reset_results_window();
    }

    pub fn current_local_root(&self) -> Option<&PathBuf> {
        let current = self.current_folder.as_ref()?;
        self.roots
            .iter()
            .filter(|root| current.starts_with(root))
            .max_by_key(|root| root.components().count())
    }

    pub fn search_scope_path(&self) -> Option<&std::path::Path> {
        match self.search_scope {
            BrowserSearchScope::SelectedFolder => self.current_folder.as_deref(),
            BrowserSearchScope::Root => self.current_local_root().map(PathBuf::as_path),
            BrowserSearchScope::Everywhere => None,
        }
    }

    pub fn search_scope_label(&self) -> &'static str {
        match self.search_scope {
            BrowserSearchScope::SelectedFolder if self.current_folder.is_none() => "EVERYWHERE",
            BrowserSearchScope::SelectedFolder
                if self.current_folder.as_ref() == self.current_local_root() =>
            {
                "THIS ROOT"
            }
            BrowserSearchScope::SelectedFolder => "THIS FOLDER",
            BrowserSearchScope::Root => "THIS ROOT",
            BrowserSearchScope::Everywhere => "EVERYWHERE",
        }
    }

    pub fn cycle_search_scope(&mut self) {
        self.search_scope = match self.search_scope {
            BrowserSearchScope::SelectedFolder
                if self.current_folder.is_some()
                    && self.current_folder.as_ref() != self.current_local_root() =>
            {
                BrowserSearchScope::Root
            }
            BrowserSearchScope::SelectedFolder | BrowserSearchScope::Root => {
                BrowserSearchScope::Everywhere
            }
            BrowserSearchScope::Everywhere if self.current_folder.is_some() => {
                BrowserSearchScope::SelectedFolder
            }
            BrowserSearchScope::Everywhere => BrowserSearchScope::Everywhere,
        };
        self.reset_results_window();
    }

    pub fn path_is_in_search_scope(&self, path: &std::path::Path) -> bool {
        self.search_scope_path()
            .is_none_or(|scope| path.starts_with(scope))
    }

    pub fn local_folder_is_result(
        &self,
        folder: &SampleBrowserFolder,
        normalized_query: &str,
    ) -> bool {
        if normalized_query.is_empty() {
            return self
                .current_folder
                .as_deref()
                .is_some_and(|current| folder.path.parent() == Some(current));
        }
        self.path_is_in_search_scope(&folder.path) && folder.search_text.contains(normalized_query)
    }

    pub fn local_entry_is_result(
        &self,
        entry: &SampleBrowserEntry,
        normalized_query: &str,
    ) -> bool {
        let path = entry.root_path.join(&entry.relative_path);
        if normalized_query.is_empty() {
            return self
                .current_folder
                .as_deref()
                .is_some_and(|current| path.parent() == Some(current));
        }
        self.path_is_in_search_scope(&path) && entry.search_text.contains(normalized_query)
    }

    pub fn visible_result_count(&self, total: usize) -> usize {
        total.min(self.results_visible_limit)
    }

    /// Root and folder rows shown above the media rows for `query`
    /// (already the visible window the media limit subtracts from).
    pub fn visible_folder_row_count(&self, query: &str) -> usize {
        let searching = !query.is_empty();
        let root_results = if !searching && self.current_folder.is_none() {
            self.roots.len()
        } else if searching && self.search_scope_path().is_none() {
            self.roots
                .iter()
                .filter(|root| root.display().to_string().to_lowercase().contains(query))
                .count()
        } else {
            0
        };
        let folder_results = self
            .folders
            .iter()
            .filter(|folder| self.local_folder_is_result(folder, query))
            .count();
        root_results + folder_results
    }

    pub fn has_more_results(&self, total: usize) -> bool {
        self.results_visible_limit < total
    }

    pub fn select_source(&mut self, source: MediaSourceRef) -> bool {
        let changed = self.selected_source.as_ref() != Some(&source);
        self.selected_source = Some(source);
        if changed {
            self.clear_waveform();
        }
        changed
    }

    pub fn clear_selection(&mut self) {
        self.selected_source = None;
        self.selected_extra.clear();
        self.clear_waveform();
    }

    /// Whether `source` is part of the current selection.
    pub fn is_selected(&self, source: &MediaSourceRef) -> bool {
        self.selected_source.as_ref() == Some(source) || self.selected_extra.contains(source)
    }

    /// Every selected source: the primary one first, then the extras in
    /// list order. Empty when nothing is selected.
    pub fn selection_sources(&self, ordered_results: &[MediaSourceRef]) -> Vec<MediaSourceRef> {
        let mut out = Vec::new();
        if let Some(primary) = &self.selected_source {
            out.push(primary.clone());
        }
        for source in ordered_results {
            if self.selected_extra.contains(source) && Some(source) != self.selected_source.as_ref()
            {
                out.push(source.clone());
            }
        }
        out
    }

    pub fn selection_count(&self) -> usize {
        self.selected_extra.len() + usize::from(self.selected_source.is_some())
    }

    pub fn is_pinned(&self, source: &MediaSourceRef) -> bool {
        self.pins.contains(source)
    }

    /// Pin or unpin `source`; returns the new pinned state.
    pub fn toggle_pin(&mut self, source: MediaSourceRef) -> bool {
        if let Some(index) = self.pins.iter().position(|pinned| pinned == &source) {
            self.pins.remove(index);
            false
        } else {
            self.pins.push(source);
            true
        }
    }

    /// Record `source` as recently used (imported into the project),
    /// newest first, deduplicated, capped at 50.
    pub fn push_recent(&mut self, source: MediaSourceRef) {
        self.recent.retain(|recent| recent != &source);
        self.recent.insert(0, source);
        self.recent.truncate(BROWSER_RECENT_CAP);
    }

    /// Show a special place in the results pane, leaving any folder or
    /// category. `None` goes back to folders/categories.
    pub fn select_special_place(&mut self, place: Option<SpecialPlace>) {
        if place.is_some() {
            self.current_folder = None;
            self.category = None;
            self.results_view = BrowserResultsView::List;
        }
        self.special_place = place;
    }

    pub fn section_open(&self, section: &'static str) -> bool {
        self.sections_open.get(section).copied().unwrap_or(true)
    }

    pub fn toggle_section(&mut self, section: &'static str) {
        let open = self.section_open(section);
        self.sections_open.insert(section, !open);
    }

    /// Sources a special place lists: pins in pin order, recent newest
    /// first. Missing files are kept (shown as missing), not dropped.
    pub fn special_place_sources(&self, place: &SpecialPlace) -> Vec<MediaSourceRef> {
        match place {
            SpecialPlace::Pinned => self.pins.clone(),
            SpecialPlace::Recent => self.recent.clone(),
            SpecialPlace::Tags(ids) => self.items_with_any_tags(ids),
        }
    }

    /// Resolve a special-place source against the catalog: the indexed
    /// entry when the file is still there, else a bare reference shown as
    /// missing.
    pub fn entry_for_source(&self, source: &MediaSourceRef) -> Option<&SampleBrowserEntry> {
        self.entries.iter().find(|entry| &entry.source == source)
    }

    pub fn begin_waveform_load(&mut self, source: &MediaSourceRef) {
        if self.selected_source.as_ref() == Some(source) {
            self.waveform_loading = true;
        }
    }

    /// Mint a fresh audition request token; any older in-flight decode
    /// or WARP preparation becomes stale.
    pub fn begin_audition_load(&mut self, source: &MediaSourceRef) -> u64 {
        self.begin_waveform_load(source);
        if self.selected_source.as_ref() == Some(source) {
            self.audition_loading = true;
        }
        self.audition_generation = self.audition_generation.wrapping_add(1);
        self.audition_generation
    }

    /// Start preparing an alternate treatment for an already decoded source.
    /// Unlike [`Self::begin_audition_load`], this does not mark the selected
    /// waveform as loading again.
    pub fn begin_audition_preparation(&mut self, source: &MediaSourceRef) -> u64 {
        if self.selected_source.as_ref() == Some(source) {
            self.audition_loading = true;
        }
        self.audition_generation = self.audition_generation.wrapping_add(1);
        self.audition_generation
    }

    pub fn audition_request_is_current(&self, generation: u64) -> bool {
        self.audition_generation == generation
    }

    /// Invalidate only background decode/WARP preparation. Unlike an explicit
    /// Stop this deliberately leaves the currently audible voice alone.
    pub fn cancel_audition_preparation(&mut self) {
        self.audition_generation = self.audition_generation.wrapping_add(1);
        self.audition_loading = false;
    }

    /// Explicit user cancellation: clears audition state and
    /// invalidates every in-flight audition request token.
    pub fn cancel_audition_requests(&mut self) {
        self.audition_generation = self.audition_generation.wrapping_add(1);
        self.stop_audition_state();
    }

    pub fn install_audition(
        &mut self,
        generation: u64,
        source: MediaSourceRef,
        analysed: crate::message::AnalysedBrowserAudio,
    ) -> bool {
        if !self.audition_request_is_current(generation) {
            return false;
        }
        if !self.install_waveform(source, analysed) {
            return false;
        }
        self.audition_loading = false;
        true
    }

    pub fn stop_audition_state(&mut self) {
        self.audition_loading = false;
        self.audition_playing = false;
        self.audition_queued = false;
        self.audition_position_frames = 0;
        self.audition_playback_mode = None;
    }

    pub fn toggle_audition_enabled(&mut self) -> bool {
        self.audition_enabled = !self.audition_enabled;
        self.audition_enabled
    }

    pub fn set_audition_gain(&mut self, gain: f32) {
        self.audition_gain = gain.clamp(0.0, 2.0);
    }

    pub fn mark_audition_requested(&mut self, queued: bool, mode: AuditionMode) {
        self.audition_loading = false;
        self.audition_queued = queued;
        self.audition_playing = !queued;
        self.audition_position_frames = 0;
        self.audition_playback_mode = Some(mode);
    }

    pub fn audition_playhead_fraction(&self) -> Option<f32> {
        let frames = self.audition_audio.current()?.num_frames();
        (self.audition_playing && frames > 0)
            .then(|| (self.audition_position_frames as f64 / frames as f64).clamp(0.0, 1.0) as f32)
    }

    /// The footer waveform's playhead: the playing audition's position, or
    /// the position dragged while stopped, relative to the shown audio.
    pub fn preview_playhead_fraction(&self) -> Option<f32> {
        if let Some(fraction) = self.audition_playhead_fraction() {
            return Some(fraction);
        }
        let audio = self.waveform_audio.as_ref()?;
        let frames = audio.num_frames();
        (frames > 0 && self.audition_position_frames > 0)
            .then(|| (self.audition_position_frames as f64 / frames as f64).clamp(0.0, 1.0) as f32)
    }

    pub fn audition_status(&self) -> AuditionStatus {
        if self.audition_loading
            && self.audition_playing
            && self.audition_playback_mode == Some(AuditionMode::Raw)
        {
            AuditionStatus::Warping
        } else if self.audition_loading {
            AuditionStatus::Preparing
        } else if self.audition_queued {
            AuditionStatus::Queued
        } else if self.audition_playing {
            match self.audition_playback_mode {
                Some(AuditionMode::Raw) if self.audition_mode == AuditionMode::Warp => {
                    AuditionStatus::Warping
                }
                mode => AuditionStatus::Playing(mode),
            }
        } else if self.waveform_error.is_some() {
            AuditionStatus::Unavailable
        } else if self.waveform_audio.is_some() || self.midi_preview.is_some() {
            AuditionStatus::Ready(self.audition_mode)
        } else {
            AuditionStatus::Select
        }
    }

    /// Whether the selection is a local MIDI file, shown as notes rather
    /// than a waveform.
    pub fn selected_is_midi(&self) -> bool {
        self.mode == SampleBrowserMode::Local
            && self.selected_source.as_ref().is_some_and(|source| {
                crate::domains::browser::DragMedia::of(source)
                    == crate::domains::browser::DragMedia::Midi
            })
    }

    /// The track that plays the shown MIDI file, given the MIDI target a new
    /// audition would play on.
    pub fn midi_audition_player(&self, target: Option<TrackId>) -> Option<TrackId> {
        match &self.midi_audition {
            Some(started)
                if (self.audition_playing || self.audition_queued) && self.shows_started_midi() =>
            {
                started.target
            }
            _ => target,
        }
    }

    /// Whether the MIDI file last handed to the Audition Bus is the one shown.
    fn shows_started_midi(&self) -> bool {
        self.audition_pattern.current().is_some()
            && self.midi_preview.as_ref().is_some_and(|preview| {
                self.midi_audition
                    .as_ref()
                    .is_some_and(|started| started.source == preview.source)
            })
    }

    /// Where the playhead sits in the MIDI strip, at `samples_per_beat`.
    pub fn midi_audition_playhead(&self, samples_per_beat: f64) -> Option<f32> {
        let pattern = self.audition_pattern.current()?;
        if !self.audition_playing || !self.shows_started_midi() {
            return None;
        }
        crate::domains::midi_audition::playhead_fraction(
            pattern,
            self.audition_position_frames,
            samples_per_beat,
            self.audition_playback_mode
                .is_some_and(crate::state::AuditionMode::loops),
        )
    }

    /// Shows `preview` if its file is still the selected one.
    pub fn install_midi_preview(
        &mut self,
        preview: crate::domains::midi_audition::MidiPreview,
    ) -> bool {
        if self.selected_source.as_ref() != Some(&preview.source) {
            return false;
        }

        self.waveform_loading = false;
        self.waveform_error = None;
        self.midi_preview = Some(preview);
        true
    }

    pub fn midi_preview_for(
        &self,
        source: &MediaSourceRef,
    ) -> Option<&crate::domains::midi_audition::MidiPreview> {
        self.midi_preview
            .as_ref()
            .filter(|preview| &preview.source == source)
    }

    pub fn import_input_for_source(&self, source: &MediaSourceRef) -> AuditionImportInput {
        AuditionImportInput {
            mode: self.audition_mode,
            source_bpm: if self.waveform_source.as_ref() == Some(source) {
                self.waveform_loop_fit.map(|fit| fit.source_bpm)
            } else {
                None
            },
        }
    }

    pub fn audition_playback_plan(&self, source: &MediaSourceRef) -> BrowserAuditionPlan {
        match self.audition_mode {
            AuditionMode::Raw => BrowserAuditionPlan::Raw,
            AuditionMode::Warp if self.waveform_source.as_ref() == Some(source) => self
                .waveform_loop_fit
                .map(|fit| BrowserAuditionPlan::Warp {
                    source_bpm: fit.source_bpm,
                })
                .unwrap_or(BrowserAuditionPlan::Raw),
            AuditionMode::Warp => BrowserAuditionPlan::Raw,
        }
    }

    pub fn waveform_for_source(&self, source: &MediaSourceRef) -> Option<Arc<DecodedAudio>> {
        if self.waveform_source.as_ref() != Some(source) {
            return None;
        }
        self.waveform_audio.clone()
    }

    pub fn begin_pending_drag(
        &mut self,
        sources: Vec<MediaSourceRef>,
        label: String,
        origin_x: f32,
        origin_y: f32,
    ) {
        self.cancel_media_drag();
        self.new_track_group = None;
        let mut sources = sources;
        let Some(source) = sources.first().cloned() else {
            return;
        };
        let extra = sources.split_off(1);
        // A multi-row drag is labelled with its count: "kick.wav +2".
        let label = if extra.is_empty() {
            label
        } else {
            format!("{label} +{}", extra.len())
        };
        self.pending_drag = Some(PendingMediaDrag {
            source,
            extra,
            label,
            origin_x,
            origin_y,
        });
    }

    /// Start a pending drag of a device row (instrument, effect, plug-in);
    /// it becomes a tag drag once the pointer moves far enough.
    pub fn begin_pending_taggable_drag(
        &mut self,
        item: BrowserTaggable,
        label: String,
        origin_x: f32,
        origin_y: f32,
    ) {
        self.cancel_media_drag();
        self.pending_taggable = Some(PendingTaggableDrag {
            item,
            label,
            origin_x,
            origin_y,
        });
    }

    /// Past the drag threshold, the pending device drag becomes active.
    pub fn move_pending_taggable_drag(&mut self, x: f32, y: f32) -> bool {
        let Some(pending) = self.pending_taggable.as_ref() else {
            return false;
        };
        let dx = x - pending.origin_x;
        let dy = y - pending.origin_y;
        if dx * dx + dy * dy <= MEDIA_DRAG_THRESHOLD_PX * MEDIA_DRAG_THRESHOLD_PX {
            return false;
        }
        let pending = self.pending_taggable.take().expect("pending taggable drag");
        self.tag_drag = Some(pending.item);
        self.drag_label = Some(pending.label);
        self.drag_target = None;
        true
    }

    /// Whether any drag is in flight, media or device.
    pub fn drag_in_flight(&self) -> bool {
        self.drag_source.is_some() || self.tag_drag.is_some()
    }

    /// Every source the active drag carries, primary first; empty when no
    /// drag is active. Takes the drag's sources but leaves the rest of the
    /// drag state for `cancel_media_drag`.
    pub fn take_drag_sources(&mut self) -> Vec<MediaSourceRef> {
        let mut sources = Vec::new();
        if let Some(source) = self.drag_source.take() {
            sources.push(source);
        }
        sources.append(&mut self.drag_extra);
        sources
    }

    /// A catalog entry's duration in samples at `sample_rate`, when known;
    /// used to place a multi-row drop's clips one after another.
    pub fn entry_duration_samples(&self, source: &MediaSourceRef, sample_rate: u32) -> u64 {
        let seconds = self
            .entry_for_source(source)
            .and_then(|entry| entry.duration_seconds)
            .unwrap_or(0.0);
        if sample_rate == 0 || !(seconds.is_finite() && seconds > 0.0) {
            return 0;
        }
        (seconds * sample_rate as f64) as u64
    }

    pub fn move_pending_drag(&mut self, x: f32, y: f32) -> bool {
        let Some(pending) = self.pending_drag.as_ref() else {
            return false;
        };
        let dx = x - pending.origin_x;
        let dy = y - pending.origin_y;
        if dx * dx + dy * dy <= MEDIA_DRAG_THRESHOLD_PX * MEDIA_DRAG_THRESHOLD_PX {
            return false;
        }
        let pending = self.pending_drag.take().expect("pending drag exists");
        self.drag_source = Some(pending.source);
        self.drag_extra = pending.extra;
        self.drag_label = Some(pending.label);
        self.drag_target = None;
        true
    }

    pub fn cancel_pending_drag(&mut self) {
        self.pending_drag = None;
        self.pending_taggable = None;
    }

    pub fn cancel_media_drag(&mut self) {
        self.pending_drag = None;
        self.pending_taggable = None;
        self.tag_drag = None;
        self.drag_source = None;
        self.drag_extra.clear();
        self.drag_label = None;
        self.drag_target = None;
        self.desktop_drop = None;
        self.ghost_cache = None;
    }

    /// The drag's name, how it imports and its musical length, as the lane
    /// it hovers previews it.
    pub fn drag_detail(&self, project_bpm: f64) -> Option<String> {
        let label = self.drag_label.as_ref()?;
        if self.drag_media() == Some(crate::domains::browser::DragMedia::Midi) {
            return Some(format!("{label} · MIDI"));
        }
        let mode = match self.audition_mode {
            AuditionMode::Raw => "RAW",
            AuditionMode::Warp => "WARP",
        };
        Some(match self.drag_preview_beats(project_bpm) {
            Some(beats) => format!("{label} · {mode} · {beats:.2} beats"),
            None => format!("{label} · {mode}"),
        })
    }

    pub fn drag_preview_beats(&self, project_bpm: f64) -> Option<f64> {
        let source = self.drag_source.as_ref()?;
        if self.waveform_source.as_ref()? != source {
            return None;
        }
        let audio = self.waveform_audio.as_ref()?;
        if audio.sample_rate == 0 || audio.num_frames() == 0 {
            return None;
        }
        let seconds = audio.num_frames() as f64 / audio.sample_rate as f64;
        self.audition_mode
            .imported_beats(seconds, self.waveform_loop_fit, project_bpm)
    }

    /// Decoded preview for a local file decoded recently, if still cached.
    pub fn cached_preview(
        &self,
        path: &std::path::Path,
    ) -> Option<crate::message::AnalysedBrowserAudio> {
        self.preview_cache
            .get(path)
            .map(|cached| (**cached).clone())
    }

    /// Keeps a decoded preview for its file, evicting the oldest entries
    /// past the entry-count or total-frames budget.
    pub fn remember_preview(
        &mut self,
        path: PathBuf,
        analysed: crate::message::AnalysedBrowserAudio,
    ) {
        const MAX_ENTRIES: usize = 6;
        const MAX_FRAMES: u64 = 30_000_000;
        self.preview_cache_order.retain(|entry| *entry != path);
        self.preview_cache_order.push_back(path.clone());
        self.preview_cache.insert(path, Arc::new(analysed));
        let mut total: u64 = self
            .preview_cache
            .values()
            .map(|cached| cached.audio.num_frames() as u64)
            .sum();
        while (self.preview_cache.len() > MAX_ENTRIES || total > MAX_FRAMES)
            && self.preview_cache_order.len() > 1
        {
            let Some(oldest) = self.preview_cache_order.pop_front() else {
                break;
            };
            if let Some(removed) = self.preview_cache.remove(&oldest) {
                total = total.saturating_sub(removed.audio.num_frames() as u64);
            }
        }
    }

    pub fn install_waveform(
        &mut self,
        source: MediaSourceRef,
        analysed: crate::message::AnalysedBrowserAudio,
    ) -> bool {
        if self.selected_source.as_ref() != Some(&source) {
            return false;
        }
        if let MediaSourceRef::LocalFile { path } = &source {
            self.remember_preview(path.clone(), analysed.clone());
        }
        let channels = analysed.audio.num_channels();
        let sample_rate = analysed.audio.sample_rate;
        let duration_seconds = if sample_rate > 0 {
            Some(analysed.audio.num_frames() as f64 / sample_rate as f64)
        } else {
            None
        };
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.source == source) {
            entry.duration_seconds = duration_seconds;
            entry.channels = Some(channels);
            entry.sample_rate = Some(sample_rate);
        }
        self.waveform_source = Some(source);
        self.waveform_audio = Some(analysed.audio);
        self.waveform_loop_fit = analysed.loop_fit;
        self.waveform_loading = false;
        self.waveform_error = None;
        true
    }

    /// Stores the preview's asynchronously analysed loop fit.
    pub fn set_waveform_loop_fit(&mut self, fit: Option<crate::warp::LoopGridFit>) {
        self.waveform_loop_fit = fit;
    }

    /// Keeps a cached preview's loop fit in step with the shown waveform.
    pub fn update_cached_preview_fit(
        &mut self,
        path: &std::path::Path,
        fit: Option<crate::warp::LoopGridFit>,
    ) {
        if let Some(cached) = self.preview_cache.get_mut(path) {
            Arc::make_mut(cached).loop_fit = fit;
        }
    }

    pub fn fail_waveform_load(&mut self, source: &MediaSourceRef, error: String) {
        if self.selected_source.as_ref() == Some(source) {
            self.waveform_loading = false;
            self.waveform_error = Some(error);
        }
    }

    fn clear_waveform(&mut self) {
        self.midi_preview = None;
        self.waveform_source = None;
        self.waveform_audio = None;
        self.waveform_loop_fit = None;
        self.waveform_loading = false;
        self.waveform_error = None;
    }

    pub fn set_dock_width(&mut self, width: f32) {
        self.dock_width = width.clamp(BROWSER_DOCK_MIN_WIDTH, BROWSER_DOCK_MAX_WIDTH);
    }

    /// Width a live splitter drag should store: unlike a preference
    /// restored from settings, a drag also respects the window cap
    /// applied by [`Self::effective_dock_width`], so the handle keeps
    /// tracking the cursor instead of freezing at the yield point.
    pub fn dock_drag_width(&self, cursor_x: f32, window_width: f32) -> f32 {
        crate::shell_layout::browser_width_at(cursor_x)
            .min(window_width - ARRANGE_MIN_WIDTH_WITH_BROWSER)
    }

    /// Whether keyboard shortcuts act on the Browser: it is open and the
    /// producer last worked in it.
    pub fn has_keyboard_focus(&self) -> bool {
        self.open && self.keyboard_focus
    }

    pub fn effective_dock_width(&self, window_width: f32) -> f32 {
        let available = (window_width - ARRANGE_MIN_WIDTH_WITH_BROWSER)
            .clamp(BROWSER_DOCK_MIN_WIDTH, BROWSER_DOCK_MAX_WIDTH);
        self.dock_width.min(available).max(BROWSER_DOCK_MIN_WIDTH)
    }

    pub fn places_pane_width(&self, window_width: f32) -> f32 {
        (self.effective_dock_width(window_width) * 0.36)
            .clamp(BROWSER_PLACES_MIN_WIDTH, BROWSER_PLACES_MAX_WIDTH)
    }

    /// The single Results table keeps Name and Status visible throughout the
    /// resize range, then promotes BPM and Length into dedicated columns once
    /// the Results pane has enough room to keep every column readable.
    pub fn results_use_wide_columns(&self, window_width: f32) -> bool {
        let results_width =
            self.effective_dock_width(window_width) - self.places_pane_width(window_width);
        results_width >= 400.0
    }
}
