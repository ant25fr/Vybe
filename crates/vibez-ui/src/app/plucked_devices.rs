//! The native Plucked String card on the visible device chain: one knob
//! column per parameter, like the simple built-in effects.

use iced::{Color, Element};
use vibez_core::id::TrackId;

use crate::message::Message;
use crate::state::UiTrack;
use crate::widgets::plucked_device::PluckedView;

use super::*;

impl App {
    /// The Plucked String card at `width`.
    pub(super) fn plucked_card<'a>(
        &'a self,
        track_id: TrackId,
        track: &'a UiTrack,
        accent: Color,
        width: f32,
    ) -> Element<'a, Message> {
        self.tabbed_instrument_card(
            "Plucked String",
            track_id,
            track,
            accent,
            width,
            move |device, _panel, entry, size| {
                PluckedView {
                    track_id,
                    device,
                    accent,
                    entry,
                    sample_rate: self.state.transport.sample_rate,
                }
                .view(size)
            },
        )
    }
}
