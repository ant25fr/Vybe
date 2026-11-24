//! The Multiband Dynamics scope: the input level over the last seconds
//! with each band's gain reduction hanging from the top, and each band's
//! threshold as a dashed line in its EQ band colour — drag a line up or
//! down to set that band's threshold.

use std::time::{Duration, Instant};

use iced::alignment::Horizontal;
use iced::mouse;
use iced::widget::canvas;
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use vibez_core::id::{EffectId, TrackId};
use vibez_dsp::multiband_dynamics::param;

use crate::domains::device_levels::ReadingHistory;
use crate::domains::multiband_dynamics::MultibandReading;
use crate::message::Message;
use crate::state::UndoGestureId;
use crate::theme as th;
use crate::widgets::device_controls::LABEL_SIZE;

/// The level axis runs from 0 dBFS at the top to here at the bottom, so the
/// lowest threshold fits too.
const FLOOR_DB: f32 = -60.0;
/// The reduction traces' scale, in dB taken off.
const REDUCTION_RANGE_DB: f32 = 24.0;
const LABEL_W: f32 = 26.0;
const LABEL_H: f32 = 14.0;
/// How close to a threshold line a press must land to grab it.
const GRAB_DISTANCE: f32 = 7.0;

/// Band trace colours, shared with the EQ bands: low, mid, high.
pub fn band_color(band: usize) -> Color {
    match band {
        0 => th::eq_lf(),
        1 => th::eq_hmf(),
        _ => th::eq_hf(),
    }
}

const BAND_LABELS: [&str; 3] = ["LOW", "MID", "HIGH"];
/// The threshold parameter index of each band.
const BAND_THRESHOLD_PARAM: [usize; 3] = [
    param::LOW_THRESHOLD,
    param::MID_THRESHOLD,
    param::HIGH_THRESHOLD,
];

fn grid() -> Color {
    th::with_alpha(th::border(), 0.35)
}

fn label(frame: &mut canvas::Frame, text: String, at: Point, align: Horizontal, color: Color) {
    frame.fill_text(canvas::Text {
        content: text,
        position: at,
        color,
        size: LABEL_SIZE.into(),
        horizontal_alignment: align,
        vertical_alignment: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}

/// The input level and each band's reduction over the last `span`, newest
/// at the right, with one draggable threshold line per band.
pub struct BandScope<'a> {
    pub track_id: TrackId,
    pub effect_id: EffectId,
    pub params: &'a [f32],
    pub history: &'a ReadingHistory<MultibandReading>,
    pub now: Instant,
    pub span: Duration,
    pub bypass: bool,
}

/// Which band's threshold line is being dragged, and its undo gesture.
#[derive(Default)]
pub struct BandScopeState {
    gesture: Option<(UndoGestureId, usize)>,
}

impl BandScope<'_> {
    fn plot(bounds: Size) -> Rectangle {
        let labels = bounds.height >= 50.0;
        let left = if labels { LABEL_W } else { 4.0 };
        let bottom = if labels { LABEL_H } else { 4.0 };
        Rectangle::new(
            Point::new(left, 4.0),
            Size::new(
                (bounds.width - left - 6.0).max(1.0),
                (bounds.height - bottom - 6.0).max(1.0),
            ),
        )
    }

    /// 0 dBFS at the top, `FLOOR_DB` at the bottom.
    fn y(plot: Rectangle, db: f32) -> f32 {
        plot.y + plot.height * (db.clamp(FLOOR_DB, 0.0) / FLOOR_DB)
    }

    fn threshold(&self, band: usize) -> f32 {
        self.params
            .get(BAND_THRESHOLD_PARAM[band])
            .copied()
            .unwrap_or(-24.0)
    }

    /// Inverse of [`Self::y`]: the dB level at a y position in the plot.
    fn db_at(plot: Rectangle, y: f32) -> f32 {
        FLOOR_DB * (y - plot.y) / plot.height
    }

    /// The band whose threshold line is nearest to `at`, when grabbable.
    fn band_at(&self, plot: Rectangle, at: Point) -> Option<usize> {
        (0..3)
            .map(|band| (band, (Self::y(plot, self.threshold(band)) - at.y).abs()))
            .filter(|(_, distance)| *distance <= GRAB_DISTANCE)
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(band, _)| band)
    }
}

impl canvas::Program<Message> for BandScope<'_> {
    type State = BandScopeState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill(
            &canvas::Path::rounded_rectangle(Point::ORIGIN, bounds.size(), 3.0.into()),
            th::bg_dark(),
        );
        let labels = bounds.height >= 50.0;
        let plot = Self::plot(bounds.size());
        let y = |db: f32| Self::y(plot, db);
        let y_reduction = |db: f32| {
            plot.y + plot.height * (db.clamp(0.0, REDUCTION_RANGE_DB) / REDUCTION_RANGE_DB)
        };
        let span = self.span.as_secs_f32().max(0.1);
        let x = |age: f32| plot.x + plot.width * (1.0 - age / span);
        for db in [0.0f32, -12.0, -24.0, -36.0, -48.0, -60.0] {
            frame.stroke(
                &canvas::Path::line(
                    Point::new(plot.x, y(db)),
                    Point::new(plot.x + plot.width, y(db)),
                ),
                canvas::Stroke::default().with_color(grid()).with_width(1.0),
            );
            if labels {
                label(
                    &mut frame,
                    format!("{db:.0}"),
                    Point::new(LABEL_W - 4.0, y(db)),
                    Horizontal::Right,
                    th::text_dim(),
                );
            }
        }
        let seconds = span.round() as u32;
        let step = if seconds > 6 { 2 } else { 1 };
        for second in (0..=seconds).step_by(step) {
            let at = x(second as f32);
            frame.stroke(
                &canvas::Path::line(Point::new(at, plot.y), Point::new(at, plot.y + plot.height)),
                canvas::Stroke::default().with_color(grid()).with_width(1.0),
            );
            if labels {
                let align = if second == 0 {
                    Horizontal::Right
                } else if second == seconds {
                    Horizontal::Left
                } else {
                    Horizontal::Center
                };
                let text = if second == 0 {
                    "0s".into()
                } else {
                    format!("-{second}s")
                };
                label(
                    &mut frame,
                    text,
                    Point::new(at, plot.y + plot.height + LABEL_H / 2.0 + 1.0),
                    align,
                    th::text_dim(),
                );
            }
        }
        let dim = self.bypass;
        let age = |at: &Instant| self.now.saturating_duration_since(*at).as_secs_f32();
        let readings: Vec<(f32, MultibandReading)> = self
            .history
            .iter()
            .map(|(at, reading)| (age(at), *reading))
            .filter(|(age, _)| *age <= span)
            .collect();
        // The input as one filled shape through its readings.
        if readings.len() > 1 {
            let bottom = plot.y + plot.height;
            let input = canvas::Path::new(|builder| {
                builder.move_to(Point::new(x(readings[0].0), bottom));
                for (age, reading) in &readings {
                    builder.line_to(Point::new(x(*age), y(reading.input_db)));
                }
                builder.line_to(Point::new(x(readings[readings.len() - 1].0), bottom));
                builder.close();
            });
            frame.fill(&input, th::with_alpha(th::text_dim(), 0.45));
        }
        // One reduction trace per band, hanging down from 0 dB at the top,
        // and one dashed threshold line per band the user can drag.
        for band in 0..3 {
            let color = if dim {
                th::text_muted()
            } else {
                band_color(band)
            };
            if readings.len() > 1 {
                let trace = canvas::Path::new(|builder| {
                    for (index, (age, reading)) in readings.iter().enumerate() {
                        let point =
                            Point::new(x(*age), y_reduction(reading.band_reduction_db[band]));
                        if index == 0 {
                            builder.move_to(point);
                        } else {
                            builder.line_to(point);
                        }
                    }
                });
                frame.stroke(
                    &trace,
                    canvas::Stroke::default().with_color(color).with_width(1.5),
                );
            }
            let threshold_y = y(self.threshold(band));
            frame.stroke(
                &canvas::Path::line(
                    Point::new(plot.x, threshold_y),
                    Point::new(plot.x + plot.width, threshold_y),
                ),
                canvas::Stroke {
                    line_dash: canvas::LineDash {
                        segments: &[4.0, 3.0],
                        offset: 0,
                    },
                    ..canvas::Stroke::default()
                        .with_color(th::with_alpha(color, 0.75))
                        .with_width(1.0)
                },
            );
            if labels {
                let engaged = state.gesture.is_some_and(|(_, held)| held == band)
                    || cursor
                        .position_in(bounds)
                        .is_some_and(|at| self.band_at(plot, at) == Some(band));
                label(
                    &mut frame,
                    format!("{}: {:.0}", BAND_LABELS[band], self.threshold(band)),
                    Point::new(plot.x + plot.width - 4.0, threshold_y),
                    Horizontal::Right,
                    if engaged {
                        color
                    } else {
                        th::with_alpha(color, 0.8)
                    },
                );
            }
        }
        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        if self.bypass {
            return (canvas::event::Status::Ignored, None);
        }
        let plot = Self::plot(bounds.size());
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(at) = cursor.position_in(bounds) {
                    if let Some(band) = self.band_at(plot, at) {
                        state.gesture = Some((UndoGestureId::new(), band));
                        return (canvas::event::Status::Captured, None);
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.gesture.take().is_some() =>
            {
                return (canvas::event::Status::Captured, None);
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some((gesture, band)), Some(at)) = (state.gesture, cursor.position()) {
                    let db = Self::db_at(plot, at.y - bounds.y).clamp(-60.0, 0.0);
                    let value = (db * 10.0).round() / 10.0;
                    return (
                        canvas::event::Status::Captured,
                        Some(
                            Message::set_effect_param(
                                self.track_id,
                                self.effect_id,
                                BAND_THRESHOLD_PARAM[band],
                                value,
                            )
                            .in_undo_gesture(gesture),
                        ),
                    );
                }
            }
            _ => {}
        }
        (canvas::event::Status::Ignored, None)
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.bypass {
            return mouse::Interaction::default();
        }
        let plot = Self::plot(bounds.size());
        if state.gesture.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor
            .position_in(bounds)
            .is_some_and(|at| self.band_at(plot, at).is_some())
        {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_colors_match_the_eq_band_palette() {
        assert_eq!(band_color(0), th::eq_lf());
        assert_eq!(band_color(1), th::eq_hmf());
        assert_eq!(band_color(2), th::eq_hf());
        assert_eq!(BAND_LABELS.len(), 3);
        assert_eq!(BAND_THRESHOLD_PARAM.len(), 3);
    }

    #[test]
    fn db_axis_runs_from_zero_at_the_top_to_the_floor_at_the_bottom() {
        let plot = Rectangle::new(Point::new(0.0, 0.0), Size::new(100.0, 60.0));
        assert!((BandScope::y(plot, 0.0) - 0.0).abs() < 1e-6);
        assert!((BandScope::y(plot, FLOOR_DB) - 60.0).abs() < 1e-6);
        assert!((BandScope::db_at(plot, 30.0) - -30.0).abs() < 1e-6);
        assert!((BandScope::db_at(plot, BandScope::y(plot, -18.0)) - -18.0).abs() < 1e-4);
    }
}
