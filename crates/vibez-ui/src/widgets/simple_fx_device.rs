//! Body of the simple built-in effects (Chorus, Flanger, Pitch Shifter,
//! Ring Modulator, Frequency Shifter): the plots of what the effect does
//! above one row of knobs, like the Phaser and Auto-Pan. Short cards drop
//! the plots, then put the controls on a line.

use iced::widget::{canvas, column, container, row};
use iced::{Alignment, Color, Element, Length, Size};
use vibez_core::id::TrackId;

use crate::domains::device_params::DeviceParams;
use crate::domains::param_entry::ValueEntry;
use crate::message::Message;
use crate::theme as th;
use crate::widgets::device_chrome::CardWidths;
use crate::widgets::device_controls::{heading, plot_band, rule, vertical_rule, Param};
use crate::widgets::device_layout::{stack, Stack, KNOB_ROW_H};
use crate::widgets::simple_fx_display::{
    CombPlot, DelaySweepPlot, Delayed, PitchPlot, Shifting, SidebandPlot,
};

pub const PAD: f32 = 10.0;
const KNOB: f32 = 40.0;
const KNOB_COL_W: f32 = 68.0;
const PLOT_MIN_H: f32 = 72.0;
const LINE_ITEM_W: f32 = 124.0;
const LINE_GAP: f32 = 12.0;

/// Which simple effect a card shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SimpleKind {
    Chorus,
    Flanger,
    PitchShifter,
    RingModulator,
    FrequencyShifter,
}

impl SimpleKind {
    /// The widths the card lays itself out for: the knobs in one row, wide
    /// enough that the plots have room.
    pub const fn widths(count: usize) -> CardWidths {
        let full = PAD * 2.0 + count as f32 * KNOB_COL_W;
        let compact = if full > 380.0 { full } else { 380.0 };
        CardWidths {
            compact,
            medium: compact,
            expanded: compact + 100.0,
            max: compact + 460.0,
        }
    }
}

pub fn arrange(size: Size) -> Stack {
    stack(size.height.max(0.0), None, PLOT_MIN_H)
}

pub struct SimpleFxView<'a> {
    pub kind: SimpleKind,
    pub track_id: TrackId,
    pub device: DeviceParams<'a>,
    pub accent: Color,
    pub entry: Option<&'a ValueEntry>,
}

impl<'a> SimpleFxView<'a> {
    fn accent(&self) -> Color {
        if self.device.bypass {
            th::text_muted()
        } else {
            self.accent
        }
    }

    fn param(&self, index: usize) -> Param<'a> {
        Param::new(self.track_id, self.device, index, self.accent(), self.entry)
    }

    pub fn view(self, size: Size) -> Element<'a, Message> {
        let stack = arrange(size);
        let mut body = column![];
        if stack.plots > 0.0 {
            body = body.push(self.plots(stack.plots)).push(rule());
        }
        body = body.push(self.knob_row(stack.knob_row, size.width));
        body.width(Length::Fill).height(Length::Fill).into()
    }

    fn scope<P>(&self, title: &'static str, program: P) -> iced::widget::Column<'a, Message>
    where
        P: canvas::Program<Message> + 'a,
    {
        column![
            heading(title),
            canvas(program).width(Length::Fill).height(Length::Fill),
        ]
        .spacing(2)
        .width(Length::Fill)
    }

    fn plots(&self, height: f32) -> Element<'a, Message> {
        let (accent, params) = (self.accent(), self.device.params);
        let content: Element<'a, Message> = match self.kind {
            SimpleKind::Chorus | SimpleKind::Flanger => {
                let kind = if self.kind == SimpleKind::Chorus {
                    Delayed::Chorus
                } else {
                    Delayed::Flanger
                };
                row![
                    self.scope(
                        "Delay (ms)",
                        DelaySweepPlot {
                            kind,
                            params,
                            accent
                        }
                    ),
                    vertical_rule(),
                    self.scope(
                        "Gain (dB)",
                        CombPlot {
                            kind,
                            params,
                            accent
                        }
                    ),
                ]
                .spacing(8)
                .into()
            }
            SimpleKind::PitchShifter => self
                .scope("Transposition (st)", PitchPlot { params, accent })
                .into(),
            SimpleKind::RingModulator | SimpleKind::FrequencyShifter => {
                let kind = if self.kind == SimpleKind::RingModulator {
                    Shifting::Ring
                } else {
                    Shifting::Frequency
                };
                self.scope(
                    "Output of a 1 kHz tone",
                    SidebandPlot {
                        kind,
                        params,
                        accent,
                    },
                )
                .into()
            }
        };
        plot_band(content, height, PAD)
    }

    fn knob_row(&self, height: f32, width: f32) -> Element<'a, Message> {
        let count = self.device.descriptors.len();
        if height < KNOB_ROW_H {
            // Controls leave from the end when the line runs out of room.
            let mut room = width - PAD * 2.0;
            let mut line = row![].spacing(LINE_GAP).align_y(Alignment::Center);
            for index in 0..count {
                if LINE_ITEM_W > room {
                    break;
                }
                room -= LINE_ITEM_W + LINE_GAP;
                line = line.push(self.param(index).line_item());
            }
            return container(line)
                .padding([0.0, PAD])
                .height(height)
                .center_y(height)
                .into();
        }
        let mut knobs = row![].spacing(0).align_y(Alignment::Center);
        for index in 0..count {
            knobs = knobs.push(
                container(self.param(index).column(KNOB))
                    .width(Length::Fill)
                    .max_width(KNOB_COL_W * 1.6)
                    .align_x(Alignment::Center),
            );
        }
        container(knobs)
            .padding([4.0, PAD])
            .width(Length::Fill)
            .height(height)
            .center_x(Length::Fill)
            .center_y(height)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::device_layout::LINE_H;

    #[test]
    fn every_supported_size_fits_without_scrolling() {
        let widths = SimpleKind::widths(5);
        for width in [widths.compact, widths.max] {
            for height in (56..=420).step_by(2).map(|height| height as f32) {
                let stack = arrange(Size::new(width, height));
                let context = format!("{width}x{height}: {stack:?}");
                let mut used = stack.knob_row;
                if stack.plots > 0.0 {
                    used += stack.plots + 1.0;
                    assert!(stack.plots >= PLOT_MIN_H, "{context}");
                }
                assert!(stack.knob_row >= LINE_H, "{context}");
                assert!(used <= height + 0.01, "{context}");
            }
        }
    }
}
