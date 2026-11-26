//! A group's Arrange lane: its tracks' Clips in miniature, one thin strip
//! per track, in their colours and at their places, as Ableton shows them.

use iced::widget::canvas;
use iced::{mouse, Color, Point, Rectangle, Renderer, Size, Theme};

use vibez_core::id::TrackId;

use crate::domains::view::ViewMsg;
use crate::message::Message;
use crate::theme as th;
use crate::timeline_geometry::TimelineGeometry;

/// Room left above and below the strips.
const MARGIN: f32 = 3.0;
/// A strip never grows past this, so a group of one track still reads as a
/// summary.
const MAX_STRIP: f32 = 12.0;

/// One member track's Clip: where it sits and its colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MiniClip {
    pub start_beats: f64,
    pub length_beats: f64,
    pub color: Color,
}

pub struct GroupLaneWidget {
    pub group: TrackId,
    /// The Clips of every track in the group, top track first.
    pub rows: Vec<Vec<MiniClip>>,
    pub geometry: TimelineGeometry,
    pub playhead_beats: f64,
    pub selected: bool,
    pub meter: vibez_core::meter::MeterMap,
    pub wheel_zoom: crate::shortcuts::WheelZoom,
}

/// Top and height of each of `rows` strips in a lane `height` tall.
fn strips(rows: usize, height: f32) -> impl Iterator<Item = (f32, f32)> {
    let room = (height - 2.0 * MARGIN).max(1.0);
    let step = (room / rows.max(1) as f32).min(MAX_STRIP);
    // A hairline separates strips tall enough to spare it.
    let gap = if step > 3.0 { 1.0 } else { 0.0 };
    (0..rows).map(move |row| (MARGIN + row as f32 * step, (step - gap).max(0.5)))
}

#[derive(Debug, Default)]
pub struct GroupLaneState {
    shift_held: bool,
}

impl canvas::Program<Message> for GroupLaneWidget {
    type State = GroupLaneState;

    fn update(
        &self,
        state: &mut GroupLaneState,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        use canvas::event::Status;
        match event {
            canvas::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.shift_held = modifiers.shift();
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                return (Status::Captured, Some(Message::select_track(self.group)));
            }
            // Pans and zooms as the track lanes do; a plain vertical wheel
            // goes on to scroll the track list.
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(position) = cursor.position_in(bounds) else {
                    return (Status::Ignored, None);
                };
                let (dx, dy) = crate::timeline_geometry::wheel_delta_pixels(delta);
                if dx.abs() > dy.abs() {
                    return (
                        Status::Captured,
                        Some(Message::View(ViewMsg::ScrollArrangement(
                            -self.geometry.beats_for_width(dx),
                        ))),
                    );
                }
                let zoom_bound = self.wheel_zoom.matches(dy > 0.0, cfg!(target_os = "macos"));
                if dy.abs() > 0.0 && (state.shift_held || zoom_bound) {
                    return (
                        Status::Captured,
                        Some(Message::View(ViewMsg::ZoomAround {
                            factor: crate::timeline_geometry::zoom_factor_from_pixels(dy),
                            anchor_x: position.x,
                        })),
                    );
                }
            }
            _ => {}
        }
        (Status::Ignored, None)
    }

    fn draw(
        &self,
        _state: &GroupLaneState,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let background = if self.selected {
            th::blend(th::track_bg(), th::text(), 0.06)
        } else {
            th::track_bg()
        };
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), background);

        // Bar lines, left out once they would crowd.
        let first = self.meter.position_at(self.geometry.x_to_beat(0.0));
        if self.geometry.pixels_per_beat() * first.signature.bar_length() as f32 >= 8.0 {
            let mut bar = first.bar;
            loop {
                let x = self.geometry.beat_to_x(self.meter.bar_start(bar));
                if x > bounds.width {
                    break;
                }
                if x >= 0.0 {
                    frame.fill_rectangle(
                        Point::new(x, 0.0),
                        Size::new(1.0, bounds.height),
                        th::groove(),
                    );
                }
                bar += 1;
            }
        }

        for (clips, (top, height)) in self.rows.iter().zip(strips(self.rows.len(), bounds.height)) {
            for clip in clips {
                let left = self.geometry.beat_to_x(clip.start_beats);
                let right = self
                    .geometry
                    .beat_to_x(clip.start_beats + clip.length_beats);
                if right < 0.0 || left > bounds.width {
                    continue;
                }
                let left = left.max(0.0);
                // A pixel of lane keeps neighbouring Clips apart.
                let width = (right.min(bounds.width) - left - 1.0).max(1.0);
                frame.fill_rectangle(Point::new(left, top), Size::new(width, height), clip.color);
            }
        }
        frame.fill_rectangle(
            Point::new(0.0, bounds.height - 1.0),
            Size::new(bounds.width, 1.0),
            th::groove(),
        );

        let playhead = self.geometry.beat_to_x(self.playhead_beats);
        let playhead = (0.0..=bounds.width).contains(&playhead).then_some(playhead);
        std::iter::once(frame.into_geometry())
            .chain(super::playhead::playhead_geometry(
                renderer, bounds, playhead,
            ))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_share_the_lane_without_leaving_it() {
        let one: Vec<_> = strips(1, 80.0).collect();
        assert_eq!(one, [(MARGIN, MAX_STRIP - 1.0)]);

        let many: Vec<_> = strips(40, 60.0).collect();
        assert_eq!(many.len(), 40);
        let (top, height) = many[39];
        assert!(top + height <= 60.0 - MARGIN + 0.001);
    }
}
