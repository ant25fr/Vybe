//! The native Stereo Widener card: a fan scope that opens and closes with
//! Width (a sector of the upper semicircle, like a goniometer), above the
//! two knobs.

use iced::widget::{canvas, column, container, row};
use iced::{Alignment, Color, Element, Length, Point, Rectangle, Size};
use vibez_core::id::TrackId;
use vibez_dsp::stereo_widener::param as sw_param;

use crate::domains::device_params::DeviceParams;
use crate::domains::param_entry::ValueEntry;
use crate::message::Message;
use crate::spectrum::{SpectrumState, DISPLAY_BINS, FLOOR_DB};
use crate::theme as th;
use crate::widgets::device_chrome::CardWidths;
use crate::widgets::device_controls::{heading, plot_band, rule, Param};
use crate::widgets::device_layout::{stack, KNOB_ROW_H};

pub const PAD: f32 = 10.0;
const PLOT_MIN_H: f32 = 72.0;
const KNOB: f32 = 40.0;
const LINE_ITEM_W: f32 = 124.0;
const LINE_GAP: f32 = 12.0;
const COL_W: f32 = 76.0;
const GAP: f32 = 8.0;

pub const COMPACT_W: f32 = PAD * 2.0 + 2.0 * COL_W + GAP;
pub const MAX_W: f32 = 460.0;

pub const WIDTHS: CardWidths = CardWidths {
    compact: COMPACT_W.max(240.0),
    medium: COMPACT_W.max(240.0),
    expanded: 300.0,
    max: MAX_W,
};

/// The fan scope: a filled sector of the upper semicircle whose half-angle
/// follows Width (0 = a single centre line, 1 = a quarter fan, 2 = the whole
/// semicircle).
pub struct FanScope {
    pub width: f32,
    pub accent: Color,
    /// Live side and mid levels per log-spaced frequency bin (dB), lowest
    /// frequency first; empty when nothing is listened to.
    pub side: Vec<f32>,
    pub mid: Vec<f32>,
}

/// The narrowest a live band is drawn, in radians.
const MIN_HALF_ANGLE: f32 = 0.05;

/// Half-angle (radians) a band opens to, from its side and mid levels: 0 is
/// a mono band, a quarter turn a band that is all side.
fn band_half_angle(side_db: f32, mid_db: f32) -> f32 {
    let ratio = 10f32.powf((side_db - mid_db) / 20.0);
    ratio.atan().min(std::f32::consts::FRAC_PI_2)
}

impl canvas::Program<Message> for FanScope {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let size = bounds.size();
        frame.fill_rectangle(Point::ORIGIN, size, th::bg_dark());
        let cx = size.width / 2.0;
        let cy = size.height - 10.0;
        let radius = ((size.width / 2.0).min(size.height) - 14.0).max(20.0);
        let at = |angle_deg: f32| {
            let a = angle_deg.to_radians();
            Point::new(cx + radius * a.cos(), cy - radius * a.sin())
        };
        // The full upper semicircle and its ticks.
        let mut arc = canvas::Path::new(|builder| {
            builder.move_to(at(180.0));
            for deg in (0..=180).step_by(6) {
                builder.line_to(at(deg as f32));
            }
        });
        frame.stroke(
            &arc,
            canvas::Stroke::default()
                .with_color(th::border())
                .with_width(1.0),
        );
        let _ = &mut arc;
        for deg in (0..=180).step_by(15) {
            let inner = radius - if deg % 45 == 0 { 7.0 } else { 4.0 };
            let a = (deg as f32).to_radians();
            frame.stroke(
                &canvas::Path::line(
                    Point::new(cx + inner * a.cos(), cy - inner * a.sin()),
                    at(deg as f32),
                ),
                canvas::Stroke::default()
                    .with_color(th::with_alpha(th::text_muted(), 0.6))
                    .with_width(1.0),
            );
        }
        // The fan: centred on straight up (90 deg), half-angle = width * 45 deg.
        let half = (self.width.clamp(0.0, 2.0) * 45.0).to_radians();
        let from = std::f32::consts::FRAC_PI_2 - half;
        let to = std::f32::consts::FRAC_PI_2 + half;
        let point = |a: f32| Point::new(cx + radius * a.cos(), cy - radius * a.sin());
        let mut fan = canvas::Path::new(|builder| {
            builder.move_to(Point::new(cx, cy));
            builder.line_to(point(from));
            let steps = ((to - from).abs() / 0.05).ceil().max(1.0) as i32;
            for i in 1..=steps {
                builder.line_to(point(from + (to - from) * i as f32 / steps as f32));
            }
            builder.close();
        });
        frame.fill(&fan, th::with_alpha(self.accent, 0.30));
        frame.stroke(
            &fan,
            canvas::Stroke::default()
                .with_color(self.accent)
                .with_width(1.5),
        );
        let _ = &mut fan;
        // The live spectrum: frequency runs from the centre out to the rim,
        // and each band opens to the angle its side/mid balance gives, so
        // Width opens the fan and Bass Mono closes it at the centre.
        if self.side.len() == DISPLAY_BINS && self.mid.len() == DISPLAY_BINS {
            let bins = DISPLAY_BINS;
            for i in 0..bins {
                let (side, mid) = (self.side[i], self.mid[i]);
                let level = side.max(mid);
                if level <= FLOOR_DB + 3.0 {
                    continue;
                }
                // A mono band keeps a sliver, so it stays visible.
                let half = band_half_angle(side, mid).max(MIN_HALF_ANGLE);
                let strength = ((level - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
                let (r0, r1) = (
                    radius * i as f32 / bins as f32,
                    radius * (i + 1) as f32 / bins as f32,
                );
                let around = |r: f32, a: f32| {
                    Point::new(
                        cx + r * (std::f32::consts::FRAC_PI_2 + a).cos(),
                        cy - r * (std::f32::consts::FRAC_PI_2 + a).sin(),
                    )
                };
                let steps = ((2.0 * half) / 0.08).ceil().max(1.0) as i32;
                let slice = canvas::Path::new(|builder| {
                    builder.move_to(around(r0, -half));
                    for k in 0..=steps {
                        builder.line_to(around(r1, -half + 2.0 * half * k as f32 / steps as f32));
                    }
                    builder.line_to(around(r0, half));
                    for k in (0..=steps).rev() {
                        builder.line_to(around(r0, -half + 2.0 * half * k as f32 / steps as f32));
                    }
                    builder.close();
                });
                frame.fill(&slice, th::with_alpha(self.accent, 0.15 + 0.7 * strength));
            }
        }
        // The centre (mono) line.
        frame.stroke(
            &canvas::Path::line(Point::new(cx, cy), Point::new(cx, cy - radius)),
            canvas::Stroke::default()
                .with_color(th::with_alpha(th::text_dim(), 0.8))
                .with_width(1.0),
        );
        frame.fill(
            &canvas::Path::circle(Point::new(cx, cy), 2.5),
            th::text_dim(),
        );
        vec![frame.into_geometry()]
    }
}

pub struct StereoWidenerView<'a> {
    pub track_id: TrackId,
    pub device: DeviceParams<'a>,
    pub accent: Color,
    pub entry: Option<&'a ValueEntry>,
    /// The output's (side, mid) spectra, while this card is listened to.
    pub spectrum: Option<(&'a SpectrumState, &'a SpectrumState)>,
}

impl<'a> StereoWidenerView<'a> {
    fn param(&self, index: usize, accent: Color) -> Param<'a> {
        Param::new(self.track_id, self.device, index, accent, self.entry)
    }

    pub fn view(self, size: Size) -> Element<'a, Message> {
        let accent = if self.device.bypass {
            th::text_muted()
        } else {
            self.accent
        };
        let width =
            self.device.params.get(sw_param::WIDTH).copied().unwrap_or(
                vibez_dsp::stereo_widener::STEREO_WIDENER_PARAMS[sw_param::WIDTH].default,
            );
        let stack = stack(size.height.max(0.0), None, PLOT_MIN_H);
        let count = self.device.descriptors.len();
        let knobs: Element<'a, Message> = if stack.knob_row < KNOB_ROW_H {
            let mut line = row![].spacing(LINE_GAP).align_y(Alignment::Center);
            let mut room = size.width - PAD * 2.0;
            for index in 0..count {
                if LINE_ITEM_W > room {
                    break;
                }
                room -= LINE_ITEM_W + LINE_GAP;
                line = line.push(self.param(index, accent).line_item());
            }
            container(line)
                .padding([0.0, PAD])
                .width(Length::Fill)
                .height(stack.knob_row)
                .center_x(Length::Fill)
                .center_y(stack.knob_row)
                .into()
        } else {
            let mut knobs = row![].spacing(GAP).align_y(Alignment::Center);
            for index in 0..count {
                knobs = knobs.push(container(self.param(index, accent).column(KNOB)).width(COL_W));
            }
            container(knobs)
                .padding([4.0, PAD])
                .width(Length::Fill)
                .height(stack.knob_row)
                .center_x(Length::Fill)
                .center_y(stack.knob_row)
                .into()
        };
        let mut body = column![];
        if stack.plots > 0.0 {
            let (side, mid) = self
                .spectrum
                .filter(|(side, mid)| side.active || mid.active)
                .map_or((Vec::new(), Vec::new()), |(side, mid)| {
                    (side.bins.clone(), mid.bins.clone())
                });
            let plots: Element<'a, Message> = column![
                heading("Width"),
                canvas(FanScope {
                    width,
                    accent,
                    side,
                    mid,
                })
                .width(Length::Fill)
                .height(Length::Fill),
            ]
            .spacing(2)
            .into();
            body = body.push(plot_band(plots, stack.plots, PAD)).push(rule());
        }
        body.push(knobs)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::band_half_angle;

    #[test]
    fn bands_open_with_their_side_level() {
        // No side at all is a mono band; equal side and mid is a quarter of
        // the fan's full opening; side alone opens it fully.
        assert!(band_half_angle(-60.0, 0.0) < 0.01);
        assert!((band_half_angle(-6.0, -6.0) - std::f32::consts::FRAC_PI_4).abs() < 1e-4);
        assert!(band_half_angle(0.0, -60.0) > 1.5);
        // More width, more opening.
        assert!(band_half_angle(-6.0, -12.0) > band_half_angle(-12.0, -12.0));
    }
}
