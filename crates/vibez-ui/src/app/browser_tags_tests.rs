//! Colour tags and tag folders of the browser library: creation rules,
//! nesting cap, removal semantics, item tagging, tag places and filters.

use std::path::PathBuf;

use vibez_core::track::MediaSourceRef;

use crate::domains::browser::BrowserMsg;
use crate::state::{
    is_selector_name, BrowserState, SpecialPlace, TAG_COLORS, TAG_FOLDER_MAX_DEPTH, TAG_LIMIT,
};

fn source(name: &str) -> MediaSourceRef {
    MediaSourceRef::LocalFile {
        path: PathBuf::from(format!("/library/{name}")),
    }
}

#[test]
fn a_fresh_library_starts_with_seven_coloured_tags() {
    let browser = BrowserState::default();
    assert_eq!(browser.tags.len(), 7);
    assert_eq!(browser.top_tag_ids, (1..=7).collect::<Vec<_>>());
    for (index, tag) in browser.tags.iter().enumerate() {
        assert_eq!(tag.id, index as u32 + 1);
        assert_eq!(tag.color, index as u8);
    }
}

#[test]
fn selector_shaped_names_are_refused_as_tag_names() {
    assert!(is_selector_name("#12"));
    assert!(is_selector_name("#1+#2"));
    assert!(!is_selector_name("#"));
    assert!(!is_selector_name("#a"));
    assert!(!is_selector_name("Red"));
    let mut browser = BrowserState::default();
    assert!(browser.make_tag("#12", 0).is_err());
    assert!(browser.make_tag("Red", 0).is_err(), "names are unique");
    assert!(browser.make_tag("", 0).is_err());
    assert_eq!(browser.tags.len(), 7, "refused names make no tag");
}

#[test]
fn making_tags_assigns_fresh_ids_and_the_next_free_colour() {
    let mut browser = BrowserState::default();
    let id = browser.make_tag("Loops", 3).unwrap();
    assert_eq!(id, 8, "ids continue past the seven defaults");
    // All seven colours are taken by the defaults, so the next one wraps.
    assert_eq!(browser.next_tag_color(), 8 % TAG_COLORS.len() as u8);
    assert!(browser.top_tag_ids.contains(&id));
    // All seven colours taken: the eighth tag wraps around.
    let mut full = BrowserState::default();
    for _ in 0..TAG_LIMIT - 7 {
        full.make_tag(&full.default_tag_name(), full.next_tag_color())
            .unwrap();
    }
    assert_eq!(full.tags.len(), TAG_LIMIT);
    assert!(full.make_tag("One too many", 0).is_err(), "the limit holds");
}

#[test]
fn tag_folders_nest_at_most_four_deep() {
    let mut browser = BrowserState::default();
    let f1 = browser.make_folder(None, "A").unwrap();
    let f2 = browser.make_folder(Some(f1), "B").unwrap();
    let f3 = browser.make_folder(Some(f2), "C").unwrap();
    let f4 = browser.make_folder(Some(f3), "D").unwrap();
    assert!(
        browser.make_folder(Some(f4), "E").is_err(),
        "a fifth level is refused"
    );
    assert_eq!(TAG_FOLDER_MAX_DEPTH, 4);
}

#[test]
fn removing_a_folder_lifts_its_children_into_the_parent() {
    let mut browser = BrowserState::default();
    let parent = browser.make_folder(None, "Parent").unwrap();
    let child = browser.make_folder(Some(parent), "Child").unwrap();
    let tag_in_child = browser.make_tag_in(Some(child), "Inner", 0).unwrap();
    let moved = browser.remove_folder(child);
    assert_eq!(moved, 1, "the child's tag moved up");
    assert!(browser.tag(tag_in_child).is_some());
    // The tag now sits in the parent folder.
    let parent_node = browser
        .tag_tree
        .iter()
        .find(|node| node.id == parent)
        .unwrap();
    assert!(parent_node.tags.contains(&tag_in_child));
    // Removing a top-level folder lifts into the top level.
    let top_tag = browser.make_tag_in(Some(parent), "Deep", 1).unwrap();
    browser.remove_folder(parent);
    assert!(browser.top_tag_ids.contains(&top_tag));
    assert!(browser.tag_tree.iter().all(|node| node.id != parent));
}

#[test]
fn removing_a_tag_detaches_it_everywhere_and_reports_carriers() {
    let mut browser = BrowserState::default();
    let red = browser.tags[0].id;
    let kick = source("kick.wav");
    browser.toggle_item_tag(&kick, red);
    assert_eq!(browser.tag_carrier_count(red), 1);
    browser.select_tag_place(vec![red]);
    browser.toggle_tag_filter(red);
    let carriers = browser.remove_tag(red);
    assert_eq!(carriers, 1);
    assert!(browser.tag(red).is_none());
    assert!(browser.tags_of(&kick).is_empty());
    assert!(browser.item_tags.is_empty(), "empty taggings are dropped");
    assert!(!matches!(browser.special_place, Some(SpecialPlace::Tags(ids)) if ids.contains(&red)));
    assert!(!browser.tag_filters.contains(&red));
}

#[test]
fn toggle_item_tag_sets_then_clears_and_reports() {
    let mut browser = BrowserState::default();
    let id = browser.tags[0].id;
    let snare = source("snare.wav");
    assert!(browser.toggle_item_tag(&snare, id));
    assert_eq!(browser.tags_of(&snare), vec![id]);
    assert!(!browser.toggle_item_tag(&snare, id));
    assert!(browser.tags_of(&snare).is_empty());
}

#[test]
fn tag_places_list_items_carrying_any_of_the_tags() {
    let mut browser = BrowserState::default();
    let red = browser.tags[0].id;
    let blue = browser.tags[4].id;
    let kick = source("kick.wav");
    let bass = source("bass.wav");
    let hat = source("hat.wav");
    browser.toggle_item_tag(&kick, red);
    browser.toggle_item_tag(&bass, blue);
    browser.select_tag_place(vec![red, blue]);
    let sources = browser.special_place_sources(&SpecialPlace::Tags(vec![red, blue]));
    assert_eq!(sources.len(), 2);
    assert!(sources.contains(&kick));
    assert!(sources.contains(&bass));
    assert!(!sources.contains(&hat));
    // Ctrl-click adds a tag to the place, then removes it.
    let green = browser.tags[3].id;
    browser.toggle_tag_in_place(green);
    match &browser.special_place {
        Some(SpecialPlace::Tags(ids)) => assert!(ids.contains(&green)),
        other => panic!("expected a tag place, got {other:?}"),
    }
    browser.toggle_tag_in_place(green);
    match &browser.special_place {
        Some(SpecialPlace::Tags(ids)) => assert!(!ids.contains(&green)),
        other => panic!("expected a tag place, got {other:?}"),
    }
    // Leaving the last tag of a place goes back to folders.
    browser.select_tag_place(vec![red]);
    browser.toggle_tag_in_place(red);
    assert!(browser.special_place.is_none());
}

#[test]
fn tag_filters_pass_only_carriers_and_are_any_of() {
    let mut browser = BrowserState::default();
    let red = browser.tags[0].id;
    let blue = browser.tags[4].id;
    let kick = source("kick.wav");
    let hat = source("hat.wav");
    browser.toggle_item_tag(&kick, red);
    assert!(browser.passes_tag_filters(&kick), "no filter passes all");
    assert!(browser.passes_tag_filters(&hat));
    assert!(browser.toggle_tag_filter(red));
    assert!(browser.passes_tag_filters(&kick));
    assert!(!browser.passes_tag_filters(&hat));
    browser.toggle_tag_filter(blue);
    assert!(
        !browser.passes_tag_filters(&hat),
        "untagged items pass no active filter"
    );
    browser.toggle_item_tag(&hat, blue);
    assert!(browser.passes_tag_filters(&hat), "any-of: hat carries blue");
    assert!(!browser.toggle_tag_filter(red));
    assert!(
        !browser.passes_tag_filters(&kick),
        "red filter is off and kick carries no blue"
    );
}

#[test]
fn browser_messages_drive_the_whole_tag_flow() {
    let mut browser = BrowserState::default();
    // New tag from the sidebar starts an inline rename.
    let action = browser.update(BrowserMsg::NewTag {
        apply_to: Vec::new(),
        folder: None,
    });
    assert!(action.persist_settings);
    let new_id = *browser.top_tag_ids.last().unwrap();
    assert_eq!(browser.renaming_tag, Some(new_id));
    // Rename it through the inline field.
    browser.update(BrowserMsg::RenameTextChanged("Favourites".into()));
    browser.update(BrowserMsg::CommitRename);
    assert_eq!(browser.tag(new_id).unwrap().name, "Favourites");
    assert!(browser.renaming_tag.is_none());
    // Tag two items through the entry menu message.
    let kick = source("kick.wav");
    let snare = source("snare.wav");
    browser.update(BrowserMsg::ToggleItemTag {
        sources: vec![kick.clone(), snare.clone()],
        tag_id: new_id,
    });
    assert_eq!(browser.tag_carrier_count(new_id), 2);
    // The tag place shows exactly those two.
    browser.update(BrowserMsg::SelectTagPlace(vec![new_id]));
    assert_eq!(
        browser
            .special_place_sources(&SpecialPlace::Tags(vec![new_id]))
            .len(),
        2
    );
    // Recolour, then remove with the two-step confirm.
    browser.update(BrowserMsg::RecolorTag {
        id: new_id,
        color: 5,
    });
    assert_eq!(browser.tag(new_id).unwrap().color, 5);
    browser.update(BrowserMsg::RequestRemoveTag(new_id));
    assert_eq!(browser.confirm_remove_tag, Some(new_id));
    let action = browser.update(BrowserMsg::RemoveTag(new_id));
    assert!(action.persist_settings);
    assert!(browser.tag(new_id).is_none());
    assert_eq!(browser.tag_carrier_count(new_id), 0);
}

#[test]
fn new_tag_folder_starts_expanded_and_renaming() {
    let mut browser = BrowserState::default();
    browser.update(BrowserMsg::NewTagFolder { parent: None });
    let folder_id = browser.tag_tree.last().unwrap().id;
    assert!(browser.expanded_tag_folders.contains(&folder_id));
    assert_eq!(browser.renaming_folder, Some(folder_id));
    browser.update(BrowserMsg::RenameTextChanged("Drums".into()));
    browser.update(BrowserMsg::CommitRename);
    assert_eq!(browser.tag_tree.last().unwrap().name, "Drums");
    // A tag made inside the folder lands in it.
    let tag_id = browser.make_tag_in(Some(folder_id), "Kicks", 0).unwrap();
    assert!(browser.tag_tree.last().unwrap().tags.contains(&tag_id));
}
