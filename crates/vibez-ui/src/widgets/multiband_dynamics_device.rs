//! Native Multiband Dynamics body: the per-band reduction scope above the
//! knob rows, which wrap to as many rows as the card is wide.

use std::time::{Duration, Instant};

use iced::widget::{canvas, column, container, row};
use iced::{Color, Element, Length, Size};
use vibez_core::id::TrackId;

use crate::domains::device_levels::ReadingHistory;
use crate::domains::multiband_dynamics::{MultibandPanelState, MultibandReading};
use crate::message::Message;
use crate::state::UiEffect;
use crate::theme as th;
use crate::widgets::effect_knob::{param_column, EffectKnobWidget};
use crate::widgets::multiband_dynamics_display::BandScope;

const PAD: f32 = 10.0;
/// One knob column's width including its spacing.
const KNOB_COL_W: f32 = 78.0;
/// One knob row's height, matching the shared device layout.
const KNOB_ROW_H: f32 = crate::widgets::device_layout::KNOB_ROW_H;
/// The scope's smallest and largest height, so it follows the rack's
/// height like the other DSPs' plots.
const SCOPE_MIN_H: f32 = 64.0;
const SCOPE_MAX_H: f32 = 420.0;
/// How far back the scope reaches.
const SCOPE_SPAN: Duration = Duration::from_secs(8);

pub const COMPACT_W: f32 = 420.0;
/// Two rows of knobs under the scope.
pub const MEDIUM_W: f32 = 640.0;
/// Three rows of knobs under the scope.
pub const FULL_W: f32 = 960.0;
pub const MAX_W: f32 = 1_200.0;

/// The widths this card lays itself out for.
pub const WIDTHS: crate::widgets::device_chrome::CardWidths =
    crate::widgets::device_chrome::CardWidths {
        compact: COMPACT_W,
        medium: MEDIUM_W,
        expanded: FULL_W,
        max: MAX_W,
    };

pub struct MultibandDynamicsView<'a> {
    pub track_id: TrackId,
    pub effect: &'a UiEffect,
    pub accent: Color,
    pub panel: &'a MultibandPanelState,
    pub now: Instant,
}

impl<'a> MultibandDynamicsView<'a> {
    pub fn view(&self, size: Size) -> Element<'a, Message> {
        let knob_color = if self.effect.bypass {
            th::text_muted()
        } else {
            self.accent
        };

        // Knobs wrap to as many rows as fit the card's width.
        let per_row = (((size.width - PAD * 2.0) / KNOB_COL_W).floor() as usize).max(1);
        let row_count = self.effect.descriptors.len().div_ceil(per_row);
        let knobs_h = row_count as f32 * KNOB_ROW_H + (row_count.saturating_sub(1)) as f32 * 6.0;
        // The scope takes whatever height the rack leaves above the knobs,
        // like the other DSPs' plots.
        let scope_h = (size.height - PAD * 2.0 - 8.0 - knobs_h)
            .clamp(SCOPE_MIN_H, SCOPE_MAX_H)
            .max(0.0);
        let scope = container(
            canvas(BandScope {
                track_id: self.track_id,
                effect_id: self.effect.id,
                params: &self.effect.params,
                history: &self.panel.bands,
                now: self.now,
                span: SCOPE_SPAN,
                bypass: self.effect.bypass,
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fixed(scope_h));

        let mut knob_rows = column![].spacing(6);
        for (chunk_index, chunk) in self.effect.descriptors.chunks(per_row).enumerate() {
            let offset = chunk_index * per_row;
            let mut knob_row = row![].spacing(6);
            for (i, descriptor) in chunk.iter().enumerate() {
                let index = offset + i;
                let value = self
                    .effect
                    .params
                    .get(index)
                    .copied()
                    .unwrap_or(descriptor.default);
                let knob = EffectKnobWidget::new(
                    self.track_id,
                    self.effect.id,
                    index,
                    value,
                    descriptor.min,
                    descriptor.max,
                    descriptor.default,
                    knob_color,
                );
                knob_row = knob_row.push(param_column(
                    knob,
                    descriptor.name.to_string(),
                    crate::widgets::effect_knob::format_value(value, descriptor.unit),
                ));
            }
            knob_rows = knob_rows.push(knob_row);
        }

        container(
            column![scope, knob_rows]
                .spacing(8)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .padding(PAD)
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .into()
    }
}

/// Keep the history type nameable for the view's scope borrow.
pub type BandHistory = ReadingHistory<MultibandReading>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_grow_with_the_knob_rows() {
        assert!(COMPACT_W < MEDIUM_W && MEDIUM_W < FULL_W && FULL_W < MAX_W);
        assert!(SCOPE_MIN_H < SCOPE_MAX_H);
        assert!(SCOPE_SPAN <= crate::domains::multiband_dynamics::MULTIBAND_HISTORY);
    }
}
