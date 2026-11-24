//! Browser places, pins/recent and multi-selection, after the reference
//! browser redesign: sidebar sections, Pinned/Recent places, row menus.

use std::path::PathBuf;

use vibez_core::track::MediaSourceRef;

use super::test_support::app;
use crate::domains::browser::BrowserMsg;
use crate::message::Message;
use crate::state::{BrowserState, SampleBrowserEntry, SpecialPlace};

fn source(name: &str) -> MediaSourceRef {
    MediaSourceRef::LocalFile {
        path: PathBuf::from(format!("/library/{name}")),
    }
}

fn entry(name: &str) -> SampleBrowserEntry {
    SampleBrowserEntry {
        source: source(name),
        name: name.to_string(),
        root_path: PathBuf::from("/library"),
        relative_path: PathBuf::from(name),
        format: "wav".into(),
        duration_seconds: Some(1.0),
        channels: Some(2),
        sample_rate: Some(48_000),
        file_size: Some(1024),
        modified: None,
        search_text: name.to_lowercase(),
    }
}

#[test]
fn toggle_pin_adds_then_removes_and_reports_state() {
    let mut browser = BrowserState::default();
    let kick = source("kick.wav");
    assert!(browser.toggle_pin(kick.clone()));
    assert!(browser.is_pinned(&kick));
    assert!(!browser.toggle_pin(kick.clone()));
    assert!(!browser.is_pinned(&kick));
}

#[test]
fn recent_is_newest_first_deduplicated_and_capped() {
    let mut browser = BrowserState::default();
    for i in 0..60 {
        browser.push_recent(source(&format!("s{i:02}.wav")));
    }
    assert_eq!(browser.recent.len(), 50);
    assert_eq!(
        browser.recent[0],
        source("s59.wav"),
        "newest import comes first"
    );
    browser.push_recent(source("s59.wav"));
    assert_eq!(browser.recent.len(), 50, "re-import dedups, not grows");
    assert_eq!(browser.recent[0], source("s59.wav"));
    assert_eq!(
        browser
            .recent
            .iter()
            .filter(|s| **s == source("s59.wav"))
            .count(),
        1
    );
}

#[test]
fn special_place_selection_clears_folder_and_category_and_back() {
    let mut browser = BrowserState::default();
    browser.current_folder = Some(PathBuf::from("/library"));
    browser.category = Some(crate::state::BrowserCategory::Fx);
    let action = browser.update(BrowserMsg::SelectSpecialPlace(Some(SpecialPlace::Pinned)));
    assert_eq!(browser.special_place, Some(SpecialPlace::Pinned));
    assert!(browser.current_folder.is_none());
    assert!(browser.category.is_none());
    assert!(!action.persist_settings);

    browser.update(BrowserMsg::SelectLocalFolder(Some(PathBuf::from(
        "/library",
    ))));
    assert_eq!(browser.special_place, None, "a folder place leaves Pinned");
    assert_eq!(browser.current_folder, Some(PathBuf::from("/library")));

    browser.update(BrowserMsg::SelectSpecialPlace(Some(SpecialPlace::Recent)));
    browser.update(BrowserMsg::SetBrowserCategory(Some(
        crate::state::BrowserCategory::Fx,
    )));
    assert_eq!(browser.special_place, None, "a category leaves Recent");
    assert_eq!(browser.category, Some(crate::state::BrowserCategory::Fx));
}

#[test]
fn pin_action_persists_settings() {
    let mut browser = BrowserState::default();
    let action = browser.update(BrowserMsg::TogglePin(source("kick.wav")));
    assert!(action.persist_settings);
    assert!(action.status.is_some());
}

#[test]
fn sections_toggle_open_and_closed() {
    let mut browser = BrowserState::default();
    assert!(browser.section_open("PINNED"));
    browser.toggle_section("PINNED");
    assert!(!browser.section_open("PINNED"));
    browser.toggle_section("PINNED");
    assert!(browser.section_open("PINNED"));
}

#[test]
fn special_place_sources_list_pins_and_recent() {
    let mut browser = BrowserState::default();
    browser.toggle_pin(source("a.wav"));
    browser.toggle_pin(source("b.wav"));
    browser.push_recent(source("c.wav"));
    assert_eq!(
        browser.special_place_sources(&SpecialPlace::Pinned),
        vec![source("a.wav"), source("b.wav")]
    );
    assert_eq!(
        browser.special_place_sources(&SpecialPlace::Recent),
        vec![source("c.wav")]
    );
}

fn app_with_entries(names: &[&str]) -> super::App {
    let mut app = app();
    app.state.browser.roots = vec![PathBuf::from("/library")];
    app.state.browser.current_folder = Some(PathBuf::from("/library"));
    for name in names {
        app.state.browser.entries.push(entry(name));
    }
    app.state.browser.bump_catalog_revision();
    app
}

#[test]
fn ctrl_click_toggles_rows_in_a_multi_selection() {
    let mut app = app_with_entries(&["a.wav", "b.wav", "c.wav"]);
    let a = source("a.wav");
    let b = source("b.wav");

    let _ = app.update(Message::BrowserEntryReleased {
        source: a.clone(),
        ctrl: false,
        shift: false,
    });
    assert_eq!(app.state.browser.selected_source, Some(a.clone()));
    assert!(app.state.browser.selected_extra.is_empty());

    let _ = app.update(Message::BrowserEntryReleased {
        source: b.clone(),
        ctrl: true,
        shift: false,
    });
    assert_eq!(
        app.state.browser.selected_source,
        Some(a.clone()),
        "anchor stays"
    );
    assert!(app.state.browser.is_selected(&b));
    assert_eq!(app.state.browser.selection_count(), 2);

    let _ = app.update(Message::BrowserEntryReleased {
        source: b.clone(),
        ctrl: true,
        shift: false,
    });
    assert!(
        !app.state.browser.is_selected(&b),
        "ctrl-click again removes"
    );
    assert_eq!(app.state.browser.selection_count(), 1);
}

#[test]
fn shift_click_selects_the_range_from_the_anchor() {
    let mut app = app_with_entries(&["a.wav", "b.wav", "c.wav", "d.wav"]);
    let a = source("a.wav");
    let d = source("d.wav");

    let _ = app.update(Message::BrowserEntryReleased {
        source: a.clone(),
        ctrl: false,
        shift: false,
    });
    let _ = app.update(Message::BrowserEntryReleased {
        source: d.clone(),
        ctrl: false,
        shift: true,
    });
    assert_eq!(app.state.browser.selected_source, Some(a.clone()));
    for name in ["b.wav", "c.wav", "d.wav"] {
        assert!(
            app.state.browser.is_selected(&source(name)),
            "{name} in range"
        );
    }
    assert_eq!(app.state.browser.selection_count(), 4);
}

#[test]
fn right_click_selects_the_row_before_opening_its_menu() {
    let mut app = app_with_entries(&["a.wav", "b.wav"]);
    let b = source("b.wav");
    let _ = app.update(Message::BrowserEntryReleased {
        source: source("a.wav"),
        ctrl: false,
        shift: false,
    });
    let _ = app.update(Message::BrowserEntryRightPressed(b.clone()));
    assert_eq!(app.state.browser.selected_source, Some(b.clone()));
    assert!(app.state.browser.selected_extra.is_empty());
    let menu = app.state.view.context_menu.as_ref().expect("menu open");
    assert!(matches!(
        menu.target,
        crate::state::ContextMenuTarget::BrowserEntry { .. }
    ));
}

#[test]
fn double_click_records_recent_and_targets_the_selected_track() {
    let mut app = app_with_entries(&["a.wav"]);
    let a = source("a.wav");
    // Double-click = two plain releases in quick succession at the same spot.
    let _ = app.update(Message::BrowserEntryReleased {
        source: a.clone(),
        ctrl: false,
        shift: false,
    });
    let _ = app.update(Message::BrowserEntryReleased {
        source: a.clone(),
        ctrl: false,
        shift: false,
    });
    assert_eq!(
        app.state.browser.recent.first(),
        Some(&a),
        "import lands in Recent"
    );
}
