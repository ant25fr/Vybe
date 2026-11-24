//! The Ctrl+P quick-add palette: binding, open/close, filtering, arrow-key
//! movement and what Enter adds.

use vibez_core::midi::InstrumentKind;

use super::test_support::app;
use crate::message::{Message, QuickAddItem};
use crate::shortcuts::{KeyCombo, ShortcutAction};
use crate::state::QuickAddKind;

#[test]
fn quick_add_binds_ctrl_p_and_toggles_the_palette() {
    assert_eq!(ShortcutAction::QuickAdd.id(), "quick-add");
    assert!(ShortcutAction::QuickAdd
        .default_combos()
        .contains(&KeyCombo::new("p", true, false, false)));
    assert!(matches!(
        ShortcutAction::QuickAdd.message(),
        Message::OpenQuickAdd
    ));
    let mut app = app();
    assert!(app.state.view.quick_add.is_none());
    let _ = app.update(Message::OpenQuickAdd);
    assert!(app.state.view.quick_add.is_some());
    let _ = app.update(Message::OpenQuickAdd);
    assert!(app.state.view.quick_add.is_none(), "Ctrl+P toggles closed");
}

#[test]
fn the_query_is_kept_between_openings() {
    let mut app = app();
    let _ = app.update(Message::OpenQuickAdd);
    let _ = app.update(Message::QuickAddQueryChanged("delay".into()));
    let _ = app.update(Message::CloseQuickAdd);
    let _ = app.update(Message::OpenQuickAdd);
    assert_eq!(app.state.view.quick_add.as_ref().unwrap().query, "delay");
}

#[test]
fn the_list_filters_by_query_and_kind_and_arrows_wrap() {
    let mut app = app();
    let _ = app.update(Message::OpenQuickAdd);
    let items = app.quick_add_items(app.state.view.quick_add.as_ref().unwrap());
    assert!(items
        .iter()
        .any(|item| matches!(item, QuickAddItem::Instrument(InstrumentKind::Sampler))));
    let _ = app.update(Message::QuickAddQueryChanged("sampler".into()));
    let state = app.state.view.quick_add.clone().unwrap();
    let items = app.quick_add_items(&state);
    assert!(!items.is_empty());
    assert!(items
        .iter()
        .all(|item| item.label().to_lowercase().contains("sampler")));
    // Arrows move the highlight and wrap around the list.
    let _ = app.update(Message::QuickAddMove(1));
    assert_eq!(
        app.state.view.quick_add.as_ref().unwrap().selected,
        1 % items.len().max(1)
    );
    let _ = app.update(Message::QuickAddMove(-1));
    let _ = app.update(Message::QuickAddMove(-1));
    assert_eq!(
        app.state.view.quick_add.as_ref().unwrap().selected,
        items.len().saturating_sub(1)
    );
    // The kind chips narrow the list.
    let _ = app.update(Message::QuickAddQueryChanged(String::new()));
    let _ = app.update(Message::QuickAddSetKind(QuickAddKind::Instruments));
    let state = app.state.view.quick_add.clone().unwrap();
    let items = app.quick_add_items(&state);
    assert!(items
        .iter()
        .all(|item| matches!(item, QuickAddItem::Instrument(_) | QuickAddItem::Plugin(_))));
    let _ = app.update(Message::QuickAddSetKind(QuickAddKind::Samples));
    let state = app.state.view.quick_add.clone().unwrap();
    assert!(app
        .quick_add_items(&state)
        .iter()
        .all(|item| matches!(item, QuickAddItem::Sample(_))));
}

#[test]
fn enter_adds_the_highlighted_instrument_to_a_new_track() {
    let mut app = app();
    let _ = app.update(Message::OpenQuickAdd);
    let _ = app.update(Message::QuickAddQueryChanged("sampler".into()));
    let _ = app.update(Message::QuickAddSetKind(QuickAddKind::Instruments));
    let _ = app.update(Message::QuickAddSubmit);
    assert!(
        app.state.view.quick_add.is_none(),
        "submit closes the palette"
    );
    let track_id = app
        .state
        .arrangement
        .selected_track
        .expect("a track was made and selected");
    let track = app.state.find_track(track_id).expect("track exists");
    assert!(track.kind.is_midi());
    assert!(track.has_instrument, "the Sampler was set on the new track");
}
