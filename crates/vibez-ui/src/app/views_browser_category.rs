//! Device categories (Instruments, Plugins) shown in the browser's results
//! pane. Split from views_browser.rs; inherent methods on [`super::App`].

use iced::widget::{column, container, mouse_area, row, text};
use iced::{Element, Length};

use crate::icons;
use crate::message::Message;
use crate::state::{BrowserCategory, BrowserTaggable};
use crate::theme as th;
use vibez_core::effect::EffectType;
use vibez_core::midi::InstrumentKind;
use vibez_plugin_host::{PluginCategory, PluginFormat, PluginInfo};

use super::views_browser::tag_dots_row;
use super::views_browser_style::*;
use super::*;

const CATEGORY_ICON_SIZE: f32 = 13.0;
const CATEGORY_ROW_H: f32 = 26.0;

/// One row in a browser category: an icon, its name, its tag dots and a small
/// badge. A plain click runs `on_press` (add the device to the selected
/// track); dragging the row onto a tag row of the TAGS section tags the
/// device, so the row body is a plain container that lets the surrounding
/// mouse_area take the press.
fn category_row<'a>(
    icon: char,
    name: String,
    badge: String,
    on_press: Option<Message>,
    taggable: BrowserTaggable,
    tag_dots: Vec<iced::Color>,
) -> Element<'a, Message> {
    let content = row![
        icons::icon(icon)
            .size(CATEGORY_ICON_SIZE)
            .color(th::text_dim()),
        text(name.clone())
            .size(12)
            .color(th::text())
            .width(Length::Fill),
        tag_dots_row(tag_dots),
        text(badge).size(9).color(th::text_muted()),
    ]
    .spacing(7)
    .align_y(iced::Alignment::Center);
    let body = container(content)
        .center_y(CATEGORY_ROW_H)
        .padding([0, 7])
        .width(Length::Fill);
    mouse_area(body)
        .on_press(Message::BeginPendingTaggableDrag(taggable.clone(), name))
        .on_release(Message::TaggableDragReleased {
            item: taggable.clone(),
            click: on_press.map(Box::new),
        })
        .on_right_press(Message::BrowserTaggableRightPressed(taggable))
        .into()
}

/// The small badge under a plugin's name: its format and what it can be.
fn plugin_badge(plugin: &PluginInfo) -> String {
    let format = match plugin.format {
        PluginFormat::Clap => "CLAP",
        PluginFormat::Vst3 => "VST3",
    };
    let category = match plugin.category {
        PluginCategory::Effect => "fx",
        PluginCategory::Instrument => "inst",
        PluginCategory::Both => "fx+inst",
    };
    format!("{format} {category}")
}

impl App {
    /// The Instruments or Plugins category in the results pane. Each entry
    /// adds its device to the selected track, so the browser can load a
    /// synth or a plugin without opening the device panel's menu.
    pub(super) fn view_browser_category(&self, category: BrowserCategory) -> Element<'_, Message> {
        let search_lower = self.state.browser.search.trim().to_lowercase();
        let matches_search = |name: &str, vendor: &str| {
            search_lower.is_empty()
                || name.to_lowercase().contains(&search_lower)
                || vendor.to_lowercase().contains(&search_lower)
        };
        let track_id = self.state.active_timeline_editor().selected_track;

        // A device row shows the dots of the tags it carries, and hides when
        // the active footer-dot filter excludes it.
        let tag_dots_of = |item: &BrowserTaggable| -> Vec<iced::Color> {
            self.state
                .browser
                .tags_of_taggable(item)
                .iter()
                .filter_map(|id| self.state.browser.tag(*id))
                .map(|tag| crate::state::BrowserState::tag_color(tag.color))
                .collect()
        };
        let passes_tags =
            |item: &BrowserTaggable| self.state.browser.passes_tag_filters_taggable(item);

        let mut list = column![
            browser_caption(category.caption()),
            self.view_tag_filter_dots()
        ]
        .spacing(1);

        match category {
            BrowserCategory::Instruments => {
                for &kind in InstrumentKind::all() {
                    let name = kind.name();
                    if !matches_search(name, "") {
                        continue;
                    }
                    let item = BrowserTaggable::Instrument(kind);
                    if !passes_tags(&item) {
                        continue;
                    }
                    let dots = tag_dots_of(&item);
                    list = list.push(category_row(
                        icons::MUSIC,
                        name.to_string(),
                        "INSTRUMENT".into(),
                        track_id.map(|track_id| Message::set_track_instrument(track_id, kind)),
                        item,
                        dots,
                    ));
                }
                let mut plugins: Vec<&PluginInfo> = self
                    .state
                    .plugin_settings
                    .cache
                    .iter()
                    .filter(|plugin| plugin.category.is_instrument())
                    .filter(|plugin| matches_search(&plugin.name, &plugin.vendor))
                    .collect();
                plugins.sort_by_key(|plugin| plugin.name.to_lowercase());
                for plugin in plugins {
                    let item = BrowserTaggable::Plugin(plugin.id.clone());
                    if !passes_tags(&item) {
                        continue;
                    }
                    let dots = tag_dots_of(&item);
                    list = list.push(category_row(
                        icons::PLUG,
                        plugin.name.clone(),
                        plugin_badge(plugin),
                        track_id
                            .map(|track_id| Message::AddPluginToTrack(track_id, plugin.id.clone())),
                        item,
                        dots,
                    ));
                }
            }
            BrowserCategory::Fx => {
                for &effect in EffectType::all() {
                    let name = effect.name();
                    if !matches_search(name, "") {
                        continue;
                    }
                    let item = BrowserTaggable::Effect(effect);
                    if !passes_tags(&item) {
                        continue;
                    }
                    let dots = tag_dots_of(&item);
                    list = list.push(category_row(
                        icons::SLIDERS_VERTICAL,
                        name.to_string(),
                        "EFFECT".into(),
                        track_id.map(|track_id| Message::add_effect(track_id, effect)),
                        item,
                        dots,
                    ));
                }
                let mut plugins: Vec<&PluginInfo> = self
                    .state
                    .plugin_settings
                    .cache
                    .iter()
                    .filter(|plugin| plugin.category.is_effect())
                    .filter(|plugin| matches_search(&plugin.name, &plugin.vendor))
                    .collect();
                plugins.sort_by_key(|plugin| plugin.name.to_lowercase());
                for plugin in plugins {
                    let item = BrowserTaggable::Plugin(plugin.id.clone());
                    if !passes_tags(&item) {
                        continue;
                    }
                    let dots = tag_dots_of(&item);
                    list = list.push(category_row(
                        icons::PLUG,
                        plugin.name.clone(),
                        plugin_badge(plugin),
                        track_id
                            .map(|track_id| Message::AddPluginToTrack(track_id, plugin.id.clone())),
                        item,
                        dots,
                    ));
                }
            }
            BrowserCategory::Plugins => {
                let mut plugins: Vec<&PluginInfo> = self
                    .state
                    .plugin_settings
                    .cache
                    .iter()
                    .filter(|plugin| matches_search(&plugin.name, &plugin.vendor))
                    .collect();
                plugins.sort_by_key(|plugin| plugin.name.to_lowercase());
                if plugins.is_empty() {
                    list = list.push(
                        text("No plugins scanned yet. Use File > Settings to scan.")
                            .size(11)
                            .color(th::text_dim()),
                    );
                }
                for plugin in plugins {
                    let item = BrowserTaggable::Plugin(plugin.id.clone());
                    if !passes_tags(&item) {
                        continue;
                    }
                    let dots = tag_dots_of(&item);
                    list = list.push(category_row(
                        icons::PLUG,
                        plugin.name.clone(),
                        plugin_badge(plugin),
                        track_id
                            .map(|track_id| Message::AddPluginToTrack(track_id, plugin.id.clone())),
                        item,
                        dots,
                    ));
                }
            }
        }

        if track_id.is_none() {
            list = list.push(
                text("Select a track to add devices to it")
                    .size(10)
                    .color(th::text_dim()),
            );
        }

        container(crate::widgets::quiet_scroll::vertical(
            container(list)
                .padding(iced::Padding {
                    top: 0.0,
                    right: 8.0,
                    bottom: 8.0,
                    left: 0.0,
                })
                .width(Length::Fill),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
