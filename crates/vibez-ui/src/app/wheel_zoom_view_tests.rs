//! Wheel zoom through the real view: the rebindable wheel binding
//! (Ctrl+WheelUp/Down by default) zooms the overview, the ruler and lanes,
//! and the open clip editor — the same gesture everywhere.

use std::sync::Arc;

use iced::{mouse, Point};
use vibez_core::id::{ClipId, TrackId};
use vibez_core::midi::{MidiNote, TrackKind};
use vibez_core::perform::GrooveGrid;

use super::test_support::app;
use super::view_harness::events_at;
use super::*;
use crate::domains::view::ViewMsg;
use crate::shortcuts::{KeyCombo, ShortcutAction};
use crate::state::{ProjectTrack, UiNoteClip, Workspace};

fn arrange_with_open_clip() -> (App, TrackId, ClipId) {
    let mut app = app();
    app.state.view.workspace = Workspace::Arrange;
    app.state.view.window_width = 1200.0;
    app.state.view.window_height = 800.0;
    let mut track = ProjectTrack::new(TrackId::new(), "Keys".into(), 0);
    track.kind = TrackKind::Midi;
    let id = track.id;
    Arc::make_mut(&mut app.state.project_tracks)
        .tracks
        .push(track);
    let clip_id = ClipId::new();
    app.state
        .arrange_content_mut(id)
        .note_clips
        .push(UiNoteClip {
            id: clip_id,
            name: "Pattern".into(),
            position_beats: 0.0,
            duration_beats: 16.0,
            notes: vec![MidiNote {
                pitch: 60,
                velocity: 100,
                start_beat: 0.0,
                duration_beats: 8.0,
            }],
            selected_notes: Default::default(),
            start_marker_beats: 0.0,
            loop_enabled: false,
            loop_start_beats: 0.0,
            loop_end_beats: 16.0,
            groove_grid: GrooveGrid::Off,
            scale: None,
            color_index: None,
        });
    app.state.arrangement.selected_track = Some(id);
    app.state.arrangement.selected_note_clip = Some((id, clip_id));
    (app, id, clip_id)
}

/// A wheel-up turn preceded by the modifiers the app currently holds, as the
/// window delivers them (the canvas states track them from these events).
fn wheel_up_with(modifiers: iced::keyboard::Modifiers) -> Vec<iced::Event> {
    vec![
        iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)),
        iced::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 3.0 },
        }),
    ]
}

fn messages_at(app: &App, y: f32) -> Vec<Message> {
    events_at(
        app,
        Point::new(700.0, y),
        &wheel_up_with(app.state.view.held_modifiers),
    )
}

fn is_zoom(msg: &Message) -> bool {
    matches!(
        msg,
        Message::View(ViewMsg::ZoomAround { .. } | ViewMsg::SetClipEditorView { .. })
    )
}

fn is_pan(msg: &Message) -> bool {
    matches!(msg, Message::View(ViewMsg::ScrollArrangement(_)))
}

/// The y bands of the Arrange view at 1200x800: overview ~110-144,
/// ruler+lanes ~144-560, detail panel (open clip editor) below.
#[test]
fn ctrl_wheel_zooms_overview_ruler_lanes_and_clip_editor() {
    let (mut app, _id, _clip) = arrange_with_open_clip();
    // The app tracks held modifiers from keyboard events; set them directly.
    app.state.view.held_modifiers = iced::keyboard::Modifiers::CTRL;

    let overview = messages_at(&app, 120.0);
    assert!(
        overview.iter().any(is_zoom),
        "ctrl+wheel over the overview must zoom, got {overview:?}"
    );

    let lanes = messages_at(&app, 200.0);
    assert!(
        lanes.iter().any(is_zoom),
        "ctrl+wheel over the lanes must zoom, got {lanes:?}"
    );

    let ruler = messages_at(&app, 150.0);
    assert!(
        ruler.iter().any(is_zoom),
        "ctrl+wheel over the ruler must zoom, got {ruler:?}"
    );

    let clip_editor = messages_at(&app, 650.0);
    assert!(
        clip_editor.iter().any(is_zoom),
        "ctrl+wheel over the open clip editor must zoom, got {clip_editor:?}"
    );
}

#[test]
fn plain_wheel_still_pans_and_does_not_zoom() {
    let (mut app, _id, _clip) = arrange_with_open_clip();
    app.state.view.held_modifiers = iced::keyboard::Modifiers::empty();

    for y in [120.0, 200.0] {
        let msgs = messages_at(&app, y);
        assert!(
            msgs.iter().any(is_pan) && !msgs.iter().any(is_zoom),
            "plain wheel at y={y} must pan without zooming, got {msgs:?}"
        );
    }
}

#[test]
fn shift_wheel_still_zooms_the_lanes() {
    let (mut app, _id, _clip) = arrange_with_open_clip();
    app.state.view.held_modifiers = iced::keyboard::Modifiers::SHIFT;

    let lanes = messages_at(&app, 200.0);
    assert!(
        lanes.iter().any(is_zoom),
        "shift+wheel over the lanes must keep zooming, got {lanes:?}"
    );
}

#[test]
fn a_rebound_wheel_binding_drives_the_zoom_everywhere() {
    let (mut app, _id, _clip) = arrange_with_open_clip();
    // Rebind overview zoom-in to plain wheel-up.
    app.shortcuts.set_binding(
        ShortcutAction::OverviewZoomIn,
        vec![KeyCombo::new(
            crate::shortcuts::WHEEL_UP,
            false,
            false,
            false,
        )],
    );
    app.state.view.held_modifiers = iced::keyboard::Modifiers::empty();

    let lanes = messages_at(&app, 200.0);
    assert!(
        lanes.iter().any(is_zoom),
        "the rebound plain wheel-up must zoom the lanes, got {lanes:?}"
    );
    let overview = messages_at(&app, 120.0);
    assert!(
        overview.iter().any(is_zoom),
        "the rebound plain wheel-up must zoom the overview, got {overview:?}"
    );

    // Ctrl+wheel no longer zooms once rebound away.
    app.state.view.held_modifiers = iced::keyboard::Modifiers::CTRL;
    let lanes = messages_at(&app, 200.0);
    assert!(
        !lanes.iter().any(is_zoom) && lanes.iter().any(is_pan),
        "ctrl+wheel must pan after the rebind, got {lanes:?}"
    );
}
