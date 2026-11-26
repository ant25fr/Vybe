//! Application lifecycle, domain dispatch and engine-event consumer composition.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::{Subscription, Task, Theme};

use crate::domains::browser::BrowserMsg;
use crate::domains::perform::PerformMsg;
use crate::domains::view::ViewMsg;
use rtrb::Consumer;
use vibez_audio_io::audio_host::AudioBackend;
use vibez_audio_io::audio_stream::AudioOutputStream;
use vibez_core::constants::UI_TICK_MS;
use vibez_core::track::MediaSourceRef;
use vibez_engine::commands::EngineCommand;
use vibez_engine::engine::AudioEngine;
use vibez_engine::events::EngineEvent;
use vibez_plugin_host::gui::PluginGuiKey;

use crate::services::plugin_loader::{PluginInstrumentLoadResult, PluginLoadResult};

use crate::domains::audio_settings::AudioSettingsState;
use crate::message::{Message, ProjectLoadResult};
use crate::plugin_window::{PluginRawPtr, PluginWindowManager};
use crate::state::{AppState, AudioStreamHealth};
use crate::theme as th;
use crate::ui_settings::UiSettings;

pub(super) const RAW_AUDITION_PLAYING: &str = "RAW Audition playing";
pub(super) const WARP_AUDITION_PLAYING: &str = "WARP Audition playing";
pub(super) const WARP_AUDITION_PREPARING: &str = "Preparing WARP Audition";

struct App {
    state: AppState,
    sidechain_sync_inputs: Option<sidechain_sync::RoutingInputs>,
    track_meter_peaks: std::collections::HashMap<vibez_core::id::TrackId, (f32, f32)>,
    /// Follows the pointer during another app's drag.
    pointer_poll: desktop_pointer::PointerPoll,
    edge_shortcuts: EdgeShortcutState,
    /// Effective key bindings (defaults + user overrides from ui.json).
    /// The global key handler routes through this, and Settings →
    /// Shortcuts edits it.
    pub(crate) shortcuts: crate::shortcuts::Shortcuts,
    cmd_tx: crate::domains::EngineCommandQueue,
    event_rx: Option<Consumer<EngineEvent>>,
    _stream: Option<AudioOutputStream>,
    sample_retirement: sample_retirement::SampleRetirement,
    input_bridge: Arc<vibez_audio_io::audio_input::AudioInputBridge>,
    /// Open only while one Audio Track is armed or explicitly monitors input.
    _input_stream: Option<vibez_audio_io::audio_input::AudioInputStream>,
    // Channels for receiving loaded plugins from background threads
    plugin_effect_rx: std::sync::mpsc::Receiver<PluginLoadResult>,
    plugin_effect_tx: std::sync::mpsc::Sender<PluginLoadResult>,
    plugin_instrument_rx: std::sync::mpsc::Receiver<PluginInstrumentLoadResult>,
    plugin_instrument_tx: std::sync::mpsc::Sender<PluginInstrumentLoadResult>,
    plugin_load_requests: plugin_load_requests::PluginLoadRequests,
    device_meters: device_meters::DeviceMeters,
    gain_devices: gain_devices::GainDevices,
    eq_devices: eq_devices::EqDevices,
    widener_devices: widener_devices::WidenerDevices,
    compressor_devices: compressor_devices::CompressorDevices,
    filter_panels: device_panels::DevicePanels<crate::domains::filter::FilterPanelState>,
    gate_panels: device_panels::DevicePanels<crate::domains::gate::GatePanelState>,
    card_panels: device_panels::DevicePanels<crate::domains::device_card::CardPanelState>,
    multiband_panels:
        device_panels::DevicePanels<crate::domains::multiband_dynamics::MultibandPanelState>,
    instrument_panels: std::collections::HashMap<
        vibez_core::id::TrackId,
        crate::domains::instrument_card::InstrumentPanel,
    >,
    param_entry: Option<crate::domains::param_entry::ValueEntry>,
    device_presets: device_presets::DevicePresetRuntime,
    // Plugin GUI support
    plugin_window_manager: Option<PluginWindowManager>,
    plugin_gui_raw_ptrs: std::collections::HashMap<PluginGuiKey, PluginRawPtr>,
    /// Raw pointers for live state capture at save time, keyed like
    /// the GUI pointers. Entries live exactly as long as the device.
    plugin_state_ptrs: std::collections::HashMap<PluginGuiKey, vibez_plugin_host::PluginStatePtr>,
    /// Sandboxed plugin instances: bridge subprocess handles kept for GUI
    /// commands and live state capture. Dropping the last reference kills
    /// the subprocess.
    plugin_remote_handles: std::collections::HashMap<
        PluginGuiKey,
        std::sync::Arc<vibez_plugin_host::bridge::BridgeChild>,
    >,
    /// Plugin preflight and render progress for the one active project export.
    export_job: Option<project_export::ExportJob>,
    export_render_progress: Option<Arc<std::sync::atomic::AtomicU8>>,
    export_plugin_return_rx:
        Option<std::sync::mpsc::Receiver<vibez_engine::render::OfflinePlugins>>,

    /// Generation for the whole Browser import pipeline. Bumped on
    /// cancellation and project reset so an in-flight WARP preparation
    /// cannot land a clip afterwards.
    browser_import_request: tracked_request::TrackedRequest,
    /// Cancellable preparation for the next resident Section launch.
    section_residency_request: tracked_request::TrackedRequest,

    // External MIDI input (USB keyboard, Ableton Push, virtual cable...).
    // Dropping the handle closes the port.
    midi_ports: midi_ports::MidiPorts,
    /// The Arrange header being dragged.
    dragged_header: Option<vibez_core::id::TrackId>,
    /// The freeze or bounce in place being rendered.
    pending_track_render: Option<track_freeze::TrackRender>,
    midi_learn: midi_mapping::MidiLearn,
    hardware_keys: crate::domains::hardware_keys::HardwareKeys,
    /// Cached list of port names last seen by `list_midi_input_ports`.
    /// Populated when the user opens the MIDI picker; used so the UI
    /// can show a dropdown without re-scanning on every frame.
    midi_input_ports: Vec<String>,

    /// Multiplier iced applies to the whole logical coordinate space.
    /// It lives on the shell rather than in `AppState` because it
    /// describes the window the project is viewed through, not the
    /// project or the view layout, and nothing about it is undoable.
    interface_scale: f32,
    /// Debounce, revision, and in-flight coordination for project saves.
    save_runtime: save_runtime::SaveRuntime,
    // Undo / redo
}

pub fn run() -> iced::Result {
    // Raw 64x64 RGBA (assets/icon/vibez-64.rgba, generated from
    // assets/icon/vibez.svg) so the window icon needs no image
    // decoder in the dependency tree.
    let icon = iced::window::icon::from_rgba(
        include_bytes!("../../../../assets/icon/vibez-64.rgba").to_vec(),
        64,
        64,
    )
    .ok();
    crate::typography::load_bundled_fonts();
    iced::application(App::title, App::update_and_refresh_clips, App::view)
        .theme(App::theme)
        .scale_factor(App::scale_factor)
        .antialiasing(true)
        .subscription(App::subscription)
        // Unsaved edits get a confirmation instead of a silent exit, so the
        // close request has to reach `update` rather than the window itself.
        .exit_on_close_request(false)
        .window({
            #[allow(unused_mut)]
            let mut settings = iced::window::Settings {
                icon,
                min_size: Some(iced::Size::new(900.0, 600.0)),
                ..Default::default()
            };
            // WM_CLASS / app_id: lets docks and taskbars match the
            // window to a vibez.desktop entry instead of guessing.
            // The field only exists in the Linux settings variant.
            #[cfg(target_os = "linux")]
            {
                settings.platform_specific.application_id = "vibez".to_string();
            }
            settings
        })
        .window_size((1400.0, 900.0))
        .run_with(App::new)
}

mod actions;
mod audio_join_preparation;
mod audio_recording;
mod audio_settings;
mod audio_take_finalization;
mod audio_tasks;
mod auto_pan_devices;
mod bitcrush_devices;
mod bounce;
mod capture;
mod card_devices;
mod compensation;
mod compressor_devices;
mod delay_devices;
mod device_meters;
mod device_panels;
pub(crate) mod device_presets;
mod drive_devices;
mod drum_kits;
mod drum_rack_devices;
mod drum_rack_preparation;
mod engine_events;
mod eq_devices;
mod filter_devices;
mod fm_devices;
mod gain_devices;
mod gate_devices;
mod generated_media;
mod instrument_devices;
mod keyboard;
mod local_watcher;
mod menu_lifecycle;
mod midi_from;
#[cfg(test)]
mod midi_from_tests;
mod param_entry;
mod phaser_devices;
mod plucked_devices;
mod reverb_devices;
mod sampler_devices;
mod synth_devices;
mod tasks;
mod tracked_request;
mod wavetable_devices;
mod widener_devices;

pub(crate) use audio_tasks::*;
use drum_rack_preparation::*;
pub(crate) use keyboard::*;
use tasks::*;
mod media;
mod media_import;
mod plugins;
mod project_export;
mod project_io;
mod project_replay;
mod project_sample_rate;
mod project_sections;
mod project_serialization;
mod quick_add;
mod save_runtime;
mod section_record;
mod sidechain;
mod timeline_results;
mod transient_markers;
mod update;
mod update_audio_edits;
mod update_media;
mod update_policy;
mod update_project;
mod update_services;
mod update_timeline;
mod update_view;
mod views_about;
mod views_arrangement;
mod views_audio_clip;
mod views_automation;
mod views_browser;
mod views_browser_audition;
mod views_browser_category;
mod views_browser_places;
mod views_browser_sample_map;
mod views_browser_style;
mod views_clip_swing;
mod views_close_confirm;
mod views_control_bar;
mod views_control_bar_style;
mod views_control_status;
mod views_detail;
mod views_devices;
mod views_dialog;
mod views_file_menu;
mod views_grid_menu;
mod views_midi_file_layout;
mod views_mixer;
mod views_overlays;
mod views_perform;
mod views_perform_header;
mod views_perform_instrument;
mod views_perform_pads;
mod views_perform_playhead;
mod views_perform_record;
mod views_perform_sections;
mod views_quick_add;
mod views_settings;
mod views_settings_appearance;
mod views_settings_audio;
mod views_settings_perform;
mod views_settings_project;
mod views_settings_rows;
mod views_settings_shortcuts;
mod views_settings_updates;
mod views_shell;
mod views_takes;
mod views_transient_analysis;
mod views_transport;
mod window_policy;

mod sample_retirement;

mod desktop_drop;

mod desktop_pointer;
pub(crate) use desktop_pointer::PointerWindow;
#[cfg(target_os = "macos")]
mod desktop_pointer_macos;
#[cfg(windows)]
mod desktop_pointer_windows;
#[cfg(all(test, windows))]
mod desktop_pointer_windows_tests;
#[cfg(not(any(target_os = "macos", windows)))]
mod desktop_pointer_x11;
mod drop_target_probe;

#[cfg(test)]
mod desktop_pointer_tests;

mod drag_ghost;

#[cfg(test)]
mod drag_ghost_tests;

#[cfg(test)]
mod new_track_drop_tests;

#[cfg(test)]
mod clip_color_menu_tests;
#[cfg(test)]
mod empty_space_drop_tests;
#[cfg(test)]
mod undo_redo_view_tests;
#[cfg(test)]
mod view_harness;
#[cfg(test)]
mod wheel_zoom_view_tests;

mod midi_audition;
mod midi_file_drop;

#[cfg(test)]
mod device_panel_drop_tests;

#[cfg(test)]
mod midi_file_drop_tests;

#[cfg(test)]
mod browser_midi_audition_tests;

#[cfg(test)]
mod desktop_drop_tests;

#[cfg(test)]
mod instrument_media_retirement_tests;

#[cfg(test)]
mod instrument_media_shutdown_tests;

#[cfg(test)]
mod instrument_media_ui_flow_tests;

#[cfg(test)]
mod instrument_placement_tests;
#[cfg(test)]
mod loop_region_persistence_tests;
#[cfg(test)]
mod project_format_v1_tests;
#[cfg(test)]
mod sample_rate_import_tests;
#[cfg(test)]
mod sample_rate_reopen_tests;
#[cfg(test)]
mod save_round_trip_tests;
#[cfg(test)]
mod track_height_tests;

impl Drop for App {
    fn drop(&mut self) {
        // Editor callbacks must finish before the stream, engine event queues,
        // or other fields can drop plugin instances and unload their libraries.
        if let Some(manager) = self.plugin_window_manager.as_mut() {
            manager.close_all();
        }
        // Guards must outlive every callback and the engine's last voice owners.
        drop(self._stream.take());
    }
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let (mut engine, cmd_tx, event_rx) = AudioEngine::new();
        let transport = engine
            .take_transport_sender()
            .expect("a new engine still holds its transport sender");
        let eq_spectrum_rx = engine.take_effect_spectrum_consumer();
        let side_spectrum_rx = engine.take_side_spectrum_consumer();
        let mid_spectrum_rx = engine.take_mid_spectrum_consumer();
        let ui_settings = UiSettings::load();
        // Read before the settings are picked apart below, and clamped
        // here rather than at the render site so a hand-edited ui.json
        // cannot open the window at an unusable size.
        let interface_scale = ui_settings.clamped_interface_scale();
        let recent_project_paths =
            crate::ui_settings::normalize_recent_projects(ui_settings.recent_project_paths.clone());
        let shortcuts =
            crate::shortcuts::Shortcuts::from_overrides(ui_settings.shortcut_overrides.clone());

        let preferred_audio_backend = if ui_settings.audio_backend.is_available() {
            ui_settings.audio_backend
        } else {
            AudioBackend::System
        };
        let audio_backend_preferences = ui_settings.resolved_audio_device_preferences();
        let preferred_audio_output = audio_backend_preferences
            .for_backend(preferred_audio_backend)
            .output_name
            .clone();
        let requested_sample_rate = ui_settings.audio_sample_rate;
        let requested_buffer_size = ui_settings.audio_buffer_size;
        let audio_settings::InitialAudioOutput {
            stream: output_stream,
            sample_rate,
            active_output_name,
            active_backend,
            health: audio_stream_health,
            status: initial_audio_status,
        } = audio_settings::initialize_audio_output(
            engine,
            preferred_audio_backend,
            preferred_audio_output.clone(),
            requested_sample_rate,
            requested_buffer_size,
        );
        // If a saved backend cannot open any output, startup deliberately
        // falls back to the native host. Keep Settings and on-demand input on
        // that same active host instead of presenting one backend while the
        // engine silently runs another.
        let settings_audio_backend = active_backend.unwrap_or(preferred_audio_backend);
        let active_preferences = audio_backend_preferences.for_backend(settings_audio_backend);
        let preferred_audio_input = active_preferences.input_name.clone();
        let preferred_audio_output = active_preferences.output_name.clone();
        // Listing every device takes most of a second, so it runs beside the
        // opening window. ASIO drivers stay on the thread that loaded them.
        let catalog_scanned_later = settings_audio_backend != AudioBackend::Asio;
        let (audio_catalog, catalog_error) = if catalog_scanned_later {
            Default::default()
        } else {
            match audio_settings::scan_audio_catalog(settings_audio_backend) {
                Ok(catalog) => (catalog, None),
                Err(error) => (Default::default(), Some(error)),
            }
        };
        let input_bridge = output_stream.input_bridge();

        let restored_tags = crate::state::BrowserState::restore_tags(
            ui_settings.browser_tags.clone(),
            ui_settings.browser_tag_tree.clone(),
            ui_settings.browser_top_tag_ids.clone(),
        );
        let mut state = AppState {
            transport: crate::state::TransportState {
                sample_rate,
                ..Default::default()
            },
            audio_stream_health,
            audio_settings: AudioSettingsState {
                backend: settings_audio_backend,
                active_backend,
                catalog: audio_catalog,
                backend_preferences: audio_backend_preferences,
                preferred_input_name: preferred_audio_input,
                preferred_output_name: preferred_audio_output,
                active_output_name,
                sample_rate,
                buffer_size: requested_buffer_size,
                catalog_error,
            },
            auto_warp_on_import: ui_settings.auto_warp_on_import,
            warp_confidence_threshold: ui_settings.warp_confidence_threshold,
            confirm_project_track_deletion: ui_settings.confirm_project_track_deletion,
            auto_save_enabled: ui_settings.auto_save_enabled,
            update_check: crate::update_check::UpdateCheckState {
                enabled: ui_settings.check_for_updates,
                last_check_unix: ui_settings.last_update_check_unix,
                ..Default::default()
            },
            project: crate::state::ProjectState {
                recent_project_paths,
                ..Default::default()
            },
            view: crate::state::ViewState {
                perform_surface_width: ui_settings.perform_surface_width,
                detail_panel_height: ui_settings.detail_panel_height,
                mix_detail_panel_height: ui_settings.mix_detail_panel_height,
                arrange_grid: ui_settings.arrange_grid,
                clip_grid: ui_settings.clip_grid,
                ..Default::default()
            },
            browser: crate::state::BrowserState {
                open: ui_settings.sample_browser_open,
                dock_width: ui_settings.sample_browser_width.clamp(
                    crate::state::BROWSER_DOCK_MIN_WIDTH,
                    crate::state::BROWSER_DOCK_MAX_WIDTH,
                ),
                audition_enabled: ui_settings.audition_enabled,
                audition_gain: ui_settings.audition_gain.clamp(0.0, 2.0),
                roots: ui_settings.sample_library_roots,
                pins: ui_settings
                    .browser_pins
                    .iter()
                    .map(|path| MediaSourceRef::LocalFile {
                        path: PathBuf::from(path),
                    })
                    .collect(),
                recent: ui_settings
                    .browser_recent
                    .iter()
                    .map(|path| MediaSourceRef::LocalFile {
                        path: PathBuf::from(path),
                    })
                    .collect(),
                tags: restored_tags.0,
                tag_tree: restored_tags.1,
                top_tag_ids: restored_tags.2,
                retired_tag_id: ui_settings.browser_retired_tag_id,
                item_tags: ui_settings
                    .browser_item_tags
                    .into_iter()
                    .map(|stored| (stored.path, stored.tags))
                    .collect(),
                ..Default::default()
            },
            automation_ui: {
                let mut automation_ui = crate::domains::automation::AutomationState::default();
                automation_ui.set_saved_lane_heights(ui_settings.automation_lane_heights);
                automation_ui
            },
            ..Default::default()
        };
        if let Some(status) = initial_audio_status {
            state.status_text = status;
        }
        state.perform.input_mapping = ui_settings.perform_input_mapping.clone();
        state
            .perform
            .set_fixed_computer_velocity(ui_settings.fixed_computer_velocity);
        state
            .perform
            .set_track_mute_quantization(ui_settings.track_mute_quantization);

        // Themes: scan the user's .vzt collection, then restore the
        // saved selection (built-in name or user theme name).
        let (user_themes, theme_warnings) = crate::themes::scan_user_themes();
        for warning in theme_warnings {
            eprintln!("vibez: theme scan: {warning}");
        }
        state.user_themes = user_themes;
        th::set_colour_strength(ui_settings.colour_strength);
        if let Some(name) = &ui_settings.theme {
            let palette = crate::themes::builtin_by_name(name).or_else(|| {
                state
                    .user_themes
                    .iter()
                    .find(|t| t.palette.name == *name)
                    .map(|t| t.palette.clone())
            });
            if let Some(palette) = palette {
                th::set_theme(palette);
                state.current_theme_name = name.clone();
            }
        }

        let (plugin_effect_tx, plugin_effect_rx) = std::sync::mpsc::channel();
        let (plugin_instrument_tx, plugin_instrument_rx) = std::sync::mpsc::channel();

        // Register the UI thread (process main thread) as the CLAP "main thread".
        // JUCE-based CLAP plugins require GUI calls on this thread.
        vibez_plugin_host::set_clap_main_thread();

        let plugin_window_manager = PluginWindowManager::new();

        // Auto-connect to the preferred MIDI input if the saved name
        // matches one currently visible, else to the first available
        // port. Silent on failure: the user can still work without
        // MIDI, and Settings → Audio lets them pick manually later.
        let midi_ports = midi_ports::MidiPorts::restore(
            ui_settings.midi_inputs.as_deref(),
            ui_settings.preferred_midi_input.as_deref(),
        );

        let mut app = Self {
            state,
            edge_shortcuts: EdgeShortcutState::default(),
            shortcuts,
            cmd_tx: crate::domains::EngineCommandQueue::new(cmd_tx, transport),
            event_rx: Some(event_rx),
            _stream: Some(output_stream),
            sample_retirement: Default::default(),
            input_bridge,
            _input_stream: None,
            plugin_effect_rx,
            plugin_effect_tx,
            plugin_load_requests: Default::default(),
            device_meters: Default::default(),
            compressor_devices: Default::default(),
            filter_panels: Default::default(),
            gate_panels: Default::default(),
            card_panels: Default::default(),
            multiband_panels: Default::default(),
            instrument_panels: Default::default(),
            gain_devices: Default::default(),
            eq_devices: eq_devices::EqDevices {
                spectrum_rx: eq_spectrum_rx,
                ..Default::default()
            },
            widener_devices: widener_devices::WidenerDevices {
                spectrum_rx: side_spectrum_rx,
                mid_rx: mid_spectrum_rx,
                ..Default::default()
            },
            param_entry: None,
            device_presets: Default::default(),
            sidechain_sync_inputs: None,
            track_meter_peaks: Default::default(),
            pointer_poll: Default::default(),
            plugin_instrument_rx,
            plugin_instrument_tx,
            plugin_window_manager,
            plugin_gui_raw_ptrs: std::collections::HashMap::new(),
            plugin_state_ptrs: std::collections::HashMap::new(),
            plugin_remote_handles: std::collections::HashMap::new(),
            export_job: None,
            export_render_progress: None,
            export_plugin_return_rx: None,
            browser_import_request: Default::default(),
            section_residency_request: Default::default(),
            midi_ports,
            dragged_header: None,
            pending_track_render: None,
            midi_learn: Default::default(),
            hardware_keys: Default::default(),
            midi_input_ports: Vec::new(),
            interface_scale,
            save_runtime: save_runtime::SaveRuntime::default(),
        };

        // Inform the engine of the actual hardware clock and project tempo.
        app.apply_engine_sample_rate(app.state.transport.sample_rate);
        app.send_command(EngineCommand::SetBpm(app.state.transport.bpm));
        app.send_command(EngineCommand::SetMetronomeVolume(
            app.state.transport.metronome_volume,
        ));
        app.send_command(EngineCommand::SetAuditionGain(
            app.state.browser.audition_gain,
        ));

        // Console model: the master bus carries its channel EQ from
        // the first frame.
        app.ensure_master_eq();

        let roots = app.state.browser.roots.clone();
        let local_startup_task = Task::batch(roots.into_iter().map(|root| {
            let revision = app.state.browser.begin_root_scan(&root, false);
            Task::perform(scan_sample_root_async(root.clone()), move |result| {
                Message::Browser(BrowserMsg::LocalRootCatalogReconciled {
                    root: root.clone(),
                    revision,
                    result,
                })
            })
        }));
        let plugin_catalog_startup_task = if app.state.plugin_settings.cache_needs_refresh() {
            Task::done(Message::ScanPlugins)
        } else {
            Task::none()
        };

        // Staged Project Media copies are content-addressed and shared, so
        // saves and aborted imports never delete them eagerly; this sweep
        // is the cache policy that reclaims entries old enough that no
        // live session can still reference them.
        std::thread::spawn(|| {
            vibez_project::project_format_v1::sweep_stale_staging(std::time::Duration::from_secs(
                7 * 24 * 60 * 60,
            ));
        });

        // `vibez <project.vzp>` opens a project straight from the
        // command line (also how file-manager associations launch
        // us). Legacy `.vibez` files load the same way.
        // Release check. Off the UI thread via Task::perform, at most
        // once a day, and skipped entirely when the user opted out so
        // that "off" means no traffic rather than a discarded result.
        let update_check_task = if crate::update_check::should_check(
            app.state.update_check.enabled,
            app.state.update_check.last_check_unix,
            crate::update_check::now_unix(),
        ) && app.state.update_check.begin_check()
        {
            Task::perform(
                crate::update_check::fetch_latest_tag(),
                Message::UpdateCheckCompleted,
            )
        } else {
            Task::none()
        };

        let startup_project = std::env::args()
            .nth(1)
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_file());
        if startup_project.is_none() {
            app.state.project.new_project_layout = Some(vibez_project::PerformLayout::Sections);
        }
        let open_task = startup_project
            .map(|p| Task::done(Message::ProjectOpenPathSelected(Some(p))))
            .unwrap_or_else(Task::none);
        // `window::Settings` has no maximized field in iced 0.13. The
        // initial task runs after the first window exists, so ask the window
        // manager to maximize that concrete window instead of merely opening
        // at a large hard-coded size.
        let maximize_window_task =
            iced::window::get_latest().and_then(|id| iced::window::maximize(id, true));

        let catalog_task = if catalog_scanned_later {
            audio_settings::scan_audio_catalog_task(app.state.audio_settings.backend)
        } else {
            Task::none()
        };

        (
            app,
            Task::batch([
                catalog_task,
                local_startup_task,
                plugin_catalog_startup_task,
                update_check_task,
                open_task,
                maximize_window_task,
            ]),
        )
    }

    /// Find a theme by name: built-ins first, then the scanned user
    /// collection.
    pub(crate) fn resolve_theme(&self, name: &str) -> Option<crate::theme::ThemePalette> {
        crate::themes::builtin_by_name(name).or_else(|| {
            self.state
                .user_themes
                .iter()
                .find(|t| t.palette.name == name)
                .map(|t| t.palette.clone())
        })
    }

    fn send_command(&mut self, cmd: EngineCommand) {
        crate::domains::EngineHandle::send(&mut self.cmd_tx, cmd);
    }

    fn insert_built_in_effect(
        &mut self,
        track_id: vibez_core::id::TrackId,
        effect_id: vibez_core::id::EffectId,
        effect_type: vibez_core::effect::EffectType,
    ) {
        crate::domains::EngineHandle::insert_built_in_effect(
            &mut self.cmd_tx,
            track_id,
            effect_id,
            effect_type,
        );
    }

    fn set_built_in_instrument(
        &mut self,
        track_id: vibez_core::id::TrackId,
        kind: vibez_core::midi::InstrumentKind,
    ) {
        crate::domains::EngineHandle::set_built_in_instrument(&mut self.cmd_tx, track_id, kind);
    }

    fn mark_project_dirty(&mut self) {
        self.state.project.dirty = true;
        self.save_runtime.project_changed(
            self.state.auto_save_enabled,
            self.state.project.current_path.is_some(),
            std::time::Instant::now(),
        );
    }

    /// Whether Perform is showing a Section's own timeline.
    fn section_timeline_on_screen(&self) -> bool {
        self.state.view.workspace == crate::state::Workspace::Perform
            && self.state.perform.selected_section.is_some()
    }

    /// Whether an Arrange or Section timeline is on screen.
    pub(super) fn timeline_on_screen(&self) -> bool {
        self.state.view.workspace == crate::state::Workspace::Arrange
            || self.section_timeline_on_screen()
    }

    /// Zoom of the Arrange or Section timeline on screen.
    pub(super) fn timeline_pixels_per_beat(&self) -> f32 {
        if self.section_timeline_on_screen() {
            return self
                .state
                .perform
                .section_editor
                .viewport()
                .geometry()
                .pixels_per_beat();
        }
        self.state.view.viewport.geometry().pixels_per_beat()
    }

    /// Auto-scroll the arrangement when a clip's right edge nears the visible boundary.
    /// Called from resize/move handlers so the view follows the drag.
    fn auto_scroll_to_beat(&mut self, clip_end_beat: f64) {
        // Conservative estimate of canvas width (window minus track headers)
        let canvas_width = 1400.0_f32;
        let geometry = self.state.view.viewport.geometry();
        let visible_beats = geometry.visible_beats(canvas_width);
        let visible_end = self.state.view.viewport.scroll_offset_beats + visible_beats;
        let margin = 2.0_f64;

        if clip_end_beat > visible_end - margin {
            let delta = clip_end_beat - visible_end + margin * 2.0;
            let total = self.state.total_beats();
            self.state.view.viewport.scroll_offset_beats =
                (self.state.view.viewport.scroll_offset_beats + delta).clamp(0.0, total);
        }
        // Also scroll left when dragging toward the left edge
        if clip_end_beat < self.state.view.viewport.scroll_offset_beats + margin
            && self.state.view.viewport.scroll_offset_beats > 0.0
        {
            let delta = self.state.view.viewport.scroll_offset_beats + margin - clip_end_beat;
            self.state.view.viewport.scroll_offset_beats =
                (self.state.view.viewport.scroll_offset_beats - delta).max(0.0);
        }
    }

    /// Re-evaluated by iced after every update, so the title tracks the open
    /// project and its dirty flag without anything having to push it.
    fn title(&self) -> String {
        window_policy::window_title(
            self.state.project.current_path.as_deref(),
            self.state.project.dirty,
        )
    }

    fn theme(&self) -> Theme {
        th::vibez_theme()
    }

    /// Scale of the logical coordinate space the whole interface is
    /// laid out in. iced re-lays-out on change, and widgets keep
    /// working in logical units, so canvases (timeline, piano roll,
    /// meters) receive already-descaled cursor positions and need no
    /// per-widget adjustment.
    fn scale_factor(&self) -> f64 {
        self.interface_scale as f64
    }

    /// Wall-clock interval for the UI tick. iced rebuilds the whole view
    /// and redraws the window after every message, so the tick rate is
    /// the redraw rate: a 60 fps tick costs a full redraw 60 times a
    /// second even when nothing moves. The interval therefore follows
    /// what the UI actually has to animate:
    ///
    /// - [`UI_TICK_MS`] (60 fps) while interacting: an edit drag, a
    ///   recording, an export, an open plugin GUI window, or an open
    ///   hardware MIDI input (key latency must stay sub-frame-ish), so
    ///   edge auto-scroll, plugin timers and MIDI stay smooth;
    /// - 33 ms (30 fps) while the transport plays: the playhead and
    ///   meters still move smoothly, at half the redraw cost;
    /// - 100 ms (10 fps) when idle: only time-based housekeeping runs
    ///   (meter decay, pad flashes, auto-save, draining the engine event
    ///   ring, which is empty of position changes while stopped).
    ///
    /// Every poll the tick performs still runs on every tick — only less
    /// often — so behavior is unchanged, just sampled less frequently.
    fn ui_tick_interval(&self) -> std::time::Duration {
        /// Playback tick: 30 fps.
        const PLAY_TICK_MS: u64 = 33;
        /// Idle tick: 10 fps.
        const IDLE_TICK_MS: u64 = 100;
        let playing = !matches!(
            self.state.transport.playback,
            crate::state::PlaybackPhase::Stopped,
        );
        let hot = self.state.audio_recording.is_recording()
            || self.state.perform.clip_record.session.is_some()
            || self.state.arrangement.edit_drag.is_active()
            || (self.state.view.workspace == crate::state::Workspace::Perform
                && self.state.perform.timeline_editor().edit_drag.is_active())
            || self.export_render_progress.is_some()
            || self
                .plugin_window_manager
                .as_ref()
                .is_some_and(|manager| manager.any_open())
            || !self.midi_ports.open.is_empty();
        let ms = if hot {
            UI_TICK_MS
        } else if playing || self.lfo_plot_shown() {
            PLAY_TICK_MS
        } else {
            IDLE_TICK_MS
        };
        std::time::Duration::from_millis(ms)
    }
    /// The rebindable wheel-zoom bindings plus the modifiers currently
    /// held, handed to the timeline widgets (overview, ruler, lanes, clip
    /// editors) so the same wheel gesture zooms everywhere.
    pub(super) fn wheel_zoom(&self) -> crate::shortcuts::WheelZoom {
        crate::shortcuts::wheel_zoom(&self.shortcuts, self.state.view.held_modifiers)
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(self.ui_tick_interval()).map(|_| Message::Tick),
            local_watcher::subscription(self.state.browser.roots.clone()),
            iced::event::listen_with(|event, _status, _id| match event {
                iced::Event::Keyboard(event) => keyboard_input_message(event, _status),
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::View(ViewMsg::CursorMoved(position.x, position.y)))
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::View(ViewMsg::MouseReleased)),
                iced::Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) => {
                    Some(Message::GlobalWheelScrolled(delta))
                }
                iced::Event::Window(iced::window::Event::Resized(size)) => Some(Message::View(
                    ViewMsg::WindowResized(size.width, size.height),
                )),
                iced::Event::Window(iced::window::Event::Unfocused) => {
                    Some(Message::Perform(PerformMsg::WindowUnfocused))
                }
                iced::Event::Window(iced::window::Event::CloseRequested) => {
                    Some(Message::WindowCloseRequested)
                }
                iced::Event::Window(event) => desktop_drop::desktop_drop_message(&event),
                _ => None,
            }),
        ])
    }
}

#[cfg(test)]
mod browser_multi_drag_tests;
#[cfg(test)]
mod browser_places_tests;
#[cfg(test)]
mod browser_taggable_drag_tests;
#[cfg(test)]
mod browser_tags_tests;
#[cfg(test)]
mod clip_launch_options_tests;
mod clip_launcher;
#[cfg(test)]
mod clip_launcher_resume_tests;
mod clip_record;
#[cfg(test)]
mod quick_add_tests;
#[cfg(test)]
mod ui_tick_tests;
mod views_clip_launcher;
mod views_clip_launcher_new_track;

#[cfg(test)]
mod clip_project_tests;

#[cfg(test)]
mod capture_then_play_tests;

#[cfg(test)]
mod clip_drag_undo_tests;

#[cfg(test)]
mod clip_drag_cancel_tests;
#[cfg(test)]
mod fade_drag_cancel_tests;

#[cfg(test)]
mod clip_shortcut_tests;
#[cfg(test)]
mod duplicate_shortcut_tests;

#[cfg(test)]
mod grid_shortcut_tests;

#[cfg(test)]
mod note_clip_insertion_tests;

#[cfg(test)]
mod compensation_persistence_tests;
#[cfg(test)]
mod gain_persistence_tests;

#[cfg(test)]
mod plugin_load_tests;
#[cfg(test)]
mod test_support;

pub(crate) mod plugin_load_requests;
mod plugin_loading;

mod sidechain_sync;

#[cfg(test)]
mod sidechain_sync_tests;

#[cfg(test)]
mod browser_preview_flow_tests;
#[cfg(test)]
mod browser_replay_tests;

#[cfg(test)]
mod compensation_bridge_tests;

#[cfg(test)]
mod compensation_live_timing_tests;

#[cfg(test)]
mod compensation_selection_tests;

#[cfg(test)]
mod undo_replay_routing_tests;

#[cfg(test)]
mod undo_content_swap_tests;

#[cfg(test)]
mod drag_cost_tests;

mod frozen_guard;
mod header_drag;
mod midi_effect_devices;
mod midi_input;
pub(crate) mod midi_mapping;
mod midi_ports;
mod modulator_devices;
mod stem_export;
mod track_freeze;

#[cfg(test)]
mod clip_insertion_tests;

#[cfg(test)]
mod tempo_modulation_tests;
