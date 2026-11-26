//! The LFO plot of a modulator card: one cycle of its shape, with a dot
//! where the cycle is now.

use iced::widget::canvas;
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};
use vibez_core::modulation::ModulatorSettings;

use crate::message::Message;
use crate::theme as th;

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

#[cfg(test)]
mod tests {
    use super::*;
    use vibez_core::modulation::ModShape;

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
