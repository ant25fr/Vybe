//! Clip and track colors from the right-click context menus: audio clips get
//! the same color swatches as MIDI patterns, and the track header menu offers
//! the track palette.

use std::sync::Arc;

use iced::{mouse, Point};

use super::test_support::app;
use super::view_harness::events_at;
use super::*;
use crate::domains::arrangement::ArrangementMsg;
use crate::state::{ContextMenu, ContextMenuTarget, ProjectTrack, UiClip, Workspace};
use vibez_core::id::TrackId;

fn app_with_audio_clip() -> (App, TrackId, vibez_core::id::ClipId) {
    let mut app = app();
    app.state.view.window_width = 1600.0;
    app.state.view.window_height = 800.0;
    app.state.view.workspace = Workspace::Arrange;
    let track = ProjectTrack::new(TrackId::new(), "Audio 1".into(), 0);
    let track_id = track.id;
    Arc::make_mut(&mut app.state.project_tracks)
        .tracks
        .push(track);
    let clip = UiClip::test_clip(0, 48_000);
    let clip_id = clip.id;
    app.state.arrange_content_mut(track_id).clips.push(clip);
    (app, track_id, clip_id)
}

fn click_events(x: f32, y: f32) -> Vec<iced::Event> {
    vec![
        iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(x, y),
        }),
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]
}

/// Click-scan a band of the open context menu for an item whose message
/// matches. `x0..x1`, `y0..y1` bracket the rows to scan.
fn menu_has_item(
    app: &App,
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    pred: impl Fn(&Message) -> bool,
) -> bool {
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            let msgs = events_at(app, Point::new(x, y), &click_events(x, y));
            if msgs.iter().any(|m| pred(m)) {
                return true;
            }
            x += 5.0;
        }
        y += 3.0;
    }
    false
}

/// If a message carries SetTrackColor, return its color index.
fn track_color_in(msg: &Message) -> Option<u8> {
    if let Message::MenuItemSelected(_, inner) = msg {
        if let Message::Arrangement(ArrangementMsg::SetTrackColor { color_index, .. }) = &**inner {
            return Some(*color_index);
        }
    }
    None
}

/// Does a message carry this arrangement action?
fn carries(msg: &Message, pred: impl Fn(&ArrangementMsg) -> bool) -> bool {
    matches!(msg, Message::MenuItemSelected(_, inner)
        if matches!(&**inner, Message::Arrangement(action) if pred(action)))
}

#[test]
fn audio_clip_context_menu_recolors_the_clip() {
    let (mut app, track_id, clip_id) = app_with_audio_clip();
    app.state.view.context_menu = Some(ContextMenu {
        x: 100.0,
        y: 100.0,
        target: ContextMenuTarget::Clip {
            track_id,
            clip_id,
            is_note_clip: false,
        },
    });

    // The menu opens at (100, 100); the swatch rows sit below the five
    // text items and the "Color" caption.
    let found = menu_has_item(&app, 100.0, 320.0, 250.0, 320.0, |m| {
        carries(m, |a| matches!(a, ArrangementMsg::SetAudioClipColor { .. }))
    });
    assert!(found, "the audio clip menu must offer color swatches");

    // The message actually recolors the clip and marks the project dirty.
    let _ = app.update(Message::Arrangement(ArrangementMsg::SetAudioClipColor {
        track_id,
        clip_id,
        color_index: Some(5),
    }));
    let clip = app
        .state
        .arrange_content(track_id)
        .and_then(|c| c.clips.iter().find(|c| c.id == clip_id))
        .unwrap();
    assert_eq!(clip.color_index, Some(5));
    assert!(app.state.project.dirty);
}

#[test]
fn track_header_menu_recolors_the_track() {
    let (mut app, track_id, _clip_id) = app_with_audio_clip();
    app.state.view.context_menu = Some(ContextMenu {
        x: 20.0,
        y: 200.0,
        target: ContextMenuTarget::TrackHeader { track_id },
    });

    // The header menu opens at (20, 200): Delete Track, then "Color", then
    // the swatch row.
    let found = menu_has_item(&app, 20.0, 220.0, 235.0, 300.0, |m| {
        carries(m, |a| matches!(a, ArrangementMsg::SetTrackColor { .. }))
    });
    assert!(found, "the track header menu must offer the track palette");

    let _ = app.update(Message::Arrangement(ArrangementMsg::SetTrackColor {
        track_id,
        color_index: 3,
    }));
    assert_eq!(
        app.state.find_track(track_id).map(|t| t.color_index),
        Some(3)
    );
    assert!(app.state.project.dirty);
}

#[test]
fn full_chain_right_click_lane_swatch_recolors_clip() {
    use super::view_harness::bounds_of;
    use crate::domains::desktop_pointer::PointerTarget;
    let (mut app, track_id, clip_id) = app_with_audio_clip();
    let lane = bounds_of(&app, PointerTarget::Lane(track_id)).expect("lane bounds");
    let cx = lane.x + 20.0;
    let cy = lane.y + lane.height / 2.0;

    // 1. Real right-click on the clip in the lane.
    let right_click = vec![
        iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(cx, cy),
        }),
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
    ];
    let msgs = events_at(&app, Point::new(cx, cy), &right_click);
    let show = msgs
        .into_iter()
        .find_map(|m| match m {
            Message::View(ViewMsg::ShowContextMenu { x, y, target }) => Some((x, y, target)),
            _ => None,
        })
        .expect("right-click opens a context menu");
    let (mx, my, target) = show;
    eprintln!("menu at ({mx},{my}) target {target:?}");
    let _ = app.update(Message::View(ViewMsg::ShowContextMenu {
        x: mx,
        y: my,
        target,
    }));
    assert!(app.state.view.context_menu.is_some());

    // 2. Click-scan the open menu for a swatch, take its message, run it.
    let mut picked = None;
    let mut y = my + 4.0;
    'outer: while y < my + 400.0 {
        let mut x = mx + 4.0;
        while x < mx + 240.0 {
            let msgs = events_at(&app, Point::new(x, y), &click_events(x, y));
            for m in msgs {
                if carries(&m, |a| {
                    matches!(a, ArrangementMsg::SetAudioClipColor { .. })
                }) {
                    picked = Some(m);
                    break 'outer;
                }
            }
            x += 4.0;
        }
        y += 3.0;
    }
    let picked = picked.expect("a color swatch in the open clip menu");
    eprintln!("picked: {picked:?}");
    let _ = app.update(picked);
    let clip = app
        .state
        .arrange_content(track_id)
        .and_then(|c| c.clips.iter().find(|c| c.id == clip_id))
        .unwrap();
    eprintln!("clip color_index after: {:?}", clip.color_index);
    assert!(
        clip.color_index.is_some(),
        "the swatch click must recolor the clip"
    );
}

#[test]
fn full_chain_right_click_header_swatch_recolors_track() {
    use super::view_harness::bounds_of;
    use crate::domains::desktop_pointer::PointerTarget;
    let (mut app, track_id, _clip_id) = app_with_audio_clip();
    let lane = bounds_of(&app, PointerTarget::Lane(track_id)).expect("lane bounds");
    // The header sits left of the lane, same row.
    // The header occupies the rightmost part of the area left of the lane.
    let hx = lane.x - 60.0;
    let hy = lane.y + lane.height / 2.0;

    let right_click = vec![
        iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(hx, hy),
        }),
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
    ];
    let msgs = events_at(&app, Point::new(hx, hy), &right_click);
    let show = msgs
        .into_iter()
        .find_map(|m| match m {
            Message::View(ViewMsg::ShowContextMenu { x, y, target }) => Some((x, y, target)),
            _ => None,
        })
        .expect("right-click on the header opens a context menu");
    let (mx, my, target) = show;
    eprintln!("menu target: {target:?}");
    assert!(
        matches!(target, ContextMenuTarget::TrackHeader { .. }),
        "header right-click must open the TrackHeader menu, got {target:?}"
    );
    let _ = app.update(Message::View(ViewMsg::ShowContextMenu {
        x: mx,
        y: my,
        target,
    }));

    // Pick a swatch whose index differs from the current track color (0).
    let mut picked = None;
    let mut y = my + 4.0;
    'outer: while y < my + 200.0 {
        let mut x = mx + 4.0;
        while x < mx + 200.0 {
            let msgs = events_at(&app, Point::new(x, y), &click_events(x, y));
            for m in msgs {
                if let Some(idx) = track_color_in(&m) {
                    if idx != 0 {
                        picked = Some((m, idx));
                        break 'outer;
                    }
                }
            }
            x += 4.0;
        }
        y += 3.0;
    }
    let (picked, idx) = picked.expect("a color swatch in the open header menu");
    let _ = app.update(picked);
    let color = app.state.find_track(track_id).map(|t| t.color_index);
    eprintln!("track color after: {color:?} (clicked swatch {idx})");
    assert_eq!(color, Some(idx), "the swatch click must recolor the track");
}
