//! The plots of a modulator card: an LFO's cycle with a dot where it is now,
//! and an envelope whose points can be dragged.

use iced::widget::canvas;
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};
use vibez_core::id::TrackId;
use vibez_core::modulation::ModulatorSettings;

use crate::domains::modulation::{ModField, ModulationMsg};
use crate::message::Message;
use crate::state::UndoGestureId;
use crate::theme as th;
use crate::widgets::local_drag::LocalDrag;

/// Room around the curve, so the dot is never cut at a peak.
const INSET: f32 = 5.0;
const DOT_RADIUS: f32 = 4.0;
/// Below this height there is no room for a readable curve.
const MIN_HEIGHT: f32 = 16.0;

pub struct LfoPlot {
    pub settings: ModulatorSettings,
    /// Cycles elapsed, without the start phase.
    pub cycles: f64,
    pub accent: Color,
}

impl LfoPlot {
    /// Where the curve passes at `across`, 0..1 of the displayed cycle.
    fn point(&self, across: f64, width: f32, height: f32) -> Point {
        let value = self
            .settings
            .shape
            .value(across + f64::from(self.settings.phase));
        Point::new(
            INSET + across as f32 * (width - INSET * 2.0),
            height / 2.0 - value * (height / 2.0 - INSET),
        )
    }

    fn dot(&self, width: f32, height: f32) -> Point {
        self.point(self.cycles.rem_euclid(1.0), width, height)
    }
}

impl canvas::Program<Message> for LfoPlot {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (width, height) = (bounds.width, bounds.height);
        if height < MIN_HEIGHT || width <= INSET * 2.0 {
            return vec![frame.into_geometry()];
        }
        frame.fill(
            &canvas::Path::new(|path| {
                path.rounded_rectangle(Point::ORIGIN, bounds.size(), 3.0.into());
            }),
            th::bg_dark(),
        );
        let grid = th::with_alpha(th::border(), 0.35);
        frame.fill_rectangle(
            Point::new(0.0, (height / 2.0).floor()),
            iced::Size::new(width, 1.0),
            grid,
        );
        let accent = if self.settings.enabled {
            self.accent
        } else {
            th::text_muted()
        };
        let steps = width.max(2.0) as usize;
        let curve = canvas::Path::new(|path| {
            path.move_to(self.point(0.0, width, height));
            for step in 1..=steps {
                // Stops a hair short of the cycle's end, where a saw or
                // square has already jumped to the next one.
                let across = step as f64 / steps as f64;
                path.line_to(self.point(across.min(1.0 - 1e-9), width, height));
            }
        });
        frame.stroke(
            &curve,
            canvas::Stroke::default()
                .with_color(accent)
                .with_width(1.5)
                .with_line_join(canvas::LineJoin::Round),
        );
        if self.settings.enabled {
            let dot = canvas::Path::circle(self.dot(width, height), DOT_RADIUS);
            frame.fill(&dot, accent);
            frame.stroke(
                &dot,
                canvas::Stroke::default()
                    .with_color(th::text())
                    .with_width(1.0),
            );
        }
        vec![frame.into_geometry()]
    }
}

/// Share of the plot's width a full attack, decay or release takes. The held
/// sustain fills what they leave, so the curve spans the whole plot.
const SEGMENT_SHARE: f32 = 0.3;
const HANDLE_RADIUS: f32 = 4.5;
const GRAB_RADIUS: f32 = 10.0;

/// A draggable point of the envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopePoint {
    /// The peak: its place along sets the attack.
    Attack,
    /// Where the decay lands: along sets the decay, up sets the sustain.
    Decay,
    /// Where the release starts: its distance from the end sets the release.
    Release,
}

pub struct EnvelopePlot {
    pub track_id: TrackId,
    pub index: usize,
    pub settings: ModulatorSettings,
    pub accent: Color,
}

#[derive(Debug, Default)]
pub struct EnvelopePlotState {
    drag: Option<(EnvelopePoint, UndoGestureId)>,
}

impl EnvelopePlot {
    fn segment(width: f32) -> f32 {
        (width - INSET * 2.0).max(1.0) * SEGMENT_SHARE
    }

    /// The start, peak, decay landing, end of the hold and end.
    fn corners(&self, width: f32, height: f32) -> [Point; 5] {
        let settings = &self.settings;
        let segment = Self::segment(width);
        let along = |field: ModField, time: f32| field.position_of(time) * segment;
        let (top, base) = (INSET, height - INSET);
        let sustain = base - settings.sustain.clamp(0.0, 1.0) * (base - top);
        let start = Point::new(INSET, base);
        let peak = Point::new(start.x + along(ModField::Attack, settings.attack_ms), top);
        let decayed = Point::new(peak.x + along(ModField::Decay, settings.decay_ms), sustain);
        let end = Point::new((width - INSET).max(INSET), base);
        let held = Point::new(
            end.x - along(ModField::Release, settings.release_ms),
            sustain,
        );
        [start, peak, decayed, held, end]
    }

    fn handles(&self, width: f32, height: f32) -> [(EnvelopePoint, Point); 3] {
        let [_, peak, decayed, held, _] = self.corners(width, height);
        [
            (EnvelopePoint::Attack, peak),
            (EnvelopePoint::Decay, decayed),
            (EnvelopePoint::Release, held),
        ]
    }

    /// The handle nearest `pointer`, within reach.
    fn hit(&self, pointer: Point, width: f32, height: f32) -> Option<EnvelopePoint> {
        self.handles(width, height)
            .into_iter()
            .map(|(point, at)| (point, at.distance(pointer)))
            .filter(|(_, distance)| *distance <= GRAB_RADIUS)
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(point, _)| point)
    }

    /// The settings with `point` moved to `pointer`.
    fn dragged(
        &self,
        point: EnvelopePoint,
        pointer: Point,
        width: f32,
        height: f32,
    ) -> ModulatorSettings {
        let [_, peak, _, _, end] = self.corners(width, height);
        let segment = Self::segment(width);
        let time = |field: ModField, from: f32| field.value_at((pointer.x - from) / segment);
        let mut settings = self.settings;
        match point {
            EnvelopePoint::Attack => settings.attack_ms = time(ModField::Attack, INSET),
            EnvelopePoint::Decay => {
                settings.decay_ms = time(ModField::Decay, peak.x);
                let travel = (height - INSET * 2.0).max(1.0);
                settings.sustain = ((height - INSET - pointer.y) / travel).clamp(0.0, 1.0);
            }
            EnvelopePoint::Release => {
                settings.release_ms = ModField::Release.value_at((end.x - pointer.x) / segment);
            }
        }
        settings.clamped()
    }
}

impl canvas::Program<Message> for EnvelopePlot {
    type State = EnvelopePlotState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (width, height) = (bounds.width, bounds.height);
        if height < MIN_HEIGHT || width <= INSET * 2.0 {
            return vec![frame.into_geometry()];
        }
        frame.fill(
            &canvas::Path::new(|path| {
                path.rounded_rectangle(Point::ORIGIN, bounds.size(), 3.0.into());
            }),
            th::bg_dark(),
        );
        let grid = th::with_alpha(th::border(), 0.35);
        frame.fill_rectangle(
            Point::new(0.0, (height / 2.0).floor()),
            iced::Size::new(width, 1.0),
            grid,
        );
        let accent = if self.settings.enabled {
            self.accent
        } else {
            th::text_muted()
        };
        let corners = self.corners(width, height);
        let curve = canvas::Path::new(|path| {
            path.move_to(corners[0]);
            for corner in &corners[1..] {
                path.line_to(*corner);
            }
        });
        // The curve starts and ends on the baseline, so it closes along it.
        frame.fill(&curve, th::with_alpha(accent, 0.16));
        frame.stroke(
            &curve,
            canvas::Stroke::default()
                .with_color(accent)
                .with_width(1.5)
                .with_line_join(canvas::LineJoin::Round),
        );
        let active = state.drag.map(|(point, _)| point).or_else(|| {
            cursor
                .position_in(bounds)
                .and_then(|pointer| self.hit(pointer, width, height))
        });
        for (point, at) in self.handles(width, height) {
            let radius = if active == Some(point) {
                HANDLE_RADIUS + 1.5
            } else {
                HANDLE_RADIUS
            };
            let handle = canvas::Path::circle(at, radius);
            frame.fill(&handle, accent);
            frame.stroke(
                &handle,
                canvas::Stroke::default()
                    .with_color(th::text())
                    .with_width(1.0),
            );
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
        let (width, height) = (bounds.width, bounds.height);
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let hit = cursor
                    .position_in(bounds)
                    .and_then(|pointer| self.hit(pointer, width, height));
                if let Some(point) = hit {
                    state.drag = Some((point, UndoGestureId::new()));
                    return (canvas::event::Status::Captured, None);
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let local = LocalDrag::unclamped().position(cursor, bounds);
                if let (Some((point, gesture)), Some(pointer)) = (state.drag, local) {
                    let next = self.dragged(point, pointer, width, height);
                    let edit = (next != self.settings).then(|| {
                        Message::Modulation(ModulationMsg::SetSettings(
                            self.track_id,
                            self.index,
                            next,
                        ))
                        .in_undo_gesture(gesture)
                    });
                    return (canvas::event::Status::Captured, edit);
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.drag.take().is_some() {
                    return (canvas::event::Status::Captured, None);
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
        if state.drag.is_some() {
            return mouse::Interaction::Grabbing;
        }
        let over = cursor
            .position_in(bounds)
            .and_then(|pointer| self.hit(pointer, bounds.width, bounds.height));
        if over.is_some() {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibez_core::modulation::ModShape;

    fn envelope() -> EnvelopePlot {
        EnvelopePlot {
            track_id: TrackId::new(),
            index: 0,
            settings: ModulatorSettings::default(),
            accent: Color::WHITE,
        }
    }

    #[test]
    fn dragging_a_point_to_where_it_is_keeps_the_envelope() {
        let plot = envelope();
        for (point, at) in plot.handles(260.0, 80.0) {
            assert_eq!(plot.hit(at, 260.0, 80.0), Some(point));
            let kept = plot.dragged(point, at, 260.0, 80.0);
            assert!((kept.attack_ms - plot.settings.attack_ms).abs() < 0.1);
            assert!((kept.decay_ms - plot.settings.decay_ms).abs() < 1.0);
            assert!((kept.release_ms - plot.settings.release_ms).abs() < 1.0);
            assert!((kept.sustain - plot.settings.sustain).abs() < 1e-3);
        }
    }

    #[test]
    fn dragging_the_decay_point_sets_the_sustain_level() {
        let plot = envelope();
        let top = plot.dragged(EnvelopePoint::Decay, Point::new(100.0, -20.0), 260.0, 80.0);
        assert_eq!(top.sustain, 1.0);
        let floor = plot.dragged(EnvelopePoint::Decay, Point::new(100.0, 200.0), 260.0, 80.0);
        assert_eq!(floor.sustain, 0.0);
    }

    fn plot(shape: ModShape, phase: f32, cycles: f64) -> LfoPlot {
        LfoPlot {
            settings: ModulatorSettings {
                shape,
                phase,
                ..Default::default()
            },
            cycles,
            accent: Color::WHITE,
        }
    }

    #[test]
    fn the_dot_travels_the_curve_once_per_cycle() {
        let (width, height) = (110.0, 50.0);
        let start = plot(ModShape::SawUp, 0.0, 0.0).dot(width, height);
        assert_eq!(start, Point::new(INSET, height - INSET));
        let half = plot(ModShape::SawUp, 0.0, 3.5).dot(width, height);
        assert_eq!(half, Point::new(width / 2.0, height / 2.0));
    }

    #[test]
    fn the_start_phase_shifts_the_curve_under_the_dot() {
        let dot = plot(ModShape::Square, 0.5, 0.0).dot(110.0, 50.0);
        assert_eq!(dot.y, 50.0 - INSET);
    }
}
