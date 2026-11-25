//! Undo/redo buttons in the control bar (between the workspace switch and
//! the transport) and Undo/Redo items in the Edit menu, driven through the
//! real view.

use iced::{mouse, Point};

use super::test_support::app;
use super::view_harness::events_at;
use super::*;
use crate::domains::arrangement::ArrangementMsg;
use crate::domains::project::ProjectMsg;
use crate::domains::transport::TransportMsg;
use crate::domains::view::ViewMsg;
use crate::state::Workspace;

fn click_events(x: f32, y: f32) -> Vec<iced::Event> {
    vec![
        iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(x, y),
        }),
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]
}

fn click_messages(app: &App, x: f32, y: f32) -> Vec<Message> {
    events_at(app, Point::new(x, y), &click_events(x, y))
}

/// The x of the first control-bar widget that clicks to `pred`, scanning the
/// top bar(s) left to right.
fn find_x(app: &App, pred: impl Fn(&Message) -> bool) -> Option<f32> {
    let mut x = 0.0;
    while x < 1600.0 {
        if click_messages(app, x, 20.0).iter().any(|m| pred(m)) {
            return Some(x);
        }
        x += 2.0;
    }
    None
}

/// An app with one undo step and one redo step on the history stacks, so
/// both control-bar buttons are enabled.
fn app_with_edit() -> App {
    let mut app = app();
    app.state.view.window_width = 1600.0;
    app.state.view.window_height = 800.0;
    app.state.view.workspace = Workspace::Arrange;
    let _ = app.update(Message::Arrangement(ArrangementMsg::AddTrack));
    let _ = app.update(Message::Arrangement(ArrangementMsg::AddTrack));
    let _ = app.update(Message::Project(ProjectMsg::Undo));
    app
}

#[test]
fn undo_redo_buttons_sit_between_workspace_switch_and_transport() {
    let app = app_with_edit();
    assert!(app.state.project.history.can_undo());
    assert!(app.state.project.history.can_redo());

    let workspace_x = find_x(&app, |m| {
        matches!(
            m,
            Message::View(ViewMsg::SwitchWorkspace(Workspace::Perform))
        )
    })
    .expect("the workspace switch is in the bar");
    let undo_x = find_x(&app, |m| matches!(m, Message::Project(ProjectMsg::Undo)))
        .expect("an enabled Undo button is in the bar");
    let redo_x = find_x(&app, |m| matches!(m, Message::Project(ProjectMsg::Redo)))
        .expect("an enabled Redo button is in the bar");
    let play_x = find_x(&app, |m| {
        matches!(
            m,
            Message::Transport(TransportMsg::Play | TransportMsg::Stop)
        )
    })
    .expect("the transport is in the bar");

    assert!(
        workspace_x < undo_x && undo_x < redo_x && redo_x < play_x,
        "undo/redo must sit between the workspace switch ({workspace_x}) and the transport ({play_x}), got undo {undo_x}, redo {redo_x}"
    );
}

#[test]
fn undo_button_is_disabled_without_history() {
    let mut app = app();
    app.state.view.window_width = 1600.0;
    app.state.view.window_height = 800.0;
    app.state.view.workspace = Workspace::Arrange;
    assert!(!app.state.project.history.can_undo());
    assert!(
        find_x(&app, |m| matches!(m, Message::Project(ProjectMsg::Undo))).is_none(),
        "no Undo message may fire while the undo stack is empty"
    );
}

#[test]
fn edit_menu_has_undo_and_redo_items() {
    let mut app = app_with_edit();
    app.state.view.edit_menu_open = true;

    let mut found_undo = false;
    let mut found_redo = false;
    let mut y = 40.0;
    while y < 500.0 && !(found_undo && found_redo) {
        let mut x = 0.0;
        while x < 320.0 {
            for m in click_messages(&app, x, y) {
                if let Message::MenuItemSelected(_, inner) = &m {
                    if matches!(**inner, Message::Project(ProjectMsg::Undo)) {
                        found_undo = true;
                    }
                    if matches!(**inner, Message::Project(ProjectMsg::Redo)) {
                        found_redo = true;
                    }
                }
            }
            x += 4.0;
        }
        y += 4.0;
    }
    assert!(found_undo, "the Edit menu must contain an Undo item");
    assert!(found_redo, "the Edit menu must contain a Redo item");
}
