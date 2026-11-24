//! Media, sample-browser, and engine-event message handlers.
//! Split from update.rs; each method backs one arm of the App::update match.

use std::path::PathBuf;
use std::sync::Arc;

use iced::Task;

use crate::domains::arrangement::ArrangementMsg;
use crate::domains::browser::{BrowserMsg, DragMedia};
use crate::domains::device_panel_drop::DevicePanelDrop;
use crate::domains::piano_roll::PianoRollMsg;
use crate::domains::view::ViewMsg;
use vibez_core::id::{ClipId, TrackId};
use vibez_core::track::MediaSourceRef;

use crate::message::{BrowserImportTarget, Message};

use super::*;

impl App {
    fn text_field_editing(&self) -> bool {
        self.state.view.editing_track_name.is_some()
            || self.state.view.editing_clip_name.is_some()
            || self.state.perform.editing_section_name.is_some()
    }

    /// Delete/Backspace, resolved against whatever the user is actually
    /// editing.
    ///
    /// The priority ladder below reads the focused timeline, so the focus gate
    /// has to come first: in Perform the Section lanes stay on screen next to
    /// the pad surface, and a producer hitting Delete over the pads is not
    /// asking to destroy the clips they can see beside them.
    pub(super) fn on_delete_key_pressed(&mut self) -> Task<Message> {
        // Never delete anything while a text field is being
        // edited; backspace belongs to the text there.
        if self.text_field_editing() {
            return Task::none();
        }
        if !self.editor_shortcuts_have_focus() {
            return Task::none();
        }
        // Priority 1: a selected automation point.
        if self.state.automation_ui.selected.is_some() {
            return self.update(Message::Automation(
                crate::domains::automation::AutomationMsg::DeleteSelectedPoint,
            ));
        }
        // Priority 2: the selected Audio Clip marker.
        if let Some((track_id, clip_id, source_frame)) =
            self.state.active_timeline_editor().selected_warp_marker
        {
            return self.update(Message::Arrangement(ArrangementMsg::RemoveWarpMarker {
                track_id,
                clip_id,
                source_frame,
            }));
        }
        if let Some((track_id, clip_id, source_frame)) = self
            .state
            .active_timeline_editor()
            .selected_transient_marker
        {
            return self.update(Message::Arrangement(
                ArrangementMsg::RemoveTransientMarker {
                    track_id,
                    clip_id,
                    source_frame,
                },
            ));
        }
        // Priority 3: selected notes in the open piano roll.
        if let Some((track_id, clip_id)) = self.state.active_timeline_editor().selected_note_clip {
            let has_selection = self
                .state
                .active_timeline_content(track_id)
                .and_then(|content| content.note_clips.iter().find(|c| c.id == clip_id))
                .is_some_and(|c| !c.selected_notes.is_empty());
            if has_selection {
                return self.update(Message::PianoRoll(PianoRollMsg::RemoveSelectedNotes(
                    track_id, clip_id,
                )));
            }
        }
        // Priority 4: selected arrangement clips.
        if !self
            .state
            .active_timeline_editor()
            .selected_clips
            .is_empty()
        {
            return self.update(Message::Arrangement(ArrangementMsg::DeleteSelectedClip));
        }
        Task::none()
    }

    /// Command+D, resolved against whatever the user is actually editing.
    ///
    /// Mirrors [`Self::on_delete_key_pressed`]: the focus gate first, then
    /// the selected notes of the open piano roll, then the selected clips.
    pub(super) fn on_duplicate_pressed(&mut self) -> Task<Message> {
        // A text field owns Command+D for its own purposes.
        if self.text_field_editing() {
            return Task::none();
        }
        // Priority 1: selected notes in the open piano roll.
        if let Some((track_id, clip_id)) = self.state.active_timeline_editor().selected_note_clip {
            let has_selection = self
                .state
                .active_timeline_content(track_id)
                .and_then(|content| content.note_clips.iter().find(|c| c.id == clip_id))
                .is_some_and(|c| !c.selected_notes.is_empty());
            if has_selection {
                return self.update(Message::PianoRoll(PianoRollMsg::DuplicateSelectedNotes {
                    track_id,
                    clip_id,
                }));
            }
        }
        // Priority 2: selected arrangement clips.
        if !self
            .state
            .active_timeline_editor()
            .selected_clips
            .is_empty()
        {
            return self.update(Message::Arrangement(ArrangementMsg::DuplicateSelectedClip));
        }
        Task::none()
    }
    /// Command+A, resolved against whatever the user is actually editing.
    ///
    /// Mirrors [`Self::on_delete_key_pressed`]: the focus gate first, then the
    /// explicitly opened piano roll, then the focused timeline's clips. Unlike
    /// Delete this cannot key off an existing selection, since the point of
    /// the shortcut is to create one, so it asks which editor was last opened
    /// rather than which one holds a selection. Selecting everything must not
    /// escalate the target on a second press.
    pub(super) fn on_select_all_pressed(&mut self) -> Task<Message> {
        // A text field owns Command+A for its own contents.
        if self.text_field_editing() {
            return Task::none();
        }
        if self.state.view.workspace == crate::state::Workspace::Mix {
            return self.update(Message::Arrangement(ArrangementMsg::SelectAllChannels));
        }
        if !self.editor_shortcuts_have_focus() {
            return Task::none();
        }

        if let Some((track_id, clip_id)) = self.focused_piano_roll_clip() {
            return self.update(Message::PianoRoll(PianoRollMsg::SelectAllNotes(
                track_id, clip_id,
            )));
        }

        self.update(Message::Arrangement(ArrangementMsg::SelectAllClips))
    }

    pub(super) fn on_load_sampler_sample(&mut self, track_id: TrackId) -> Task<Message> {
        Task::perform(
            async {
                let handle = rfd::AsyncFileDialog::new()
                    .set_title("Load Sample")
                    .add_filter(
                        "Supported Audio",
                        vibez_core::audio_format::SUPPORTED_AUDIO_EXTENSIONS,
                    )
                    .pick_file()
                    .await;
                handle.map(|h| h.path().to_path_buf())
            },
            move |path| Message::SamplerFileSelected(track_id, path),
        )
    }

    pub(super) fn on_sampler_file_selected(
        &mut self,
        track_id: TrackId,
        path: Option<PathBuf>,
    ) -> Task<Message> {
        if let Some(path) = path {
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            self.state.status_text = format!("Loading {file_name}...");
            return Task::perform(
                decode_and_stage_local_async(path, self.state.transport.sample_rate),
                move |result| match result {
                    Ok((audio, source)) => Message::SamplerSampleDecoded(
                        track_id,
                        Arc::new(audio),
                        file_name.clone(),
                        source,
                    ),
                    Err(e) => Message::SamplerDecodeError(track_id, e),
                },
            );
        }
        Task::none()
    }

    pub(super) fn on_load_drum_rack_pad_sample(
        &mut self,
        track_id: TrackId,
        pad_index: usize,
    ) -> Task<Message> {
        Task::perform(
            async {
                let handle = rfd::AsyncFileDialog::new()
                    .set_title("Load Drum Pad Sample")
                    .add_filter(
                        "Supported Audio",
                        vibez_core::audio_format::SUPPORTED_AUDIO_EXTENSIONS,
                    )
                    .pick_file()
                    .await;
                handle.map(|h| h.path().to_path_buf())
            },
            move |path| Message::DrumRackPadFileSelected(track_id, pad_index, path),
        )
    }

    pub(super) fn on_rescan_midi_inputs(&mut self) -> Task<Message> {
        match vibez_audio_io::midi_input::list_midi_input_ports() {
            Ok(ports) => {
                self.midi_input_ports = ports;
                self.state.status_text =
                    format!("Found {} MIDI input port(s)", self.midi_input_ports.len());
            }
            Err(err) => {
                self.midi_input_ports.clear();
                self.state.status_text = format!("MIDI scan error: {err}");
            }
        }
        Task::none()
    }

    pub(super) fn on_open_midi_input(&mut self, name: String) -> Task<Message> {
        match vibez_audio_io::midi_input::open_midi_input(&name) {
            Ok(handle) => {
                self.state.status_text = format!("MIDI input: {}", handle.port_name);
                self.midi_input = Some(handle);
                self.persist_ui_settings();
            }
            Err(err) => {
                self.state.status_text = format!("MIDI open error: {err}");
            }
        }
        Task::none()
    }

    pub(super) fn on_select_theme(&mut self, name: String) -> Task<Message> {
        if let Some(palette) = self.resolve_theme(&name) {
            th::set_theme(palette);
            self.state.current_theme_name = name.clone();
            self.persist_ui_settings();
            self.state.status_text = format!("Theme: {name}");
        } else {
            self.state.status_text = format!("Theme {name:?} not found");
        }
        Task::none()
    }

    pub(super) fn on_rescan_themes(&mut self) -> Task<Message> {
        let (themes, warnings) = crate::themes::scan_user_themes();
        let count = themes.len();
        self.state.user_themes = themes;
        self.state.status_text = if warnings.is_empty() {
            format!("{count} user theme(s) found")
        } else {
            format!("{count} user theme(s), {} skipped", warnings.len())
        };
        for warning in warnings {
            eprintln!("vibez: theme scan: {warning}");
        }
        Task::none()
    }

    pub(super) fn on_save_current_theme(&mut self) -> Task<Message> {
        let name = self.state.theme_save_name.trim().to_string();
        if name.is_empty() {
            self.state.status_text = "Name the theme before saving".to_string();
            return Task::none();
        }
        let mut palette = th::current();
        palette.name = name.clone();
        match crate::themes::save_user_theme(&palette) {
            Ok(path) => {
                let (themes, _) = crate::themes::scan_user_themes();
                self.state.user_themes = themes;
                self.state.current_theme_name = name;
                self.state.theme_save_name.clear();
                self.persist_ui_settings();
                self.state.status_text = format!("Theme saved to {}", path.display());
            }
            Err(err) => {
                self.state.status_text = format!("Theme save error: {err}");
            }
        }
        Task::none()
    }

    pub(super) fn on_add_sample_library_root(&mut self) -> Task<Message> {
        Task::perform(
            async {
                let handle = rfd::AsyncFileDialog::new()
                    .set_title("Add Sample Library Root")
                    .pick_folder()
                    .await;
                handle.map(|folder| folder.path().to_path_buf())
            },
            Message::SampleLibraryRootSelected,
        )
    }

    pub(super) fn on_sample_library_root_selected(
        &mut self,
        path: Option<PathBuf>,
    ) -> Task<Message> {
        if let Some(path) = path {
            if !self.state.browser.roots.iter().any(|root| root == &path) {
                self.state.browser.roots.push(path.clone());
                self.state.browser.roots.sort();
                self.persist_ui_settings();
            }
            self.state.browser.select_local_folder(Some(path.clone()));
            let revision = self.state.browser.begin_root_scan(&path, false);
            self.state.status_text = format!("Scanning {}...", path.display());
            return Task::perform(scan_sample_root_async(path.clone()), move |result| {
                Message::Browser(BrowserMsg::LocalRootCatalogReconciled {
                    root: path.clone(),
                    revision,
                    result,
                })
            });
        }
        Task::none()
    }

    pub(super) fn on_rescan_sample_library(&mut self) -> Task<Message> {
        self.state.status_text = "Rescanning sample library...".to_string();
        let roots = self.state.browser.roots.clone();
        Task::batch(roots.into_iter().map(|root| {
            let revision = self.state.browser.begin_root_scan(&root, false);
            Task::perform(scan_sample_root_async(root.clone()), move |result| {
                Message::Browser(BrowserMsg::LocalRootCatalogReconciled {
                    root: root.clone(),
                    revision,
                    result,
                })
            })
        }))
    }

    pub(super) fn on_click_local_browser_entry(&mut self, source: MediaSourceRef) -> Task<Message> {
        if self.state.browser.drag_source.is_some() {
            self.state.browser.cancel_media_drag();
            self.state.status_text = "Drag cancelled".into();
            return Task::none();
        }
        if DragMedia::of(&source) == DragMedia::Midi {
            return self.select_midi_file(source);
        }
        let changed = self.state.browser.select_source(source.clone());
        if changed {
            if let MediaSourceRef::LocalFile { path } = source.clone() {
                if let Some(cached) = self.state.browser.cached_preview(&path) {
                    if self.state.browser.audition_enabled {
                        let generation = self.state.browser.begin_audition_load(&source);
                        return self.update(Message::LocalSamplePreviewReady(
                            source,
                            generation,
                            Ok(cached),
                        ));
                    }
                    self.state.browser.begin_waveform_load(&source);
                    return self.update(Message::BrowserWaveformReady(source, Ok(cached)));
                }
                if self.state.browser.audition_enabled {
                    let generation = self.state.browser.begin_audition_load(&source);
                    self.state.status_text = "Preparing Audition...".to_string();
                    return Task::perform(decode_local_for_preview_async(path), move |result| {
                        Message::LocalSamplePreviewReady(
                            source.clone(),
                            generation,
                            result.map(|(audio, _name)| crate::message::AnalysedBrowserAudio {
                                audio,
                                loop_fit: None,
                            }),
                        )
                    });
                }
                self.state.browser.begin_waveform_load(&source);
                return Task::perform(decode_local_for_preview_async(path), move |result| {
                    Message::BrowserWaveformReady(
                        source.clone(),
                        result.map(|(audio, _name)| crate::message::AnalysedBrowserAudio {
                            audio,
                            loop_fit: None,
                        }),
                    )
                });
            }
        } else {
            return self.replay_selected_browser_sample();
        }
        Task::none()
    }

    /// A result row was released without starting a drag. Plain releases
    /// keep the historical click behavior (select + preview); Ctrl toggles
    /// the row in a multi-selection, Shift ranges from the anchor, and a
    /// rapid second release is a double-click that imports the row.
    pub(super) fn on_browser_entry_released(
        &mut self,
        source: MediaSourceRef,
        ctrl: bool,
        shift: bool,
    ) -> Task<Message> {
        let is_double = if ctrl || shift {
            self.state.browser.entry_double_click.clear();
            false
        } else {
            let now = std::time::Instant::now();
            let position = iced::Point::new(self.state.view.cursor_x, self.state.view.cursor_y);
            let double = self.state.browser.entry_double_click.press(
                now,
                position,
                std::time::Duration::from_millis(450),
                Some(8.0),
            );
            if double {
                self.state.browser.entry_double_click.clear();
            }
            double
        };
        if is_double {
            return self.on_browser_entry_double_clicked(source);
        }
        if ctrl || shift {
            return self.select_browser_entry_modified(source, ctrl, shift);
        }
        self.update(Message::ClickLocalBrowserEntry(source))
    }

    /// Ctrl toggles a row in the multi-selection; Shift selects the range
    /// from the primary (anchor) row to `source` over the visible results.
    fn select_browser_entry_modified(
        &mut self,
        source: MediaSourceRef,
        ctrl: bool,
        shift: bool,
    ) -> Task<Message> {
        let visible = self.visible_browser_results();
        let browser = &mut self.state.browser;
        if shift {
            if let Some(anchor) = browser.selected_source.clone() {
                let anchor_pos = visible.iter().position(|s| *s == anchor);
                let target_pos = visible.iter().position(|s| *s == source);
                if let (Some(a), Some(b)) = (anchor_pos, target_pos) {
                    let (lo, hi) = (a.min(b), a.max(b));
                    browser.selected_extra.clear();
                    for item in &visible[lo..=hi] {
                        if *item != anchor {
                            browser.selected_extra.insert(item.clone());
                        }
                    }
                    return Task::none();
                }
            }
            return Task::none();
        }
        if ctrl {
            if browser.selected_source.as_ref() == Some(&source) {
                // Removing the primary promotes the first extra, if any.
                browser.selected_source = None;
                if let Some(next) = browser.selected_extra.iter().next().cloned() {
                    browser.selected_extra.remove(&next);
                    browser.selected_source = Some(next);
                }
            } else if browser.selected_extra.contains(&source) {
                browser.selected_extra.remove(&source);
            } else if browser.selected_source.is_none() {
                browser.selected_source = Some(source);
            } else {
                browser.selected_extra.insert(source);
            }
        }
        Task::none()
    }

    /// Double-click on a row: select it, then add it to the project at the
    /// playhead (selected track when it can take it, else a new track).
    pub(super) fn on_browser_entry_double_clicked(
        &mut self,
        source: MediaSourceRef,
    ) -> Task<Message> {
        let selection = self.update(Message::ClickLocalBrowserEntry(source.clone()));
        self.state.browser.push_recent(source.clone());
        self.persist_ui_settings();
        let import = self.handle_import_selected_browser_sample_to_arrangement();
        Task::batch([selection, import])
    }

    /// Right-click on a row: make it the selection when it is not part of
    /// one, then open its context menu at the cursor.
    pub(super) fn on_browser_entry_right_pressed(
        &mut self,
        source: MediaSourceRef,
    ) -> Task<Message> {
        if !self.state.browser.is_selected(&source) {
            self.state.browser.selected_extra.clear();
            self.state.browser.select_source(source.clone());
        }
        let x = self.state.view.cursor_x;
        let y = self.state.view.cursor_y;
        self.update(Message::View(
            crate::domains::view::ViewMsg::ShowContextMenu {
                x,
                y,
                target: crate::state::ContextMenuTarget::BrowserEntry { source },
            },
        ))
    }

    pub(super) fn on_browser_entry_hover(
        &mut self,
        source: Option<MediaSourceRef>,
    ) -> Task<Message> {
        self.state.browser.hovered_source = source;
        Task::none()
    }

    /// The row's play button: stop when this row is the one playing,
    /// otherwise select it and start its preview.
    pub(super) fn on_toggle_browser_row_audition(
        &mut self,
        source: MediaSourceRef,
    ) -> Task<Message> {
        if self.state.browser.audition_playing
            && self.state.browser.selected_source.as_ref() == Some(&source)
        {
            self.stop_browser_audition();
            self.state.browser.audition_playing = false;
            self.state.status_text = "Preview stopped".into();
            return Task::none();
        }
        self.update(Message::PreviewLocalEntry(source))
    }

    /// A sample-map point under the pointer: play it, without selecting it.
    pub(super) fn on_sample_map_hover(&mut self, source: Option<MediaSourceRef>) -> Task<Message> {
        let Some(source) = source else {
            return Task::none();
        };
        if self.state.browser.drag_source.is_some()
            || crate::domains::browser::DragMedia::of(&source)
                == crate::domains::browser::DragMedia::Midi
        {
            return Task::none();
        }
        let generation = self.state.browser.begin_audition_load(&source);
        if let MediaSourceRef::LocalFile { path } = &source {
            if let Some(cached) = self.state.browser.cached_preview(path) {
                return self.update(Message::LocalSamplePreviewReady(
                    source,
                    generation,
                    Ok(cached),
                ));
            }
            return Task::perform(
                decode_local_for_preview_async(path.clone()),
                move |result| {
                    Message::LocalSamplePreviewReady(
                        source.clone(),
                        generation,
                        result.map(|(audio, _name)| crate::message::AnalysedBrowserAudio {
                            audio,
                            loop_fit: None,
                        }),
                    )
                },
            );
        }
        Task::none()
    }

    pub(super) fn on_preview_local_entry(&mut self, source: MediaSourceRef) -> Task<Message> {
        if DragMedia::of(&source) == DragMedia::Midi {
            return self.preview_midi_file(source);
        }
        self.state.browser.select_source(source.clone());
        if let Some(raw) = self.state.browser.waveform_for_source(&source) {
            return self.play_browser_mode(source, raw);
        }
        let generation = self.state.browser.begin_audition_load(&source);
        if let MediaSourceRef::LocalFile { path } = source.clone() {
            if let Some(cached) = self.state.browser.cached_preview(&path) {
                return self.update(Message::LocalSamplePreviewReady(
                    source,
                    generation,
                    Ok(cached),
                ));
            }
            self.state.status_text = "Preparing Audition...".to_string();
            return Task::perform(decode_local_for_preview_async(path), move |result| {
                Message::LocalSamplePreviewReady(
                    source.clone(),
                    generation,
                    result.map(|(audio, _name)| crate::message::AnalysedBrowserAudio {
                        audio,
                        loop_fit: None,
                    }),
                )
            });
        }
        Task::none()
    }

    pub(super) fn on_stop_browser_preview(&mut self) -> Task<Message> {
        self.stop_browser_audition();
        self.state.status_text = "Audition stopped".to_string();
        Task::none()
    }

    /// A click or drag on the preview waveform: seek the playing voice,
    /// or remember the position for the next Play when stopped.
    pub(super) fn on_seek_browser_preview(&mut self, fraction: f32) -> Task<Message> {
        let fraction = fraction.clamp(0.0, 1.0);
        let Some(audio) = self.state.browser.waveform_audio.clone() else {
            return Task::none();
        };
        let frames = audio.num_frames();
        if frames == 0 {
            return Task::none();
        }
        let position = ((fraction as f64) * (frames as u64) as f64) as u64;
        let position = position.min(frames.saturating_sub(1) as u64);
        self.state.browser.audition_position_frames = position;
        if self.state.browser.audition_playing {
            self.send_command(EngineCommand::SeekAudition {
                position_frames: position,
            });
        }
        Task::none()
    }

    pub(super) fn on_toggle_audition_enabled(&mut self) -> Task<Message> {
        let enabled = self.state.browser.toggle_audition_enabled();
        if !enabled {
            self.stop_browser_audition();
        }
        self.persist_ui_settings();
        self.state.status_text = if enabled {
            "Selection Audition enabled".into()
        } else {
            "Selection Audition disabled; explicit Play remains available".into()
        };
        Task::none()
    }

    pub(super) fn on_set_audition_mode(
        &mut self,
        mode: crate::state::AuditionMode,
    ) -> Task<Message> {
        if self.state.browser.audition_mode == mode {
            return Task::none();
        }
        // A mode switch supersedes only in-flight preparation. The current
        // voice keeps playing until the Audition Bus crossfades the requested
        // treatment, so RAW/WARP never introduces a needless gap.
        self.state.browser.cancel_audition_preparation();
        self.state.browser.audition_mode = mode;
        let Some(source) = self.state.browser.selected_source.clone() else {
            return Task::none();
        };
        if self.audition_read_midi_file(&source) {
            return Task::none();
        }
        let Some(raw) = self.state.browser.waveform_audio.clone() else {
            self.state.status_text = "Select a source to audition".into();
            return Task::none();
        };
        self.play_browser_mode(source, raw)
    }

    pub(super) fn on_escape_pressed(&mut self) -> Task<Message> {
        if self.state.project.new_project_layout.take().is_some() {
            return Task::none();
        }
        if self.state.view.transient_analysis_dialog.take().is_some() {
            return Task::none();
        }
        if self.state.view.drum_rack_slice_dialog.take().is_some() {
            return Task::none();
        }
        if self.state.view.midi_file_layout_choice.is_some() {
            self.on_choose_midi_file_layout(None);
            return Task::none();
        }
        if !self
            .state
            .active_timeline_editor()
            .audio_clip_inspector_edits
            .is_empty()
        {
            self.state
                .active_timeline_editor_mut()
                .discard_audio_clip_inspector_edits();
            self.state.status_text = "Audio Clip edit cancelled".into();
            return Task::none();
        }
        if let Some(overlay) = menu_lifecycle::visible(&self.state) {
            menu_lifecycle::dismiss(&mut self.state, overlay);
            return Task::none();
        }

        // Escape abandons any pending browser import at whatever stage it is
        // in; the shared tracker aborts attached work and rejects late output.
        self.browser_import_request.cancel();
        if self.state.browser.audition_playing
            || self.state.browser.audition_loading
            || self.state.browser.audition_queued
        {
            self.stop_browser_audition();
            self.state.status_text = "Audition stopped".into();
        } else if self.state.browser.drag_source.is_some()
            || self.state.browser.pending_drag.is_some()
        {
            self.state.browser.cancel_media_drag();
            self.state.status_text = "Drag cancelled".into();
        } else if self.state.perform.editing_section_name.is_some() {
            return self.update(Message::Perform(
                crate::domains::perform::PerformMsg::CancelSectionNameEdit,
            ));
        } else {
            return self.update(Message::View(ViewMsg::CancelEditing));
        }
        Task::none()
    }

    pub(super) fn on_drop_sample_on_arrangement(
        &mut self,
        track_id: TrackId,
        position_samples: u64,
    ) -> Task<Message> {
        let sources = self.state.browser.take_drag_sources();
        if sources.is_empty() {
            return Task::none();
        }
        self.state.browser.cancel_media_drag();
        let ctrl = super::keyboard::command_held(
            self.state.view.held_modifiers,
            super::keyboard::ON_MACOS,
        );
        if self.state.view.workspace == crate::state::Workspace::Perform {
            if let Some(section_id) = self.state.perform.selected_section {
                let mut tasks = Vec::with_capacity(sources.len());
                for source in sources {
                    tasks.push(self.dispatch_drop_for_target(
                        source,
                        BrowserImportTarget::SectionClipAt {
                            section_id,
                            track_id,
                            position_samples,
                        },
                    ));
                }
                return Task::batch(tasks);
            }
        }
        // Every source goes through the lane's own clip target, so the
        // lane's existing rules still apply (a MIDI file on an audio lane
        // is turned away, an audio file converts an empty MIDI lane).
        let sample_rate = self.state.transport.sample_rate;
        let mut tasks = Vec::new();
        if ctrl {
            // Ctrl held: one new track per source, all at the dropped beat.
            for source in sources {
                tasks.push(self.dispatch_drop_for_target(
                    source,
                    BrowserImportTarget::ArrangementNewTrackAt {
                        position_samples,
                        group: None,
                    },
                ));
            }
        } else {
            // One after another on the lane, each where the previous ends.
            let mut offset = 0u64;
            for source in &sources {
                let duration = self
                    .state
                    .browser
                    .entry_duration_samples(source, sample_rate);
                tasks.push(self.dispatch_drop_for_target(
                    source.clone(),
                    BrowserImportTarget::ArrangementClipAt {
                        track_id,
                        position_samples: position_samples.saturating_add(offset),
                    },
                ));
                offset = offset.saturating_add(duration);
            }
        }
        Task::batch(tasks)
    }

    pub(super) fn on_drop_sample_on_empty_arrangement(&mut self) -> Task<Message> {
        let sources = self.state.browser.take_drag_sources();
        if sources.is_empty() {
            return Task::none();
        }
        let beat = match self.state.browser.drag_target.take() {
            Some(crate::state::BrowserDropTarget::EmptyArrangement { beat }) => beat,
            _ => self.state.transport.position_beats(),
        };
        self.state.browser.cancel_media_drag();
        let ctrl = super::keyboard::command_held(
            self.state.view.held_modifiers,
            super::keyboard::ON_MACOS,
        );
        let position_samples = self.state.transport.beats_to_samples(beat);
        let section_id = match self.active_timeline_location() {
            vibez_project::TimelineLocation::Section(section_id) => Some(section_id),
            _ => None,
        };
        if sources.len() == 1 {
            // A single file keeps its exact behaviour.
            let source = sources.into_iter().next().unwrap();
            let target = match section_id {
                Some(section_id) => BrowserImportTarget::SectionNewTrackAt {
                    section_id,
                    position_samples,
                },
                None => BrowserImportTarget::ArrangementNewTrackAt {
                    position_samples,
                    group: None,
                },
            };
            return self.dispatch_drop_for_target(source, target);
        }
        // Multi-row drop on the empty area: without Ctrl, one new track per
        // media kind with the clips one after another; with Ctrl, one new
        // track per source, all at the dropped beat.
        let sample_rate = self.state.transport.sample_rate;
        let mut audio_group: Option<u64> = None;
        let mut midi_group: Option<u64> = None;
        let mut next_group = 1u64;
        let mut audio_offset = 0u64;
        let mut midi_offset = 0u64;
        let mut tasks = Vec::with_capacity(sources.len());
        for source in &sources {
            let media = crate::domains::browser::DragMedia::of(source);
            let (group, offset) = if ctrl {
                (None, 0u64)
            } else {
                let (group_slot, offset) = match media {
                    crate::domains::browser::DragMedia::Audio => {
                        (&mut audio_group, &mut audio_offset)
                    }
                    crate::domains::browser::DragMedia::Midi => (&mut midi_group, &mut midi_offset),
                };
                let group = group_slot.get_or_insert_with(|| {
                    let g = next_group;
                    next_group += 1;
                    g
                });
                let position_offset = *offset;
                *offset = offset.saturating_add(
                    self.state
                        .browser
                        .entry_duration_samples(source, sample_rate),
                );
                (Some(*group), position_offset)
            };
            let position = position_samples.saturating_add(offset);
            let target = match section_id {
                // Perform sections make one track per source (no grouping).
                Some(section_id) => BrowserImportTarget::SectionNewTrackAt {
                    section_id,
                    position_samples: position,
                },
                None => BrowserImportTarget::ArrangementNewTrackAt {
                    position_samples: position,
                    group,
                },
            };
            tasks.push(self.dispatch_drop_for_target(source.clone(), target));
        }
        Task::batch(tasks)
    }

    pub(super) fn on_drop_sample_on_sampler(&mut self, track_id: TrackId) -> Task<Message> {
        let Some(source) = self.state.browser.drag_source.take() else {
            return Task::none();
        };
        self.state.browser.cancel_media_drag();
        self.dispatch_drop_for_target(source, BrowserImportTarget::Sampler(track_id))
    }

    pub(super) fn on_drop_sample_on_drum_rack(&mut self, track_id: TrackId) -> Task<Message> {
        let pad_index = self
            .state
            .find_track(track_id)
            .and_then(crate::domains::sampler::selected_drum_pad);
        match pad_index {
            Some(pad_index) if self.state.browser.drag_source.is_some() => {
                self.handle_drop_sample_on_drum_pad(track_id, pad_index)
            }
            _ => self.update(Message::Browser(BrowserMsg::EndDragSample)),
        }
    }

    pub(super) fn on_drop_sample_on_device_panel(&mut self, track_id: TrackId) -> Task<Message> {
        let takes_drop = self
            .state
            .find_track(track_id)
            .and_then(DevicePanelDrop::for_channel)
            .is_some();
        if !takes_drop {
            return self.update(Message::Browser(BrowserMsg::EndDragSample));
        }
        let Some(source) = self.state.browser.drag_source.take() else {
            return Task::none();
        };
        self.state.browser.cancel_media_drag();
        self.dispatch_drop_for_target(source, BrowserImportTarget::ImpliedSampler(track_id))
    }

    pub(super) fn on_local_sample_preview_ready(
        &mut self,
        source: MediaSourceRef,
        generation: u64,
        analysed: crate::message::AnalysedBrowserAudio,
    ) -> Task<Message> {
        let audio = Arc::clone(&analysed.audio);
        if self
            .state
            .browser
            .install_audition(generation, source.clone(), analysed)
        {
            // Playback starts on the decoded audio alone; the loop-tempo
            // analysis follows in its own task.
            let play = self.play_browser_mode(source.clone(), audio.clone());
            let analysis = self.spawn_loop_fit_analysis(&source, audio);
            return Task::batch([play, analysis]);
        }
        Task::none()
    }

    /// Spawns the bounded loop-tempo analysis for a shown preview, unless a
    /// fit is already known (cached preview).
    fn spawn_loop_fit_analysis(
        &self,
        source: &MediaSourceRef,
        audio: Arc<vibez_core::audio_buffer::DecodedAudio>,
    ) -> Task<Message> {
        if self.state.browser.waveform_loop_fit.is_some() {
            return Task::none();
        }
        let MediaSourceRef::LocalFile { path } = source else {
            return Task::none();
        };
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let source = source.clone();
        Task::perform(analyse_preview_loop_fit_async(audio, name), move |fit| {
            Message::BrowserLoopFitReady(source.clone(), fit)
        })
    }

    pub(super) fn on_browser_loop_fit_ready(
        &mut self,
        source: MediaSourceRef,
        fit: Option<crate::warp::LoopGridFit>,
    ) -> Task<Message> {
        if self.state.browser.waveform_source.as_ref() != Some(&source) {
            return Task::none();
        }
        self.state.browser.set_waveform_loop_fit(fit);
        if let MediaSourceRef::LocalFile { path } = &source {
            self.state.browser.update_cached_preview_fit(path, fit);
        }
        // A WARP preview now has its source tempo: prepare the warped
        // treatment; the bus crossfades when it is ready.
        if self.state.browser.audition_playing
            && self.state.browser.audition_mode == crate::state::AuditionMode::Warp
        {
            if let Some(audio) = self.state.browser.waveform_audio.clone() {
                return self.play_browser_mode(source, audio);
            }
        }
        Task::none()
    }

    pub(super) fn on_local_sample_preview_failed(
        &mut self,
        source: MediaSourceRef,
        generation: u64,
        err: String,
    ) -> Task<Message> {
        let is_current = self.state.browser.selected_source.as_ref() == Some(&source)
            && self.state.browser.audition_request_is_current(generation);
        self.state.browser.fail_waveform_load(&source, err.clone());
        if is_current {
            self.stop_browser_audition();
            self.state.status_text = format!("Audition error: {err}");
        }
        Task::none()
    }

    pub(super) fn on_browser_waveform_ready(
        &mut self,
        source: MediaSourceRef,
        analysed: crate::message::AnalysedBrowserAudio,
    ) -> Task<Message> {
        let audio = Arc::clone(&analysed.audio);
        if self
            .state
            .browser
            .install_waveform(source.clone(), analysed)
        {
            self.state.status_text = "Waveform ready".into();
            return self.spawn_loop_fit_analysis(&source, audio);
        }
        Task::none()
    }

    pub(super) fn on_browser_audition_ready(
        &mut self,
        source: MediaSourceRef,
        generation: u64,
        mode: crate::state::AuditionMode,
        project_bpm: f64,
        result: Result<Arc<vibez_core::audio_buffer::DecodedAudio>, String>,
    ) -> Task<Message> {
        let current = self.state.browser.audition_request_is_current(generation)
            && self.state.browser.selected_source.as_ref() == Some(&source)
            && self.state.browser.audition_mode == mode
            && (mode == crate::state::AuditionMode::Raw
                || (self.state.transport.bpm - project_bpm).abs() < f64::EPSILON);
        if !current {
            return Task::none();
        }
        self.state.browser.audition_loading = false;
        match result {
            Ok(audio) => self.start_browser_audition(audio, mode),
            Err(error) => {
                self.state.browser.stop_audition_state();
                self.state.status_text = format!("{} Audition error: {error}", mode.label());
            }
        }
        Task::none()
    }

    pub(super) fn on_browser_sample_decoded(
        &mut self,
        target: BrowserImportTarget,
        treatment: crate::state::AuditionImportInput,
        analysed: crate::message::AnalysedBrowserAudio,
        name: String,
        source: MediaSourceRef,
    ) -> Task<Message> {
        self.prepare_browser_sample_import(target, treatment, analysed, name, source)
    }

    pub(super) fn on_browser_import_prepared(
        &mut self,
        target: BrowserImportTarget,
        generation: u64,
        payload: crate::message::PreparedBrowserImport,
    ) -> Task<Message> {
        if !self.browser_import_request.finish(generation) {
            return Task::none();
        }
        self.apply_browser_import_prepared(target, payload)
    }

    pub(super) fn on_clip_auto_warp_ready(
        &mut self,
        track_id: TrackId,
        clip_id: ClipId,
        outcome: crate::message::AutoWarpOutcome,
    ) -> Task<Message> {
        let action = {
            let mut engine = crate::domains::EngineTx(&mut self.cmd_tx);
            self.state
                .arrangement
                .apply_auto_warp_outcome(&mut engine, track_id, clip_id, outcome)
        };
        self.apply_arrangement_action(action)
    }

    pub(super) fn on_browser_sample_decode_error(&mut self, err: String) -> Task<Message> {
        self.state.status_text = format!("Browser import error: {err}");
        Task::none()
    }
}
