//! The native Plucked String card: the envelope and loop-filter scopes and
//! the pluckable strings on top, the (small) knobs in two rows below.

use iced::widget::{canvas, column, container, row};
use iced::{Alignment, Color, Element, Length, Size};
use vibez_core::id::TrackId;

use crate::domains::device_params::DeviceParams;
use crate::domains::param_entry::ValueEntry;
use crate::message::Message;
use crate::theme as th;
use crate::widgets::device_chrome::CardWidths;
use crate::widgets::device_controls::{heading, plot_band, rule, vertical_rule, Param};
use crate::widgets::device_layout::{pack, LINE_H};
use crate::widgets::plucked_display::{EnvelopePlot, LoopFilterPlot, PluckedStrings};

pub const PAD: f32 = 10.0;
/// The knobs stay small so the scopes and strings keep the card readable.
const KNOB: f32 = 30.0;
const COL_W: f32 = 58.0;
const GAP: f32 = 6.0;
const SCOPE_H: f32 = 84.0;
const STRINGS_H: f32 = 56.0;
/// Share of the card's spare height given to the scopes.
const SCOPE_SHARE: f32 = 0.7;
/// One row of knob columns with their names and values.
const ROW_H: f32 = 76.0;
const LINE_ITEM_W: f32 = 124.0;
const LINE_GAP: f32 = 12.0;

/// Five knobs per row, two rows: the card's one-line width.
pub const COMPACT_W: f32 = PAD * 2.0 + 5.0 * COL_W + 4.0 * GAP;
pub const MAX_W: f32 = COMPACT_W + 200.0;

pub const WIDTHS: CardWidths = CardWidths {
    compact: COMPACT_W,
    medium: COMPACT_W,
    expanded: COMPACT_W,
    max: MAX_W,
};

pub struct PluckedView<'a> {
    pub track_id: TrackId,
    pub device: DeviceParams<'a>,
    pub accent: Color,
    pub entry: Option<&'a ValueEntry>,
    pub sample_rate: u32,
}

impl<'a> PluckedView<'a> {
    fn knob_row(&self, indices: &[usize], accent: Color) -> Element<'a, Message> {
        let mut knobs = row![].spacing(GAP).align_y(Alignment::Center);
        for &index in indices {
            let param = Param::new(self.track_id, self.device, index, accent, self.entry);
            knobs = knobs.push(container(param.column(KNOB)).width(COL_W));
        }
        knobs.into()
    }

    /// The knobs on lines of labelled values, for cards too short for
    /// columns; returns the element and its height.
    fn line_rows(&self, accent: Color, width: f32) -> (Element<'a, Message>, f32) {
        let count = self.device.descriptors.len();
        let widths = vec![LINE_ITEM_W; count];
        let lines = pack(&widths, (width - PAD * 2.0).max(LINE_ITEM_W), LINE_GAP);
        let height = lines.len() as f32 * LINE_H + lines.len().saturating_sub(1) as f32 * 4.0 + 8.0;
        let mut body = column![].spacing(4).align_x(Alignment::Center);
        for range in lines {
            let mut line = row![].spacing(LINE_GAP).align_y(Alignment::Center);
            for index in range {
                let param = Param::new(self.track_id, self.device, index, accent, self.entry);
                line = line.push(param.line_item());
            }
            body = body.push(line);
        }
        let element = container(body)
            .padding([4.0, PAD])
            .width(Length::Fill)
            .height(height)
            .center_x(Length::Fill)
            .into();
        (element, height)
    }

    pub fn view(self, size: Size) -> Element<'a, Message> {
        let accent = if self.device.bypass {
            th::text_muted()
        } else {
            self.accent
        };
        let indices: Vec<usize> = (0..self.device.descriptors.len()).collect();
        let split = indices.len().div_ceil(2);
        // Knob columns when the card is tall enough to keep the strings
        // beside them, otherwise labelled values on lines.
        let (knobs, knobs_h) = if size.height >= 2.0 * ROW_H + 1.0 + STRINGS_H {
            let rows: Element<'a, Message> = column![
                container(self.knob_row(&indices[..split], accent))
                    .width(Length::Fill)
                    .height(ROW_H)
                    .center_x(Length::Fill)
                    .center_y(ROW_H),
                container(self.knob_row(&indices[split..], accent))
                    .width(Length::Fill)
                    .height(ROW_H)
                    .center_x(Length::Fill)
                    .center_y(ROW_H),
            ]
            .into();
            (rows, 2.0 * ROW_H)
        } else {
            self.line_rows(accent, size.width)
        };
        let rest = size.height - knobs_h - 1.0;
        let show_scopes = rest >= SCOPE_H + 1.0 + STRINGS_H;
        // The scopes take the larger share of whatever the card has beyond
        // its minimum, so they grow with the card instead of staying thin.
        let spare = (rest - SCOPE_H - STRINGS_H - 2.0).max(0.0);
        let scope_h = SCOPE_H + spare * SCOPE_SHARE;
        let strings_h = if show_scopes {
            rest - scope_h - 1.0
        } else {
            rest
        };
        let show_strings = strings_h >= STRINGS_H;
        let mut body = column![];
        if show_scopes {
            body = body
                .push(plot_band(
                    row![
                        column![
                            heading("Envelope"),
                            canvas(EnvelopePlot {
                                params: self.device.params,
                                accent,
                            })
                            .width(Length::Fill)
                            .height(Length::Fill),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        vertical_rule(),
                        column![
                            heading("Loop Filter"),
                            canvas(LoopFilterPlot {
                                params: self.device.params,
                                sample_rate: self.sample_rate as f32,
                                accent,
                            })
                            .width(Length::Fill)
                            .height(Length::Fill),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                    ]
                    .spacing(8),
                    scope_h,
                    PAD,
                ))
                .push(rule());
        }
        if show_strings {
            body = body
                .push(plot_band(
                    column![
                        heading("Strings"),
                        canvas(PluckedStrings {
                            track_id: self.track_id,
                            accent,
                        })
                        .width(Length::Fill)
                        .height(Length::Fill),
                    ]
                    .spacing(2),
                    strings_h,
                    PAD,
                ))
                .push(rule());
        }
        body.push(knobs)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
