//! Drag-and-drop tagging of browser items: samples, instruments, effects and
//! plug-ins can be dragged onto a tag row of the TAGS section to tag them.

use std::path::PathBuf;

use vibez_core::effect::EffectType;
use vibez_core::midi::InstrumentKind;
use vibez_core::track::MediaSourceRef;
use vibez_plugin_host::{PluginFormat, PluginId};

use crate::domains::browser::BrowserMsg;
use crate::state::{BrowserDropTarget, BrowserState, BrowserTaggable};

fn source(name: &str) -> MediaSourceRef {
    MediaSourceRef::LocalFile {
        path: PathBuf::from(format!("/library/{name}")),
    }
}

/// Make a fresh tag; returns its id and name.
fn make_tag(browser: &mut BrowserState) -> (u32, String) {
    let name = browser.default_tag_name();
    let color = browser.next_tag_color();
    let id = browser.make_tag(&name, color).unwrap();
    (id, name)
}

fn plugin() -> BrowserTaggable {
    BrowserTaggable::Plugin(PluginId {
        format: PluginFormat::Vst3,
        uid: "com.example.synth".into(),
    })
}

#[test]
fn taggable_keys_separate_devices_from_sample_paths() {
    assert_eq!(
        BrowserTaggable::Sample(source("kick.wav")).key(),
        "/library/kick.wav"
    );
    assert_eq!(
        BrowserTaggable::Instrument(InstrumentKind::FmSynth).key(),
        "instrument:fm-synth"
    );
    assert_eq!(
        BrowserTaggable::Effect(EffectType::Gain).key(),
        "effect:gain"
    );
    assert_eq!(plugin().key(), "plugin:vst3:com.example.synth");
    assert_eq!(
        BrowserTaggable::Instrument(InstrumentKind::FmSynth).label(),
        "FM Synth"
    );
}

#[test]
fn set_tag_and_tag_filter_apply_to_devices() {
    let mut browser = BrowserState::default();
    let (tag, _name) = make_tag(&mut browser);
    let synth = BrowserTaggable::Instrument(InstrumentKind::FmSynth);
    let gain = BrowserTaggable::Effect(EffectType::Gain);
    assert!(browser.set_taggable_tag(&synth, tag));
    assert!(!browser.set_taggable_tag(&synth, tag), "already carried");
    assert!(browser.set_taggable_tag(&gain, tag));
    assert_eq!(browser.tags_of_taggable(&synth), vec![tag]);
    // No active filter: everything passes.
    assert!(browser.passes_tag_filters_taggable(&synth));
    assert!(browser.passes_tag_filters_taggable(&plugin()));
    // With the filter on, only carriers pass.
    browser.toggle_tag_filter(tag);
    assert!(browser.passes_tag_filters_taggable(&synth));
    assert!(browser.passes_tag_filters_taggable(&gain));
    assert!(!browser.passes_tag_filters_taggable(&plugin()));
}

#[test]
fn items_with_any_tags_returns_only_samples() {
    let mut browser = BrowserState::default();
    let (tag, _name) = make_tag(&mut browser);
    browser.set_taggable_tag(&BrowserTaggable::Sample(source("kick.wav")), tag);
    browser.set_taggable_tag(&BrowserTaggable::Instrument(InstrumentKind::FmSynth), tag);
    assert_eq!(
        browser.items_with_any_tags(&[tag]),
        vec![source("kick.wav")]
    );
}

#[test]
fn a_pending_device_drag_becomes_a_tag_drag_past_the_threshold() {
    let mut browser = BrowserState::default();
    let synth = BrowserTaggable::Instrument(InstrumentKind::FmSynth);
    browser.update(BrowserMsg::BeginPendingTaggableDrag {
        item: synth.clone(),
        label: synth.label(),
        origin_x: 0.0,
        origin_y: 0.0,
    });
    assert!(browser.pending_taggable.is_some());
    // Below the threshold the press is still a click.
    let action = browser.update(BrowserMsg::PendingDragMoved { x: 2.0, y: 0.0 });
    assert!(browser.tag_drag.is_none());
    assert!(action.status.is_none());
    // Past it, the drag carries the device.
    let action = browser.update(BrowserMsg::PendingDragMoved { x: 40.0, y: 0.0 });
    assert_eq!(browser.tag_drag, Some(synth.clone()));
    assert_eq!(browser.drag_label.as_deref(), Some("FM Synth"));
    assert!(browser.drag_in_flight());
    assert_eq!(
        action.status.as_deref(),
        Some("Moving FM Synth - choose a tag row to tag it")
    );
    // A release elsewhere cancels the drag.
    let action = browser.update(BrowserMsg::EndDragSample);
    assert!(browser.tag_drag.is_none());
    assert!(!browser.drag_in_flight());
    assert_eq!(action.status.as_deref(), Some("Drag cancelled"));
}

#[test]
fn hovering_a_tag_row_only_marks_it_while_a_drag_is_in_flight() {
    let mut browser = BrowserState::default();
    let (tag, _name) = make_tag(&mut browser);
    // No drag: the hover is ignored.
    browser.update(BrowserMsg::DragHoverTagRow { tag_id: tag });
    assert!(browser.drag_target.is_none());
    // With a sample drag in flight, the row becomes the target.
    browser.update(BrowserMsg::BeginPendingDrag {
        source: source("kick.wav"),
        label: "kick.wav".into(),
        origin_x: 0.0,
        origin_y: 0.0,
    });
    browser.update(BrowserMsg::PendingDragMoved { x: 40.0, y: 0.0 });
    browser.update(BrowserMsg::DragHoverTagRow { tag_id: tag });
    assert_eq!(
        browser.drag_target,
        Some(BrowserDropTarget::TagRow { tag_id: tag })
    );
    // Leaving the row clears the target.
    browser.update(BrowserMsg::LeaveDragTarget(BrowserDropTarget::TagRow {
        tag_id: tag,
    }));
    assert!(browser.drag_target.is_none());
}

#[test]
fn dropping_on_a_tag_row_tags_every_dragged_sample() {
    let mut browser = BrowserState::default();
    let (tag, name) = make_tag(&mut browser);
    browser.update(BrowserMsg::BeginPendingDragMany {
        sources: vec![source("kick.wav"), source("snare.wav")],
        label: "kick.wav".into(),
        origin_x: 0.0,
        origin_y: 0.0,
    });
    browser.update(BrowserMsg::PendingDragMoved { x: 40.0, y: 0.0 });
    let action = browser.update(BrowserMsg::DropOnTagRow { tag_id: tag });
    assert_eq!(
        action.status.as_deref(),
        Some(format!("Tagged 2 items with {name}").as_str())
    );
    assert!(action.persist_settings);
    assert_eq!(browser.tags_of(&source("kick.wav")), vec![tag]);
    assert_eq!(browser.tags_of(&source("snare.wav")), vec![tag]);
    assert!(!browser.drag_in_flight(), "the drop consumed the drag");
}

#[test]
fn dropping_a_device_drag_on_a_tag_row_tags_the_device() {
    let mut browser = BrowserState::default();
    let (tag, name) = make_tag(&mut browser);
    let synth = BrowserTaggable::Instrument(InstrumentKind::FmSynth);
    browser.update(BrowserMsg::BeginPendingTaggableDrag {
        item: synth.clone(),
        label: synth.label(),
        origin_x: 0.0,
        origin_y: 0.0,
    });
    browser.update(BrowserMsg::PendingDragMoved { x: 40.0, y: 0.0 });
    let action = browser.update(BrowserMsg::DropOnTagRow { tag_id: tag });
    assert_eq!(
        action.status.as_deref(),
        Some(format!("Tagged 1 item with {name}").as_str())
    );
    assert!(action.persist_settings);
    assert_eq!(browser.tags_of_taggable(&synth), vec![tag]);
    assert!(!browser.drag_in_flight());
}

#[test]
fn dropping_on_a_tag_row_with_no_drag_reports_nothing() {
    let mut browser = BrowserState::default();
    let (tag, _name) = make_tag(&mut browser);
    let action = browser.update(BrowserMsg::DropOnTagRow { tag_id: tag });
    assert_eq!(action.status.as_deref(), Some("Nothing is being dragged"));
    assert!(!action.persist_settings);
}
#[test]
fn toggling_a_tag_on_a_device_removes_it_when_carried() {
    let mut browser = BrowserState::default();
    let (tag, name) = make_tag(&mut browser);
    let synth = BrowserTaggable::Instrument(InstrumentKind::FmSynth);
    let action = browser.update(BrowserMsg::ToggleTaggableTag {
        item: synth.clone(),
        tag_id: tag,
    });
    assert_eq!(browser.tags_of_taggable(&synth), vec![tag]);
    assert_eq!(
        action.status.as_deref(),
        Some(format!("Tagged FM Synth with {name}").as_str())
    );
    assert!(action.persist_settings);
    // Toggling again removes the tag.
    let action = browser.update(BrowserMsg::ToggleTaggableTag {
        item: synth.clone(),
        tag_id: tag,
    });
    assert!(browser.tags_of_taggable(&synth).is_empty());
    assert_eq!(
        action.status.as_deref(),
        Some(format!("Removed {name} from FM Synth").as_str())
    );
    assert!(action.persist_settings);
}

#[test]
fn a_new_tag_made_from_a_device_row_tags_it_and_renames_inline() {
    let mut browser = BrowserState::default();
    let synth = BrowserTaggable::Instrument(InstrumentKind::FmSynth);
    let before = browser.tags.len();
    let action = browser.update(BrowserMsg::NewTagForTaggable {
        item: synth.clone(),
        folder: None,
    });
    assert_eq!(browser.tags.len(), before + 1);
    let new_id = *browser.tags_of_taggable(&synth).first().unwrap();
    assert_eq!(browser.renaming_tag, Some(new_id));
    assert!(action.persist_settings);
    assert_eq!(
        action.status.as_deref(),
        Some(
            format!(
                "Made tag {}, tagged FM Synth",
                browser.tag(new_id).unwrap().name
            )
            .as_str()
        )
    );
}
