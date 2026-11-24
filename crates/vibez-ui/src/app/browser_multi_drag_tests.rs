//! Multi-row drags: pressing a selected row drags the whole selection, the
//! label counts the rows, and the drop handlers receive every source.

use std::path::PathBuf;

use vibez_core::track::MediaSourceRef;

use crate::domains::browser::BrowserMsg;
use crate::state::{BrowserState, SampleBrowserEntry};

fn source(name: &str) -> MediaSourceRef {
    MediaSourceRef::LocalFile {
        path: PathBuf::from(format!("/library/{name}")),
    }
}

fn entry(name: &str, duration_seconds: Option<f64>) -> SampleBrowserEntry {
    SampleBrowserEntry {
        source: source(name),
        name: name.to_string(),
        root_path: PathBuf::from("/library"),
        relative_path: PathBuf::from(name),
        format: "wav".into(),
        duration_seconds,
        channels: Some(2),
        sample_rate: Some(44_100),
        file_size: Some(1024),
        modified: None,
        search_text: name.to_lowercase(),
    }
}

#[test]
fn a_multi_row_pending_drag_labels_and_carries_every_source() {
    let mut browser = BrowserState::default();
    let sources = vec![source("kick.wav"), source("snare.wav"), source("hat.wav")];
    browser.update(BrowserMsg::BeginPendingDragMany {
        sources: sources.clone(),
        label: "kick.wav".into(),
        origin_x: 0.0,
        origin_y: 0.0,
    });
    // Below the drag threshold nothing moves yet.
    assert!(!browser.move_pending_drag(2.0, 0.0));
    assert!(browser.drag_source.is_none());
    // Past it, the drag carries all three rows.
    assert!(browser.move_pending_drag(40.0, 0.0));
    assert_eq!(
        usize::from(browser.drag_source.is_some()) + browser.drag_extra.len(),
        3
    );
    assert_eq!(browser.drag_label.as_deref(), Some("kick.wav +2"));
    assert_eq!(browser.take_drag_sources(), sources);
    assert!(browser.drag_source.is_none() && browser.drag_extra.is_empty());
}

#[test]
fn a_single_row_drag_keeps_its_plain_label() {
    let mut browser = BrowserState::default();
    browser.update(BrowserMsg::BeginPendingDrag {
        source: source("kick.wav"),
        label: "kick.wav".into(),
        origin_x: 0.0,
        origin_y: 0.0,
    });
    assert!(browser.move_pending_drag(40.0, 0.0));
    assert_eq!(browser.drag_label.as_deref(), Some("kick.wav"));
    assert_eq!(browser.take_drag_sources().len(), 1);
}

#[test]
fn entry_durations_place_a_multi_drop_one_after_another() {
    let mut browser = BrowserState::default();
    browser.entries = vec![
        entry("kick.wav", Some(0.5)),
        entry("snare.wav", Some(0.25)),
        entry("mystery.wav", None),
    ];
    assert_eq!(
        browser.entry_duration_samples(&source("kick.wav"), 48_000),
        24_000
    );
    assert_eq!(
        browser.entry_duration_samples(&source("snare.wav"), 48_000),
        12_000
    );
    // Unknown duration falls back to zero (the clip stacks at the drop beat).
    assert_eq!(
        browser.entry_duration_samples(&source("mystery.wav"), 48_000),
        0
    );
    assert_eq!(
        browser.entry_duration_samples(&source("gone.wav"), 48_000),
        0
    );
}

#[test]
fn the_new_track_group_memo_is_cleared_by_the_next_drag() {
    let mut browser = BrowserState::default();
    browser.new_track_group = Some((1, vibez_core::id::TrackId::new()));
    browser.update(BrowserMsg::BeginPendingDrag {
        source: source("kick.wav"),
        label: "kick.wav".into(),
        origin_x: 0.0,
        origin_y: 0.0,
    });
    assert!(browser.new_track_group.is_none());
}
