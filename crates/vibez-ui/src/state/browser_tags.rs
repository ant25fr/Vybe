//! Colour tags and tag folders of the browser library, persisted in
//! ui.json: tags have a never-reused id, a unique name and one of seven
//! colours; tag folders nest them (four levels deep at most) and removing
//! a folder lifts its whole cortège into the parent.

use serde::{Deserialize, Serialize};

use super::{browser_path_string, BrowserState, MediaSourceRef, SpecialPlace};

/// The seven colours a tag starts with, as indices into [`TAG_COLORS`].
pub const TAG_COLOR_NAMES: [&str; 7] =
    ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Grey"];

/// The tag palette: the seven named colours first.
pub const TAG_COLORS: [iced::Color; 7] = [
    iced::Color::from_rgb(0.90, 0.30, 0.30),
    iced::Color::from_rgb(0.95, 0.60, 0.20),
    iced::Color::from_rgb(0.90, 0.80, 0.25),
    iced::Color::from_rgb(0.35, 0.80, 0.40),
    iced::Color::from_rgb(0.30, 0.55, 0.95),
    iced::Color::from_rgb(0.65, 0.45, 0.90),
    iced::Color::from_rgb(0.60, 0.62, 0.66),
];

/// How many tags the library holds by default limit.
pub const TAG_LIMIT: usize = 64;
/// Tag folders nest at most this deep.
pub const TAG_FOLDER_MAX_DEPTH: usize = 4;
/// A tag or folder name is 1 to 64 bytes.
pub const TAG_NAME_MAX_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagDef {
    pub id: u32,
    pub name: String,
    pub color: u8,
}

/// One tag folder: folders first, then tag ids, each level in its own order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TagFolderNode {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub folders: Vec<TagFolderNode>,
    #[serde(default)]
    pub tags: Vec<u32>,
}

/// Anything a browser row can carry a colour tag: a sample, a built-in
/// instrument, an effect, or a plug-in. Devices key into [`BrowserState::item_tags`]
/// with a prefixed key so sample paths stay bare (as saved before devices
/// could be tagged).
#[derive(Debug, Clone, PartialEq)]
pub enum BrowserTaggable {
    Sample(MediaSourceRef),
    Instrument(vibez_core::midi::InstrumentKind),
    Effect(vibez_core::effect::EffectType),
    Plugin(vibez_plugin_host::PluginId),
}

impl BrowserTaggable {
    /// The item_tags key of this item.
    pub fn key(&self) -> String {
        match self {
            BrowserTaggable::Sample(source) => browser_path_string(source),
            BrowserTaggable::Instrument(kind) => {
                format!("instrument:{}", kebab(kind.name()))
            }
            BrowserTaggable::Effect(effect) => format!("effect:{}", kebab(effect.name())),
            BrowserTaggable::Plugin(id) => format!(
                "plugin:{}:{}",
                format!("{:?}", id.format).to_lowercase(),
                id.uid
            ),
        }
    }

    pub fn label(&self) -> String {
        match self {
            BrowserTaggable::Sample(source) => source.display_name(),
            BrowserTaggable::Instrument(kind) => kind.name().to_string(),
            BrowserTaggable::Effect(effect) => effect.name().to_string(),
            BrowserTaggable::Plugin(id) => format!("plugin {}", id.uid),
        }
    }
}

fn kebab(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// Whether an item_tags key belongs to a device rather than a sample path.
fn is_device_key(key: &str) -> bool {
    key.starts_with("instrument:") || key.starts_with("effect:") || key.starts_with("plugin:")
}

/// The persisted per-item tagging: an absolute path and the tag ids it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredItemTags {
    pub path: String,
    pub tags: Vec<u32>,
}

/// Whether `name` spells an id selector (`#12`, `#1+#2`): those shapes are
/// reserved for referring to tags by id and are refused as names.
pub fn is_selector_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('#') else {
        return false;
    };
    !rest.is_empty()
        && rest.split('+').all(|part| {
            let digits = part.strip_prefix('#').unwrap_or(part);
            !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
        })
}

fn find_folder_mut<'a>(nodes: &'a mut [TagFolderNode], id: u32) -> Option<&'a mut TagFolderNode> {
    for node in nodes.iter_mut() {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_folder_mut(&mut node.folders, id) {
            return Some(found);
        }
    }
    None
}

fn folder_depth(nodes: &[TagFolderNode], id: u32) -> Option<usize> {
    fn walk(nodes: &[TagFolderNode], id: u32, depth: usize) -> Option<usize> {
        for node in nodes {
            if node.id == id {
                return Some(depth);
            }
            if let Some(found) = walk(&node.folders, id, depth + 1) {
                return Some(found);
            }
        }
        None
    }
    walk(nodes, id, 1)
}

fn find_parent_id(nodes: &[TagFolderNode], id: u32) -> Option<u32> {
    fn walk(nodes: &[TagFolderNode], id: u32) -> Option<Option<u32>> {
        for node in nodes {
            if node.folders.iter().any(|child| child.id == id) {
                return Some(Some(node.id));
            }
            if let Some(found) = walk(&node.folders, id) {
                return Some(found);
            }
        }
        None
    }
    walk(nodes, id).flatten()
}

impl BrowserState {
    /// The seven tags a fresh library starts with.
    pub fn default_tags() -> Vec<TagDef> {
        TAG_COLOR_NAMES
            .iter()
            .enumerate()
            .map(|(index, name)| TagDef {
                id: index as u32 + 1,
                name: name.to_string(),
                color: index as u8,
            })
            .collect()
    }

    /// The saved tag library: `(tags, folders, top-level tag ids)`. With
    /// nothing saved it is the seven colours; a library the user emptied
    /// stays empty. A tag saved in no folder is listed at the top.
    pub fn restore_tags(
        saved: Option<Vec<TagDef>>,
        tree: Vec<TagFolderNode>,
        mut top: Vec<u32>,
    ) -> (Vec<TagDef>, Vec<TagFolderNode>, Vec<u32>) {
        let tags = match saved {
            Some(tags) => tags,
            None => Self::default_tags(),
        };
        fn filed(nodes: &[TagFolderNode], id: u32) -> bool {
            nodes
                .iter()
                .any(|node| node.tags.contains(&id) || filed(&node.folders, id))
        }
        for tag in &tags {
            if !top.contains(&tag.id) && !filed(&tree, tag.id) {
                top.push(tag.id);
            }
        }
        (tags, tree, top)
    }

    /// The next free id, shared by tags and folders and never reused.
    pub fn next_tag_id(&self) -> u32 {
        let mut max = 0u32;
        for tag in &self.tags {
            max = max.max(tag.id);
        }
        fn walk(node: &TagFolderNode, max: &mut u32) {
            *max = (*max).max(node.id);
            for child in &node.folders {
                walk(child, max);
            }
        }
        for node in &self.tag_tree {
            walk(node, &mut max);
        }
        max.max(self.retired_tag_id).saturating_add(1)
    }

    /// Remembers the highest id in use, ahead of removing one.
    fn retire_tag_ids(&mut self) {
        self.retired_tag_id = self.next_tag_id() - 1;
    }

    /// Whether `name` is free for a new tag, or for the tag `renaming`.
    fn name_is_usable(&self, name: &str, renaming: Option<u32>) -> Result<(), String> {
        if name.is_empty() || name.len() > TAG_NAME_MAX_BYTES {
            return Err(format!("a name is 1 to {TAG_NAME_MAX_BYTES} bytes"));
        }
        if is_selector_name(name) {
            return Err("a name cannot look like a tag id selector".into());
        }
        if self
            .tags
            .iter()
            .any(|tag| tag.name == name && Some(tag.id) != renaming)
        {
            return Err(format!("a tag named {name} already exists"));
        }
        Ok(())
    }

    /// Make a tag at the top level; returns its id.
    pub fn make_tag(&mut self, name: &str, color: u8) -> Result<u32, String> {
        if self.tags.len() >= TAG_LIMIT {
            return Err(format!("the library holds at most {TAG_LIMIT} tags"));
        }
        self.name_is_usable(name, None)?;
        let id = self.next_tag_id();
        self.tags.push(TagDef {
            id,
            name: name.to_string(),
            color: color.min((TAG_COLORS.len() - 1) as u8),
        });
        self.top_tag_ids.push(id);
        Ok(id)
    }

    /// The next free colour index: the first of the seven not yet used.
    pub fn next_tag_color(&self) -> u8 {
        for index in 0..TAG_COLORS.len() {
            if !self.tags.iter().any(|tag| tag.color == index as u8) {
                return index as u8;
            }
        }
        (self.tags.len() % TAG_COLORS.len()) as u8
    }

    /// A default name for a new tag: "Tag N", numbered past any name taken.
    pub fn default_tag_name(&self) -> String {
        let mut n = self.tags.len() + 1;
        loop {
            let candidate = format!("Tag {n}");
            if !self.tags.iter().any(|tag| tag.name == candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Make a tag inside `folder` (None = top level).
    pub fn make_tag_in(
        &mut self,
        folder: Option<u32>,
        name: &str,
        color: u8,
    ) -> Result<u32, String> {
        if self.tags.len() >= TAG_LIMIT {
            return Err(format!("the library holds at most {TAG_LIMIT} tags"));
        }
        self.name_is_usable(name, None)?;
        let id = self.next_tag_id();
        self.tags.push(TagDef {
            id,
            name: name.to_string(),
            color: color.min((TAG_COLORS.len() - 1) as u8),
        });
        match folder {
            None => self.top_tag_ids.push(id),
            Some(folder_id) => {
                let node = find_folder_mut(&mut self.tag_tree, folder_id)
                    .ok_or_else(|| "that tag folder is gone".to_string())?;
                node.tags.push(id);
            }
        }
        Ok(id)
    }

    /// Make a folder inside `parent` (None = top level); nesting is capped
    /// at [`TAG_FOLDER_MAX_DEPTH`] folder levels.
    pub fn make_folder(&mut self, parent: Option<u32>, name: &str) -> Result<u32, String> {
        if name.is_empty() || name.len() > TAG_NAME_MAX_BYTES {
            return Err(format!("a name is 1 to {TAG_NAME_MAX_BYTES} bytes"));
        }
        if is_selector_name(name) {
            return Err("a name cannot look like a tag id selector".into());
        }
        let depth = match parent {
            None => 1,
            Some(id) => {
                folder_depth(&self.tag_tree, id)
                    .ok_or_else(|| "that tag folder is gone".to_string())?
                    + 1
            }
        };
        if depth > TAG_FOLDER_MAX_DEPTH {
            return Err(format!(
                "tag folders nest at most {TAG_FOLDER_MAX_DEPTH} deep"
            ));
        }
        let id = self.next_tag_id();
        let node = TagFolderNode {
            id,
            name: name.to_string(),
            folders: Vec::new(),
            tags: Vec::new(),
        };
        match parent {
            None => self.tag_tree.push(node),
            Some(parent_id) => find_folder_mut(&mut self.tag_tree, parent_id)
                .ok_or_else(|| "that tag folder is gone".to_string())?
                .folders
                .push(node),
        }
        Ok(id)
    }

    pub fn rename_tag(&mut self, id: u32, name: &str) -> Result<(), String> {
        self.name_is_usable(name, Some(id))?;
        let tag = self
            .tags
            .iter_mut()
            .find(|tag| tag.id == id)
            .ok_or_else(|| "that tag is gone".to_string())?;
        tag.name = name.to_string();
        Ok(())
    }

    pub fn rename_folder(&mut self, id: u32, name: &str) -> Result<(), String> {
        if name.is_empty() || name.len() > TAG_NAME_MAX_BYTES {
            return Err(format!("a name is 1 to {TAG_NAME_MAX_BYTES} bytes"));
        }
        if is_selector_name(name) {
            return Err("a name cannot look like a tag id selector".into());
        }
        let node = find_folder_mut(&mut self.tag_tree, id)
            .ok_or_else(|| "that tag folder is gone".to_string())?;
        node.name = name.to_string();
        Ok(())
    }

    pub fn recolor_tag(&mut self, id: u32, color: u8) -> bool {
        if let Some(tag) = self.tags.iter_mut().find(|tag| tag.id == id) {
            tag.color = color.min((TAG_COLORS.len() - 1) as u8);
            true
        } else {
            false
        }
    }

    /// Remove a tag: its items lose it, and places/filters reading it go.
    /// Returns how many items carried it.
    pub fn remove_tag(&mut self, id: u32) -> usize {
        let carriers = self.tag_carrier_count(id);
        self.retire_tag_ids();
        self.tags.retain(|tag| tag.id != id);
        self.top_tag_ids.retain(|top| *top != id);
        fn strip(node: &mut TagFolderNode, id: u32) {
            node.tags.retain(|t| *t != id);
            for child in &mut node.folders {
                strip(child, id);
            }
        }
        for node in &mut self.tag_tree {
            strip(node, id);
        }
        for tags in self.item_tags.values_mut() {
            tags.retain(|t| *t != id);
        }
        let item_tags = std::mem::take(&mut self.item_tags);
        self.item_tags = item_tags
            .into_iter()
            .filter(|(_, tags)| !tags.is_empty())
            .collect();
        if let Some(SpecialPlace::Tags(ids)) = &mut self.special_place {
            ids.retain(|t| *t != id);
        }
        self.tag_filters.retain(|t| *t != id);
        carriers
    }

    /// Remove a folder, lifting its tags and subfolders into its parent
    /// (the top level when it was top). Returns how many rows moved up.
    pub fn remove_folder(&mut self, id: u32) -> usize {
        fn detach(nodes: &mut Vec<TagFolderNode>, id: u32) -> Option<TagFolderNode> {
            if let Some(index) = nodes.iter().position(|n| n.id == id) {
                return Some(nodes.remove(index));
            }
            for node in nodes.iter_mut() {
                if let Some(found) = detach(&mut node.folders, id) {
                    return Some(found);
                }
            }
            None
        }
        self.retire_tag_ids();
        // Find the parent before detaching.
        let parent = find_parent_id(&self.tag_tree, id);
        let Some(mut node) = detach(&mut self.tag_tree, id) else {
            return 0;
        };
        let moved = node.folders.len() + node.tags.len();
        match parent {
            None => {
                self.tag_tree.append(&mut node.folders);
                self.top_tag_ids.append(&mut node.tags);
            }
            Some(parent_id) => {
                if let Some(parent_node) = find_folder_mut(&mut self.tag_tree, parent_id) {
                    parent_node.folders.append(&mut node.folders);
                    parent_node.tags.append(&mut node.tags);
                }
            }
        }
        self.expanded_tag_folders.remove(&id);
        moved
    }

    pub fn tag(&self, id: u32) -> Option<&TagDef> {
        self.tags.iter().find(|tag| tag.id == id)
    }

    pub fn tag_color(color: u8) -> iced::Color {
        TAG_COLORS[(color as usize).min(TAG_COLORS.len() - 1)]
    }

    /// How many items carry this tag.
    pub fn tag_carrier_count(&self, id: u32) -> usize {
        self.item_tags
            .values()
            .filter(|tags| tags.contains(&id))
            .count()
    }

    /// The tag ids an item carries.
    pub fn tags_of(&self, source: &MediaSourceRef) -> Vec<u32> {
        self.item_tags
            .get(&browser_path_string(source))
            .cloned()
            .unwrap_or_default()
    }

    /// Set or clear one tag on an item; returns the tag's new state.
    pub fn toggle_item_tag(&mut self, source: &MediaSourceRef, tag_id: u32) -> bool {
        let key = browser_path_string(source);
        // Sources with no path of their own (cloud, project media) would
        // all share one empty key, and each other's tags.
        if key.is_empty() {
            return false;
        }
        let tags = self.item_tags.entry(key).or_default();
        if let Some(index) = tags.iter().position(|t| *t == tag_id) {
            tags.remove(index);
            false
        } else {
            tags.push(tag_id);
            true
        }
    }

    /// The tag ids a taggable item (sample, instrument, effect, plugin) carries.
    pub fn tags_of_taggable(&self, item: &BrowserTaggable) -> Vec<u32> {
        self.item_tags.get(&item.key()).cloned().unwrap_or_default()
    }

    /// Set a tag on a taggable item; returns false when it already carried it.
    pub fn set_taggable_tag(&mut self, item: &BrowserTaggable, tag_id: u32) -> bool {
        let key = item.key();
        if key.is_empty() {
            return false;
        }
        let tags = self.item_tags.entry(key).or_default();
        if tags.contains(&tag_id) {
            false
        } else {
            tags.push(tag_id);
            true
        }
    }

    /// Set or clear one tag on a taggable item; returns the tag's new state.
    pub fn toggle_taggable_tag(&mut self, item: &BrowserTaggable, tag_id: u32) -> bool {
        let key = item.key();
        if key.is_empty() {
            return false;
        }
        let tags = self.item_tags.entry(key).or_default();
        if let Some(index) = tags.iter().position(|t| *t == tag_id) {
            tags.remove(index);
            false
        } else {
            tags.push(tag_id);
            true
        }
    }

    /// Whether a taggable item passes the active footer-dot tag filter.
    pub fn passes_tag_filters_taggable(&self, item: &BrowserTaggable) -> bool {
        self.tag_filters.is_empty()
            || self
                .tags_of_taggable(item)
                .iter()
                .any(|t| self.tag_filters.contains(t))
    }

    /// Every item carrying any of `ids` (a tag place), as sources.
    pub fn items_with_any_tags(&self, ids: &[u32]) -> Vec<MediaSourceRef> {
        self.item_tags
            .iter()
            .filter(|(path, _)| !is_device_key(path))
            .filter(|(_, tags)| tags.iter().any(|t| ids.contains(t)))
            .map(|(path, _)| MediaSourceRef::LocalFile {
                path: std::path::PathBuf::from(path),
            })
            .collect()
    }

    /// Whether a source passes the active footer-dot tag filter (any-of).
    pub fn passes_tag_filters(&self, source: &MediaSourceRef) -> bool {
        self.tag_filters.is_empty()
            || self
                .tags_of(source)
                .iter()
                .any(|t| self.tag_filters.contains(t))
    }

    /// Toggle a footer-dot filter; returns whether it is now active.
    pub fn toggle_tag_filter(&mut self, id: u32) -> bool {
        if let Some(index) = self.tag_filters.iter().position(|t| *t == id) {
            self.tag_filters.remove(index);
            false
        } else {
            self.tag_filters.push(id);
            true
        }
    }

    /// Select a tag place: items carrying any of `ids`. An empty id list
    /// leaves the place.
    pub fn select_tag_place(&mut self, ids: Vec<u32>) {
        if ids.is_empty() {
            self.special_place = None;
        } else {
            self.select_special_place(Some(SpecialPlace::Tags(ids)));
        }
    }

    /// A default name for a new folder, numbered past any name taken.
    pub fn default_folder_name(&self) -> String {
        fn has_name(nodes: &[TagFolderNode], name: &str) -> bool {
            nodes
                .iter()
                .any(|node| node.name == name || has_name(&node.folders, name))
        }
        let mut n = self.tag_tree.len() + 1;
        loop {
            let candidate = format!("Folder {n}");
            if !has_name(&self.tag_tree, &candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Collapse or expand a tag folder in the sidebar.
    pub fn toggle_tag_folder_expanded(&mut self, id: u32) {
        if !self.expanded_tag_folders.remove(&id) {
            self.expanded_tag_folders.insert(id);
        }
    }

    /// Ctrl-click on a tag row in the sidebar: add the tag to the current
    /// tag place, or remove it; leaving the last tag shows the tree again.
    pub fn toggle_tag_in_place(&mut self, id: u32) {
        match &self.special_place {
            Some(SpecialPlace::Tags(ids)) if ids.contains(&id) => {
                let mut ids = ids.clone();
                ids.retain(|t| *t != id);
                self.select_tag_place(ids);
            }
            Some(SpecialPlace::Tags(ids)) => {
                let mut ids = ids.clone();
                ids.push(id);
                self.select_tag_place(ids);
            }
            _ => self.select_tag_place(vec![id]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_library_lists_the_seven_colours_and_an_emptied_one_stays_empty() {
        let (tags, _, top) = BrowserState::restore_tags(None, Vec::new(), Vec::new());
        assert_eq!(tags.len(), 7);
        assert_eq!(top, (1..=7).collect::<Vec<_>>());
        let (tags, _, top) = BrowserState::restore_tags(Some(Vec::new()), Vec::new(), Vec::new());
        assert!(tags.is_empty() && top.is_empty());
    }

    #[test]
    fn a_tag_keeps_its_name_through_a_rename_but_cannot_take_another() {
        let mut browser = BrowserState::default();
        assert!(browser.rename_tag(1, "Red").is_ok());
        assert!(browser.rename_tag(1, "Blue").is_err());
    }

    #[test]
    fn a_removed_tag_or_folder_id_is_not_given_out_again() {
        let mut browser = BrowserState::default();
        let tag = browser.make_tag("Mine", 0).unwrap();
        browser.remove_tag(tag);
        let folder = browser.make_folder(None, "Kits").unwrap();
        assert!(folder > tag);
        browser.remove_folder(folder);
        assert!(browser.make_tag("Again", 0).unwrap() > folder);
    }

    #[test]
    fn sources_without_a_path_do_not_share_tags() {
        let mut browser = BrowserState::default();
        let cloud = |name: &str| {
            BrowserTaggable::Sample(MediaSourceRef::LocalFile {
                path: std::path::PathBuf::from(name),
            })
        };
        assert!(browser.set_taggable_tag(&cloud("kick.wav"), 1));
        // An empty key stands for no path at all.
        assert!(!browser.set_taggable_tag(&cloud(""), 1));
        assert!(browser.tags_of_taggable(&cloud("")).is_empty());
    }
}
