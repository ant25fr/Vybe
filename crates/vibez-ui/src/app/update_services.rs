//! The message router: one exhaustive match, bodies delegated to
//! domain updates and topic-module handlers.

use iced::Task;

use crate::domains::browser::BrowserMsg;
use vibez_engine::commands::EngineCommand;
use vibez_plugin_host::gui::PluginGuiKey;

use crate::services::plugin_loader::{load_plugin_effect_bg, load_plugin_instrument_bg};

use crate::message::Message;

use super::*;

impl App {
    pub(super) fn update_services(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenAbout => {
                self.state.about_open = true;
            }
            Message::OpenUrl(url) => {
                return Task::perform(open_url_async(url), Message::UrlOpened);
            }
            Message::UrlOpened(result) => {
                if let Err(error) = result {
                    self.state.status_text = format!("Could not open link: {error}");
                }
            }

            // -- Window close protection --
            Message::WindowCloseRequested => {
                return self.route_window_close_requested();
            }
            Message::CloseConfirmSave => {
                return self.route_close_confirm_save();
            }
            Message::CloseConfirmDiscard => {
                return self.route_close_confirm_discard();
            }
            Message::CloseConfirmCancel => {
                return self.route_close_confirm_cancel();
            }

            // -- Settings --
            Message::OpenSettings => {
                self.state.settings_open = true;
            }
            Message::CloseSettings => {
                self.state.settings_open = false;
                self.state.view.shortcut_capture = None;
                self.state.perform.key_rebind_target = None;
                let _ = self.state.plugin_settings.save();
            }
            Message::SelectSettingsTab(tab) => {
                self.state.settings_tab = tab;
            }
            Message::BeginShortcutCapture(action) => {
                // Clicking the row of the action being captured cancels.
                self.state.view.shortcut_capture =
                    if self.state.view.shortcut_capture == Some(action) {
                        None
                    } else {
                        Some(action)
                    };
            }
            Message::ResetShortcut(action) => {
                self.shortcuts.set_binding(action, Vec::new());
                self.persist_ui_settings();
                self.state.status_text = format!("{} reset to its default binding", action.label());
            }
            Message::SelectAudioBackend(backend) => {
                return self.handle_select_audio_backend(backend);
            }
            Message::SetBufferSize(size) => {
                return self.handle_set_buffer_size(size);
            }
            Message::SetAudioSampleRate(sample_rate) => {
                return self.handle_set_audio_sample_rate(sample_rate);
            }
            Message::SelectAudioInput(choice) => {
                return self.handle_select_audio_input(choice);
            }
            Message::SelectAudioOutput(choice) => {
                return self.handle_select_audio_output(choice);
            }
            Message::RescanAudioDevices => {
                return self.handle_rescan_audio_devices();
            }
            Message::ReconnectAudioOutput => {
                return self.handle_reconnect_audio_output();
            }
            // -- Plugin scanning --
            Message::ScanPlugins => {
                return self.handle_scan_plugins();
            }
            Message::ScanPluginsComplete(report) => {
                return self.handle_scan_plugins_complete(report);
            }
            Message::AddPluginScanPath => {
                return Task::perform(
                    async {
                        let result = rfd::AsyncFileDialog::new()
                            .set_title("Select Plugin Scan Directory")
                            .pick_folder()
                            .await;
                        result.map(|h| h.path().to_path_buf())
                    },
                    Message::PluginScanPathSelected,
                );
            }
            Message::PluginScanPathSelected(path) => {
                return self.handle_plugin_scan_path_selected(path);
            }
            Message::RemovePluginScanPath(index) => {
                if index < self.state.plugin_settings.extra_scan_paths.len() {
                    self.state.plugin_settings.extra_scan_paths.remove(index);
                    let _ = self.state.plugin_settings.save();
                }
            }
            Message::ToggleScanDefaultPaths => {
                self.state.plugin_settings.scan_default_paths =
                    !self.state.plugin_settings.scan_default_paths;
                let _ = self.state.plugin_settings.save();
            }
            Message::ToggleSandboxPlugins => {
                self.state.plugin_settings.sandbox_plugins =
                    !self.state.plugin_settings.sandbox_plugins;
                let _ = self.state.plugin_settings.save();
            }
            Message::PluginRemoteGuiResult(_key, result) => match result {
                Ok(true) => {
                    self.state.status_text = "Plugin interface opened (sandboxed)".to_string()
                }
                Ok(false) => self.state.status_text = "Plugin has no interface to open".to_string(),
                Err(e) => self.state.status_text = format!("Plugin sandbox error: {e}"),
            },
            Message::PluginRemoteGuiClosed(_) => {}

            // -- Plugin loading --
            Message::AddPluginToTrack(track_id, plugin_id) => {
                self.state.devices.context_menu = None;
                if let Some(info) = self
                    .state
                    .plugin_settings
                    .cache
                    .iter()
                    .find(|p| p.id == plugin_id)
                    .cloned()
                {
                    let sample_rate = self.state.transport.sample_rate as f64;
                    let is_instrument = info.category.is_instrument();
                    let loading_name = info.name.clone();

                    if !is_instrument
                        && self
                            .state
                            .find_track(track_id)
                            .is_some_and(|track| !track.has_device_room())
                    {
                        self.state.status_text =
                            crate::domains::devices::DEVICE_CHAIN_FULL.to_string();
                        return Task::none();
                    }
                    if is_instrument {
                        if let Some(refusal) = crate::domains::devices::instrument_refusal(
                            self.state.find_track(track_id),
                            &loading_name,
                        ) {
                            self.state.status_text = refusal;
                            return Task::none();
                        }
                        let token = self
                            .plugin_load_requests
                            .begin(PluginGuiKey::Instrument { track_id });
                        let tx = self.plugin_instrument_tx.clone();
                        let sandbox = self.state.plugin_settings.sandbox_plugins;
                        std::thread::spawn(move || {
                            match load_plugin_instrument_bg(&info, sample_rate, None, sandbox) {
                                Ok(mut result) => {
                                    result.load_token = token;
                                    result.track_id = track_id;
                                    let _ = tx.send(result);
                                }
                                Err(e) => {
                                    eprintln!("Plugin load error: {e}");
                                }
                            }
                        });
                    } else {
                        let effect_id = vibez_core::id::EffectId::new();
                        let token = self.plugin_load_requests.begin(PluginGuiKey::Effect {
                            track_id,
                            effect_id,
                        });
                        let tx = self.plugin_effect_tx.clone();
                        let sandbox = self.state.plugin_settings.sandbox_plugins;
                        std::thread::spawn(move || {
                            match load_plugin_effect_bg(&info, sample_rate, None, sandbox) {
                                Ok(mut result) => {
                                    result.load_token = token;
                                    result.track_id = track_id;
                                    result.effect_id = effect_id;
                                    let _ = tx.send(result);
                                }
                                Err(e) => {
                                    eprintln!("Plugin load error: {e}");
                                }
                            }
                        });
                    }
                    self.state.status_text = format!("Loading {loading_name}...");
                }
            }
            Message::PluginLoadError(err) => {
                self.state.status_text = format!("Plugin error: {err}");
            }

            // -- Plugin GUI windows --
            Message::OpenPluginGui(key) => {
                // If the window is already open, raise it
                if let Some(ref mgr) = self.plugin_window_manager {
                    if mgr.is_open(key) {
                        mgr.raise(key);
                        return Task::none();
                    }
                }
                if let Some(remote) = self.plugin_remote_handles.get(&key).cloned() {
                    self.state.status_text =
                        "Opening plugin interface (sandboxed window)...".to_string();
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                remote.request(
                                    vibez_plugin_host::bridge::BridgeRequest::OpenGui {
                                        width: 900,
                                        height: 600,
                                    },
                                    std::time::Duration::from_secs(15),
                                )
                            })
                            .await
                            .unwrap_or(Err(vibez_plugin_host::bridge::BridgeError::Timeout))
                        },
                        move |result| {
                            Message::PluginRemoteGuiResult(
                                key,
                                result.map(|resp| {
                                    matches!(
                                        resp,
                                        vibez_plugin_host::bridge::BridgeResponse::Bool(true)
                                    )
                                }),
                            )
                        },
                    );
                }
                if let Some(&raw_ptr) = self.plugin_gui_raw_ptrs.get(&key) {
                    let title = match key {
                        PluginGuiKey::Effect {
                            track_id,
                            effect_id,
                        } => self
                            .state
                            .find_track(track_id)
                            .and_then(|t| {
                                t.effects
                                    .iter()
                                    .find(|e| e.id == effect_id)
                                    .and_then(|e| e.plugin_name.clone())
                            })
                            .unwrap_or_else(|| "Plugin".to_string()),
                        PluginGuiKey::Instrument { track_id } => self
                            .state
                            .find_track(track_id)
                            .and_then(|t| t.plugin_instrument_name.clone())
                            .unwrap_or_else(|| "Plugin".to_string()),
                    };
                    if let Some(ref mut mgr) = self.plugin_window_manager {
                        if mgr.open(key, raw_ptr, title) {
                            self.state.status_text = "Plugin GUI opened".to_string();
                        } else {
                            self.state.status_text = "Failed to open plugin GUI".to_string();
                        }
                    } else {
                        self.state.status_text =
                            "Native window system unavailable: plugin GUI cannot open".to_string();
                    }
                } else {
                    self.state.status_text = "Plugin GUI handle not available".to_string();
                }
            }
            Message::ClosePluginGui(key) => {
                return self.handle_close_plugin_gui(key);
            }

            // -- Bounce / resample --
            Message::ToggleCheckForUpdates => {
                self.state.update_check.enabled = !self.state.update_check.enabled;
                self.persist_ui_settings();
            }
            Message::CheckForUpdatesNow => {
                if self.state.update_check.begin_check() {
                    return Task::perform(
                        crate::update_check::fetch_latest_tag(),
                        Message::UpdateCheckCompleted,
                    );
                }
            }
            Message::UpdateCheckCompleted(tag) => {
                self.state
                    .update_check
                    .record_result(tag, crate::update_check::now_unix());
                // Persist so the throttle survives a restart, which is
                // the whole point of recording failures too.
                self.persist_ui_settings();
            }
            Message::DismissUpdateNotice => {
                self.state.update_check.dismissed = true;
            }
            Message::OpenReleasesPage => {
                // Retiring the notice once the URL has been handed off
                // keeps a single mechanism for hiding it.
                return Task::perform(crate::update_check::open_releases_page(), |()| {
                    Message::DismissUpdateNotice
                });
            }
            Message::RescanMidiInputs => return self.on_rescan_midi_inputs(),
            Message::OpenMidiInput(name) => return self.on_open_midi_input(name),
            Message::CloseMidiInput => {
                self.midi_input = None;
                self.persist_ui_settings();
                self.state.status_text = "MIDI input disconnected".to_string();
            }
            Message::SelectTheme(name) => return self.on_select_theme(name),
            Message::RescanThemes => return self.on_rescan_themes(),
            Message::ThemeSaveNameChanged(name) => {
                self.state.theme_save_name = name;
            }
            Message::SaveCurrentTheme => return self.on_save_current_theme(),
            Message::RewarpAllClips => {
                return self.handle_rewarp_all_clips();
            }
            Message::AddSampleLibraryRoot => return self.on_add_sample_library_root(),
            Message::SampleLibraryRootSelected(path) => {
                return self.on_sample_library_root_selected(path)
            }
            Message::RescanSampleLibrary => return self.on_rescan_sample_library(),
            Message::ClickLocalBrowserEntry(source) => {
                return self.on_click_local_browser_entry(source)
            }
            Message::BrowserEntryReleased {
                source,
                ctrl,
                shift,
            } => return self.on_browser_entry_released(source, ctrl, shift),
            Message::BrowserEntryDoubleClicked(source) => {
                return self.on_browser_entry_double_clicked(source)
            }
            Message::BrowserEntryRightPressed(source) => {
                return self.on_browser_entry_right_pressed(source)
            }
            Message::BrowserTagRightPressed(id) => {
                let x = self.state.view.cursor_x;
                let y = self.state.view.cursor_y;
                return self.update(Message::View(
                    crate::domains::view::ViewMsg::ShowContextMenu {
                        x,
                        y,
                        target: crate::state::ContextMenuTarget::BrowserTag { id },
                    },
                ));
            }
            Message::BrowserTagFolderRightPressed(id) => {
                let x = self.state.view.cursor_x;
                let y = self.state.view.cursor_y;
                return self.update(Message::View(
                    crate::domains::view::ViewMsg::ShowContextMenu {
                        x,
                        y,
                        target: crate::state::ContextMenuTarget::BrowserTagFolder { id },
                    },
                ));
            }
            Message::BrowserTaggableRightPressed(item) => {
                let x = self.state.view.cursor_x;
                let y = self.state.view.cursor_y;
                return self.update(Message::View(
                    crate::domains::view::ViewMsg::ShowContextMenu {
                        x,
                        y,
                        target: crate::state::ContextMenuTarget::BrowserTaggable { item },
                    },
                ));
            }
            Message::DeviceParamRightPressed {
                track_id,
                slot,
                param_index,
            } => {
                let target = self.state.project_tracks.find(track_id).and_then(|track| {
                    crate::domains::automation::automation_target_for(&slot, param_index, track)
                });
                let Some(target) = target else {
                    return Task::none();
                };
                let x = self.state.view.cursor_x;
                let y = self.state.view.cursor_y;
                return self.update(Message::View(
                    crate::domains::view::ViewMsg::ShowContextMenu {
                        x,
                        y,
                        target: crate::state::ContextMenuTarget::AutomationParam {
                            track_id,
                            target,
                        },
                    },
                ));
            }
            Message::BrowserEntryHover(source) => return self.on_browser_entry_hover(source),
            Message::ToggleBrowserRowAudition(source) => {
                return self.on_toggle_browser_row_audition(source)
            }
            Message::BeginPendingBrowserDrag(source, label) => {
                // Pressing a selected row drags the whole selection.
                let sources = {
                    let browser = &self.state.browser;
                    if browser.selection_count() > 1 && browser.is_selected(&source) {
                        browser.selection_sources(&self.visible_browser_results())
                    } else {
                        vec![source.clone()]
                    }
                };
                let msg = if sources.len() > 1 {
                    BrowserMsg::BeginPendingDragMany {
                        sources,
                        label,
                        origin_x: self.state.view.cursor_x,
                        origin_y: self.state.view.cursor_y,
                    }
                } else {
                    BrowserMsg::BeginPendingDrag {
                        source,
                        label,
                        origin_x: self.state.view.cursor_x,
                        origin_y: self.state.view.cursor_y,
                    }
                };
                let action = self.state.browser.update(msg);
                return self.apply_browser_action(action);
            }
            Message::BeginPendingTaggableDrag(item, label) => {
                let action = self
                    .state
                    .browser
                    .update(BrowserMsg::BeginPendingTaggableDrag {
                        item,
                        label,
                        origin_x: self.state.view.cursor_x,
                        origin_y: self.state.view.cursor_y,
                    });
                return self.apply_browser_action(action);
            }
            Message::TaggableDragReleased { item: _, click } => {
                // A real drag is dropped by the global release on the hovered
                // tag row; only a plain click runs the row's action.
                if self.state.browser.tag_drag.is_some() {
                    return Task::none();
                }
                if let Some(click) = click {
                    return self.update(*click);
                }
                return Task::none();
            }
            Message::PreviewLocalEntry(source) => return self.on_preview_local_entry(source),
            Message::SampleMapHover(source) => return self.on_sample_map_hover(source),
            Message::StopBrowserPreview => return self.on_stop_browser_preview(),
            Message::SeekBrowserPreview(fraction) => return self.on_seek_browser_preview(fraction),
            Message::ToggleAuditionEnabled => return self.on_toggle_audition_enabled(),
            Message::SetAuditionGain(gain) => {
                self.state.browser.set_audition_gain(gain);
                self.send_command(EngineCommand::SetAuditionGain(
                    self.state.browser.audition_gain,
                ));
                self.persist_ui_settings();
            }
            Message::SetAuditionMode(mode) => return self.on_set_audition_mode(mode),
            Message::EscapePressed => return self.on_escape_pressed(),

            // -- Drag-and-drop from sample browser --
            Message::DropSampleOnArrangement {
                track_id,
                position_samples,
            } => return self.on_drop_sample_on_arrangement(track_id, position_samples),
            Message::DropSampleOnEmptyArrangement => {
                return self.on_drop_sample_on_empty_arrangement()
            }
            Message::MidiFileRead {
                target,
                generation,
                result,
            } => self.on_midi_file_read(target, generation, result),
            Message::ChooseMidiFileLayout(layout) => self.on_choose_midi_file_layout(layout),
            Message::DropSampleOnDrumPad {
                track_id,
                pad_index,
            } => {
                return self.handle_drop_sample_on_drum_pad(track_id, pad_index);
            }
            Message::DropSampleOnSampler { track_id } => {
                return self.on_drop_sample_on_sampler(track_id)
            }
            Message::DropSampleOnDrumRack { track_id } => {
                return self.on_drop_sample_on_drum_rack(track_id)
            }
            Message::DropSampleOnDevicePanel { track_id } => {
                return self.on_drop_sample_on_device_panel(track_id)
            }
            Message::LocalSamplePreviewReady(source, generation, Ok(audio)) => {
                return self.on_local_sample_preview_ready(source, generation, audio)
            }
            Message::LocalSamplePreviewReady(source, generation, Err(err)) => {
                return self.on_local_sample_preview_failed(source, generation, err)
            }
            Message::BrowserWaveformReady(source, Ok(audio)) => {
                return self.on_browser_waveform_ready(source, audio)
            }
            Message::BrowserLoopFitReady(source, fit) => {
                return self.on_browser_loop_fit_ready(source, fit)
            }
            Message::BrowserWaveformReady(source, Err(err)) => {
                self.state.browser.fail_waveform_load(&source, err);
            }
            Message::BrowserMidiPreviewReady {
                source,
                generation,
                play,
                result,
            } => self.on_midi_preview_ready(source, generation, play, result),
            Message::BrowserAuditionReady {
                source,
                generation,
                mode,
                project_bpm,
                result,
            } => {
                return self.on_browser_audition_ready(
                    source,
                    generation,
                    mode,
                    project_bpm,
                    result,
                )
            }
            Message::ImportSelectedBrowserSampleToArrangement => {
                return self.handle_import_selected_browser_sample_to_arrangement();
            }
            Message::SelectAdjacentBrowserResult(direction) => {
                return self.select_adjacent_browser_result(direction);
            }
            Message::ReplaySelectedBrowserSample => {
                return self.on_replay_key();
            }
            Message::LoadSelectedBrowserSampleToDevice => {
                return self.handle_load_selected_browser_sample_to_device();
            }
            Message::BrowserSampleDecoded(target, treatment, audio, name, source) => {
                return self.on_browser_sample_decoded(target, treatment, audio, name, source)
            }
            Message::BrowserImportPrepared {
                target,
                generation,
                payload,
            } => return self.on_browser_import_prepared(target, generation, payload),
            Message::ClipAutoWarpReady {
                track_id,
                clip_id,
                outcome,
            } => return self.on_clip_auto_warp_ready(track_id, clip_id, outcome),
            Message::BrowserSampleDecodeError(err) => {
                return self.on_browser_sample_decode_error(err)
            }
            Message::OpenQuickAdd => {
                if self.state.view.quick_add.is_some() {
                    self.close_quick_add();
                    return iced::Task::none();
                }
                self.state.view.quick_add = Some(crate::state::QuickAddState {
                    query: std::mem::take(&mut self.state.view.quick_add_last_query),
                    selected: 0,
                    kind: crate::state::QuickAddKind::All,
                });
                return iced::widget::text_input::focus(
                    super::views_quick_add::quick_add_input_id(),
                );
            }
            Message::CloseQuickAdd => {
                self.close_quick_add();
            }
            Message::QuickAddQueryChanged(query) => {
                if let Some(state) = &mut self.state.view.quick_add {
                    state.query = query;
                    state.selected = 0;
                }
            }
            Message::QuickAddMove(delta) => {
                let count = self
                    .state
                    .view
                    .quick_add
                    .as_ref()
                    .map(|state| self.quick_add_items(state).len())
                    .unwrap_or(0);
                if let Some(state) = &mut self.state.view.quick_add {
                    if count > 0 {
                        let next = state.selected as i64 + delta as i64;
                        state.selected = next.rem_euclid(count as i64) as usize;
                    }
                }
            }
            Message::QuickAddSetKind(kind) => {
                if let Some(state) = &mut self.state.view.quick_add {
                    state.kind = kind;
                    state.selected = 0;
                }
            }
            Message::QuickAddSubmit => {
                let item = self
                    .state
                    .view
                    .quick_add
                    .as_ref()
                    .and_then(|state| self.quick_add_items(state).get(state.selected).cloned());
                self.close_quick_add();
                if let Some(item) = item {
                    return self.quick_add_apply(item);
                }
            }
            Message::QuickAddDevice(item) => return self.quick_add_apply(item),
            _ => unreachable!("service messages are routed by App::update"),
        }
        Task::none()
    }
}

/// Hands a compile-time-constant link to the system browser.
///
/// `open::that` blocks while it spawns the handler, so it runs on a
/// blocking thread: the UI thread must never wait on a browser launch.
async fn open_url_async(url: &'static str) -> Result<(), String> {
    tokio::task::spawn_blocking(move || open::that(url).map_err(|error| error.to_string()))
        .await
        .map_err(|error| error.to_string())?
}
