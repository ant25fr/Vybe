//! One take lane under a track: its recordings, the parts the comp plays,
//! and the drag that chooses another part.

use std::sync::Arc;

use iced::widget::canvas;
use iced::{mouse, Color, Point, Rectangle, Renderer, Size, Theme};

use vibez_core::audio_buffer::DecodedAudio;
use vibez_core::id::TrackId;

use super::source_peaks::source_peaks;
use crate::domains::arrangement::ArrangementMsg;
use crate::message::Message;
use crate::state::GridConfig;
use crate::theme as th;
use crate::timeline_geometry::TimelineGeometry;

/// Pointer travel under which a press and release is a click.
const CLICK_PIXELS: f32 = 3.0;

/// A take's Clip as the lane draws it.
#[derive(Debug, Clone)]
pub struct TakeClip {
    pub position: u64,
    pub duration: u64,
    pub source_offset: u64,
    pub audio: Arc<DecodedAudio>,
}

pub struct TakeLaneWidget {
    pub track_id: TrackId,
    pub lane: u32,
    pub clips: Vec<TakeClip>,
    /// The stretches of this lane the comp plays, in samples.
    pub comped: Vec<(u64, u64)>,
    pub color: Color,
    pub geometry: TimelineGeometry,
    pub grid: GridConfig,
    pub samples_per_beat: f64,
}

#[derive(Debug, Default)]
pub struct TakeDrag {
    /// Where the press landed and where the pointer is now.
    drag: Option<(f32, f32)>,
}

impl TakeLaneWidget {
    fn x_of(&self, samples: u64) -> f32 {
        self.geometry
            .beat_to_x(samples as f64 / self.samples_per_beat)
    }

    fn samples_at(&self, x: f32) -> u64 {
        let beat = self
            .grid
            .snap_beat(self.geometry.x_to_beat(x), self.geometry.pixels_per_beat())
            .max(0.0);
        (beat * self.samples_per_beat).round() as u64
    }

    /// The range a gesture from `from` to `to` chooses: the dragged span, or
    /// the whole Clip under a click.
    fn chosen(&self, from: f32, to: f32) -> Option<(u64, u64)> {
        if (to - from).abs() < CLICK_PIXELS {
            return self
                .clips
                .iter()
                .find(|clip| {
                    (self.x_of(clip.position)..self.x_of(clip.position + clip.duration))
                        .contains(&from)
                })
                .map(|clip| (clip.position, clip.position + clip.duration));
        }
        let (start, end) = (self.samples_at(from.min(to)), self.samples_at(from.max(to)));
        (end > start).then_some((start, end))
    }
}

impl canvas::Program<Message> for TakeLaneWidget {
    type State = TakeDrag;

    fn update(
        &self,
        state: &mut TakeDrag,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        use canvas::event::Status;
        if self.samples_per_beat <= 0.0 {
            return (Status::Ignored, None);
        }
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(point) = cursor.position_in(bounds) {
                    state.drag = Some((point.x, point.x));
                    return (Status::Captured, None);
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some((_, at)), Some(point)) = (state.drag.as_mut(), cursor.position()) {
                    *at = point.x - bounds.x;
                    return (Status::Captured, None);
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some((from, to)) = state.drag.take() {
                    let message = self.chosen(from, to).map(|(start, end)| {
                        Message::Arrangement(ArrangementMsg::CompFromTake {
                            track_id: self.track_id,
                            lane: self.lane,
                            start,
                            end,
                        })
                    });
                    return (Status::Captured, message);
                }
            }
            _ => {}
        }
        (Status::Ignored, None)
    }

    fn draw(
        &self,
        state: &TakeDrag,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), th::bg_dark());
        let height = bounds.height;
        if self.samples_per_beat > 0.0 {
            for clip in &self.clips {
                let left = self.x_of(clip.position);
                let right = self.x_of(clip.position + clip.duration);
                let (shown_left, shown_right) = (left.max(0.0), right.min(bounds.width));
                if shown_right <= shown_left {
                    continue;
                }
                frame.fill_rectangle(
                    Point::new(shown_left, 1.0),
                    Size::new(shown_right - shown_left, height - 2.0),
                    Color {
                        a: 0.22,
                        ..self.color
                    },
                );
                let peaks = source_peaks(&clip.audio);
                let per_pixel = clip.duration as f64 / f64::from((right - left).max(1.0));
                let middle = height / 2.0;
                let mut x = shown_left.floor();
                while x < shown_right {
                    let into = (f64::from(x - left).max(0.0) * per_pixel) as u64;
                    let from = (clip.source_offset + into) as usize;
                    let to = from + per_pixel.ceil() as usize;
                    let (low, high) = peaks.peak(&clip.audio, from, to);
                    let top = middle - high.clamp(0.0, 1.0) * (middle - 2.0);
                    let bottom = middle - low.clamp(-1.0, 0.0) * (middle - 2.0);
                    frame.fill_rectangle(
                        Point::new(x, top),
                        Size::new(1.0, (bottom - top).max(1.0)),
                        self.color,
                    );
                    x += 1.0;
                }
            }
            // The parts in the comp are outlined along the bottom and top.
            for &(start, end) in &self.comped {
                let (left, right) = (self.x_of(start).max(0.0), self.x_of(end).min(bounds.width));
                if right > left {
                    for y in [0.0, height - 3.0] {
                        frame.fill_rectangle(
                            Point::new(left, y),
                            Size::new(right - left, 3.0),
                            th::accent(),
                        );
                    }
                }
            }
            if let Some((from, to)) = state.drag {
                if let Some((start, end)) = self.chosen(from, to) {
                    let (left, right) = (self.x_of(start), self.x_of(end));
                    frame.fill_rectangle(
                        Point::new(left, 0.0),
                        Size::new(right - left, height),
                        Color {
                            a: 0.25,
                            ..th::accent()
                        },
                    );
                }
            }
        }
        frame.fill_rectangle(
            Point::new(0.0, height - 1.0),
            Size::new(bounds.width, 1.0),
            th::groove(),
        );
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &TakeDrag,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}
