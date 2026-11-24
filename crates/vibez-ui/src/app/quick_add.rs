//! The Ctrl+P quick-add palette: its item list (built-in instruments,
//! effects, cached plugins, browser samples) and what Enter does with the
//! highlighted row.

use iced::Task;

use vibez_core::id::TrackId;

use crate::domains::arrangement::ArrangementMsg;
use crate::domains::devices::DevicesMsg;
use crate::message::{BrowserImportTarget, Message, QuickAddItem};
use crate::state::{QuickAddKind, QuickAddState};

use super::*;

impl App {
    /// The palette's rows for the current query and kind filter.
    pub(super) fn quick_add_items(&self, state: &QuickAddState) -> Vec<QuickAddItem> {
        let query = state.query.trim().to_lowercase();
        let matches = |name: &str| query.is_empty() || name.to_lowercase().contains(&query);
        let mut items: Vec<QuickAddItem> = Vec::new();
        if state.kind == QuickAddKind::All || state.kind == QuickAddKind::Instruments {
            for &kind in vibez_core::midi::InstrumentKind::all() {
                if matches(kind.name()) {
                    items.push(QuickAddItem::Instrument(kind));
                }
            }
            for plugin in &self.state.plugin_settings.cache {
                if plugin.category.is_instrument() && matches(&plugin.name) {
                    items.push(QuickAddItem::Plugin(plugin.clone()));
                }
            }
        }
        if state.kind == QuickAddKind::All || state.kind == QuickAddKind::Effects {
            for &effect in vibez_core::effect::EffectType::all() {
                if matches(effect.name()) {
                    items.push(QuickAddItem::Effect(effect));
                }
            }
            for plugin in &self.state.plugin_settings.cache {
                if plugin.category.is_effect() && matches(&plugin.name) {
                    items.push(QuickAddItem::Plugin(plugin.clone()));
                }
            }
        }
        if state.kind == QuickAddKind::Plugins {
            for plugin in &self.state.plugin_settings.cache {
                if matches(&plugin.name) {
                    items.push(QuickAddItem::Plugin(plugin.clone()));
                }
            }
        }
        if state.kind == QuickAddKind::All || state.kind == QuickAddKind::Samples {
            let limit = if state.kind == QuickAddKind::Samples {
                usize::MAX
            } else {
                30
            };
            let mut samples: Vec<QuickAddItem> = self
                .state
                .browser
                .entries
                .iter()
                .filter(|entry| matches(&entry.name) || matches(&entry.search_text))
                .take(limit)
                .map(|entry| QuickAddItem::Sample(entry.source.clone()))
                .collect();
            samples.sort_by_key(|item| item.label().to_lowercase());
            items.extend(samples);
        }
        items
    }

    /// Close the palette, keeping its query for the next opening.
    pub(super) fn close_quick_add(&mut self) {
        if let Some(state) = self.state.view.quick_add.take() {
            self.state.view.quick_add_last_query = state.query;
        }
    }

    /// Add one palette row to the project: a device lands on the selected
    /// track of its kind (a new one is made when none fits), a sample is
    /// imported at the playhead.
    pub(super) fn quick_add_apply(&mut self, item: QuickAddItem) -> Task<Message> {
        match item {
            QuickAddItem::Instrument(kind) => {
                let track = self.selected_or_new_track(true);
                self.update(Message::set_track_instrument(track, kind))
            }
            QuickAddItem::Effect(effect) => {
                let track = self.selected_or_new_track(false);
                self.update(Message::Devices(DevicesMsg::AddEffect(track, effect)))
            }
            QuickAddItem::Plugin(plugin) => {
                let wants_midi = plugin.category.is_instrument() && !plugin.category.is_effect();
                let track = self.selected_or_new_track(wants_midi);
                self.update(Message::AddPluginToTrack(track, plugin.id))
            }
            QuickAddItem::Sample(source) => {
                let position_samples = self.state.transport.position_samples;
                let media = crate::domains::browser::DragMedia::of(&source);
                let selected_lane = self.state.arrangement.selected_track.filter(|track_id| {
                    self.state
                        .find_track(*track_id)
                        .is_some_and(|track| media.lands_on(track.kind))
                });
                let target = match selected_lane {
                    Some(track_id) => BrowserImportTarget::ArrangementClipAt {
                        track_id,
                        position_samples,
                    },
                    None => BrowserImportTarget::ArrangementNewTrackAt {
                        position_samples,
                        group: None,
                    },
                };
                self.dispatch_drop_for_target(source, target)
            }
        }
    }

    /// The selected track when it fits `midi`, else a fresh track of that
    /// kind (which becomes the selected one).
    fn selected_or_new_track(&mut self, midi: bool) -> TrackId {
        let selected = self
            .state
            .active_timeline_editor()
            .selected_track
            .filter(|track_id| {
                self.state
                    .find_track(*track_id)
                    .is_some_and(|track| track.kind.is_midi() == midi)
            });
        if let Some(track_id) = selected {
            return track_id;
        }
        let add = if midi {
            Message::Arrangement(ArrangementMsg::AddInstrumentTrack)
        } else {
            Message::Arrangement(ArrangementMsg::AddTrack)
        };
        let _ = self.update(add);
        self.state
            .active_timeline_editor()
            .selected_track
            .unwrap_or_else(TrackId::new)
    }
}
