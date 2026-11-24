//! Tests for the overview's view box gestures.

use iced::mouse::{Button, Cursor, Event, Interaction};
use iced::widget::canvas;
use iced::{Point, Rectangle, Size};

use super::*;

const WIDTH: f32 = 400.0;
const Y: f32 = 10.0;

/// One overview pixel per beat, showing beats 100 to 120 (x 100 to 120).
fn minimap() -> ArrangementMinimap {
    ArrangementMinimap {
        total_beats: 400.0,
        scroll_offset_beats: 100.0,
        zoom_level: 1.0,
        playhead_beats: 0.0,
        bpm: 120.0,
        loop_enabled: false,
        loop_start_beats: 0.0,
        loop_end_beats: 0.0,
        tracks: vec![MinimapTrack {
            color: iced::Color::WHITE,
            clips: vec![(0.0, 400.0)],
        }],
        wheel_zoom: crate::shortcuts::WheelZoom {
            combos: vec![
                crate::shortcuts::KeyCombo::new(crate::shortcuts::WHEEL_UP, true, false, false),
                crate::shortcuts::KeyCombo::new(crate::shortcuts::WHEEL_DOWN, true, false, false),
            ],
            modifiers: iced::keyboard::Modifiers::empty(),
        },
    }
}

fn bounds() -> Rectangle {
    Rectangle::new(Point::ORIGIN, Size::new(WIDTH, OVERVIEW_HEIGHT))
}

fn send(
    minimap: &ArrangementMinimap,
    state: &mut MinimapInteractionState,
    event: Event,
    x: f32,
) -> Option<Message> {
    <ArrangementMinimap as canvas::Program<Message>>::update(
        minimap,
        state,
        canvas::Event::Mouse(event),
        bounds(),
        Cursor::Available(Point::new(x, Y)),
    )
    .1
}

/// The beats a drag from `from` to `to` sets the view to show.
fn drag_view(from: f32, to: f32) -> Option<(f64, f64)> {
    let minimap = minimap();
    let mut state = MinimapInteractionState::default();
    send(
        &minimap,
        &mut state,
        Event::ButtonPressed(Button::Left),
        from,
    );
    let message = send(
        &minimap,
        &mut state,
        Event::CursorMoved {
            position: Point::new(to, Y),
        },
        to,
    );
    match message? {
        Message::View(ViewMsg::SetViewport(view)) => {
            let start = view.scroll_offset_beats;
            Some((start, start + view.geometry().visible_beats(WIDTH)))
        }
        _ => None,
    }
}

fn assert_shows(view: Option<(f64, f64)>, start: f64, end: f64) {
    let (shown_start, shown_end) = view.expect("a view change");
    assert!(
        (shown_start - start).abs() < 1e-3 && (shown_end - end).abs() < 1e-3,
        "shows {shown_start}..{shown_end}, expected {start}..{end}"
    );
}

#[test]
fn dragging_the_view_box_left_edge_zooms_keeping_its_right_edge() {
    assert_shows(drag_view(101.0, 60.0), 60.0, 120.0);
}

#[test]
fn dragging_the_view_box_right_edge_zooms_keeping_its_left_edge() {
    assert_shows(drag_view(119.0, 200.0), 100.0, 200.0);
}

#[test]
fn the_pointer_over_a_view_box_edge_shows_it_resizes() {
    let minimap = minimap();
    let state = MinimapInteractionState::default();
    let interaction = |x: f32| {
        <ArrangementMinimap as canvas::Program<Message>>::mouse_interaction(
            &minimap,
            &state,
            bounds(),
            Cursor::Available(Point::new(x, Y)),
        )
    };

    assert_eq!(interaction(100.0), Interaction::ResizingHorizontally);
    assert_eq!(interaction(120.0), Interaction::ResizingHorizontally);
    assert_eq!(interaction(110.0), Interaction::Grab);
}

#[test]
fn a_narrow_view_box_pans_from_its_middle() {
    let minimap = ArrangementMinimap {
        zoom_level: 10.0,
        ..minimap()
    };
    let mut state = MinimapInteractionState::default();
    send(
        &minimap,
        &mut state,
        Event::ButtonPressed(Button::Left),
        101.0,
    );
    let message = send(
        &minimap,
        &mut state,
        Event::CursorMoved {
            position: Point::new(151.0, Y),
        },
        151.0,
    );

    assert!(
        matches!(message, Some(Message::View(ViewMsg::ScrollArrangement(delta))) if (delta - 50.0).abs() < 1e-3),
        "expected a 50 beat pan, got {message:?}"
    );
}

/// Send a wheel turn at `x`, with `modifiers` held, and return the message.
fn wheel(
    minimap: &mut ArrangementMinimap,
    state: &mut MinimapInteractionState,
    delta: iced::mouse::ScrollDelta,
    modifiers: iced::keyboard::Modifiers,
    x: f32,
) -> Option<Message> {
    minimap.wheel_zoom.modifiers = modifiers;
    send(minimap, state, Event::WheelScrolled { delta }, x)
}

#[test]
fn ctrl_wheel_zooms_around_the_cursor() {
    let mut minimap = minimap();
    let mut state = MinimapInteractionState::default();
    let up = iced::mouse::ScrollDelta::Lines { x: 0.0, y: 3.0 };

    let message = wheel(
        &mut minimap,
        &mut state,
        up,
        iced::keyboard::Modifiers::CTRL,
        150.0,
    );
    match message {
        Some(Message::View(ViewMsg::ZoomAround { factor, anchor_x })) => {
            assert!(factor > 1.0, "wheel up zooms in, factor {factor}");
            assert_eq!(anchor_x, 150.0);
        }
        other => panic!("expected ZoomAround, got {other:?}"),
    }

    let down = iced::mouse::ScrollDelta::Lines { x: 0.0, y: -3.0 };
    let message = wheel(
        &mut minimap,
        &mut state,
        down,
        iced::keyboard::Modifiers::CTRL,
        200.0,
    );
    match message {
        Some(Message::View(ViewMsg::ZoomAround { factor, anchor_x })) => {
            assert!(factor < 1.0, "wheel down zooms out, factor {factor}");
            assert_eq!(anchor_x, 200.0);
        }
        other => panic!("expected ZoomAround, got {other:?}"),
    }
}

#[test]
fn plain_wheel_pans_instead_of_zooming() {
    let mut minimap = minimap();
    let mut state = MinimapInteractionState::default();
    let up = iced::mouse::ScrollDelta::Lines { x: 0.0, y: 3.0 };

    // No modifier held: the default Ctrl+Wheel binding does not match.
    let message = wheel(
        &mut minimap,
        &mut state,
        up,
        iced::keyboard::Modifiers::empty(),
        150.0,
    );
    assert!(
        matches!(message, Some(Message::View(ViewMsg::ScrollArrangement(_)))),
        "expected a pan, got {message:?}"
    );
    // Shift+Ctrl is not the binding either.
    let message = wheel(
        &mut minimap,
        &mut state,
        up,
        iced::keyboard::Modifiers::CTRL | iced::keyboard::Modifiers::SHIFT,
        150.0,
    );
    assert!(
        matches!(message, Some(Message::View(ViewMsg::ScrollArrangement(_)))),
        "expected a pan, got {message:?}"
    );
}

#[test]
fn a_rebound_wheel_combo_is_the_one_that_zooms() {
    let mut minimap = minimap();
    // Rebind zoom-in to plain wheel-up.
    minimap.wheel_zoom.combos = vec![crate::shortcuts::KeyCombo::new(
        crate::shortcuts::WHEEL_UP,
        false,
        false,
        false,
    )];
    let mut state = MinimapInteractionState::default();
    let up = iced::mouse::ScrollDelta::Lines { x: 0.0, y: 3.0 };

    let message = wheel(
        &mut minimap,
        &mut state,
        up,
        iced::keyboard::Modifiers::empty(),
        150.0,
    );
    assert!(
        matches!(message, Some(Message::View(ViewMsg::ZoomAround { .. }))),
        "expected zoom on the rebound combo, got {message:?}"
    );
    // Ctrl+wheel no longer zooms.
    let message = wheel(
        &mut minimap,
        &mut state,
        up,
        iced::keyboard::Modifiers::CTRL,
        150.0,
    );
    assert!(
        matches!(message, Some(Message::View(ViewMsg::ScrollArrangement(_)))),
        "expected a pan, got {message:?}"
    );
}
