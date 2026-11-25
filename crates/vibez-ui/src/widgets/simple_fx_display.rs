//! Plots for the Chorus, Flanger, Pitch Shifter, Ring Modulator and
//! Frequency Shifter cards, each drawn from the effect's own maths: the
//! delay taps over two LFO cycles, the comb they make, the transposition
//! and the sidebands of a 1 kHz test tone.

use iced::alignment::Horizontal;
use iced::mouse;
use iced::widget::canvas;
use iced::{Color, Point, Rectangle, Renderer, Theme};

use crate::message::Message;
use crate::spectrum::{freq_to_norm, norm_to_freq};
use crate::theme as th;
use crate::widgets::phaser_display::{area, grid, label, panel, time_label, LABEL_H, LABEL_W};

const DISPLAY_RATE: f32 = 48_000.0;
const TOP_DB: f32 = 12.0;
const FLOOR_DB: f32 = -24.0;
/// The test tone the spectrum plots shift.
const TEST_HZ: f32 = 1_000.0;
const WIDE_AXIS_W: f32 = 300.0;

fn value(params: &[f32], descriptors: &[vibez_core::effect::ParamDescriptor], i: usize) -> f32 {
    params.get(i).copied().unwrap_or(descriptors[i].default)
}

fn stroke(color: Color, width: f32) -> canvas::Stroke<'static> {
    canvas::Stroke::default().with_color(color).with_width(width)
}

fn dim(color: Color) -> Color {
    Color { a: 0.45, ..color }
}

/// Which of the two modulated-delay effects a plot describes.
#[derive(Clone, Copy, PartialEq)]
pub enum Delayed {
    Chorus,
    Flanger,
}

impl Delayed {
    fn descriptors(self) -> &'static [vibez_core::effect::ParamDescriptor] {
        match self {
            Self::Chorus => vibez_dsp::chorus::CHORUS_PARAMS,
            Self::Flanger => vibez_dsp::flanger::FLANGER_PARAMS,
        }
    }

    /// Widest sweep (at Depth 1) and the top of the delay axis, in ms.
    fn sweep_ms(self) -> f32 {
        match self {
            Self::Chorus => 10.0,
            Self::Flanger => 5.0,
        }
    }

    fn axis_ms(self) -> f32 {
        match self {
            Self::Chorus => 50.0,
            Self::Flanger => 15.0,
        }
    }
}

/// Rate, depth, delay, feedback and mix, read the way the effects order them.
struct Delays {
    rate: f32,
    depth: f32,
    delay: f32,
    feedback: f32,
    mix: f32,
}

fn delays(kind: Delayed, params: &[f32]) -> Delays {
    let d = kind.descriptors();
    Delays {
        rate: value(params, d, 0),
        depth: value(params, d, 1),
        delay: value(params, d, 2),
        feedback: value(params, d, 3),
        mix: value(params, d, 4),
    }
}

/// The two read taps' delay in ms over two LFO cycles; they move in
/// anti-phase around the centre delay.
pub struct DelaySweepPlot<'a> {
    pub kind: Delayed,
    pub params: &'a [f32],
    pub accent: Color,
}

impl canvas::Program<Message> for DelaySweepPlot<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        panel(&mut frame, bounds.size());
        let (plot, labels) = area(bounds.size());
        let p = delays(self.kind, self.params);
        let top = self.kind.axis_ms();
        let y = |ms: f32| plot.y + plot.height * (1.0 - (ms / top).clamp(0.0, 1.0));
        for step in 0..=2 {
            let ms = top * step as f32 / 2.0;
            frame.stroke(
                &canvas::Path::line(
                    Point::new(plot.x, y(ms)),
                    Point::new(plot.x + plot.width, y(ms)),
                ),
                stroke(grid(), 1.0),
            );
            if labels {
                label(
                    &mut frame,
                    format!("{ms:.0}"),
                    Point::new(LABEL_W - 4.0, y(ms)),
                    Horizontal::Right,
                );
            }
        }
        let seconds = 2.0 / p.rate.max(0.01);
        if labels {
            for (position, horizontal) in [
                (0.0f32, Horizontal::Left),
                (0.5, Horizontal::Center),
                (1.0, Horizontal::Right),
            ] {
                label(
                    &mut frame,
                    time_label(seconds * position, position == 1.0),
                    Point::new(
                        plot.x + plot.width * position,
                        plot.y + plot.height + LABEL_H / 2.0 + 1.0,
                    ),
                    horizontal,
                );
            }
        }
        let sweep = self.kind.sweep_ms() * p.depth;
        let tap = |phase: f32| {
            canvas::Path::new(|builder| {
                for step in 0..=128 {
                    let position = step as f32 / 128.0;
                    let angle = std::f32::consts::TAU * 2.0 * position + phase;
                    let ms = (p.delay + sweep * angle.sin()).max(0.0);
                    let point = Point::new(plot.x + position * plot.width, y(ms));
                    if step == 0 {
                        builder.move_to(point);
                    } else {
                        builder.line_to(point);
                    }
                }
            })
        };
        frame.stroke(&tap(std::f32::consts::PI), stroke(dim(self.accent), 1.5));
        frame.stroke(&tap(0.0), stroke(self.accent, 2.0));
        vec![frame.into_geometry()]
    }
}

/// Gain against frequency with both taps at the centre delay (accent) and
/// at the widest spread of the sweep (grey).
pub struct CombPlot<'a> {
    pub kind: Delayed,
    pub params: &'a [f32],
    pub accent: Color,
}

fn comb_db(kind: Delayed, p: &Delays, spread: f32, frequency: f32) -> f32 {
    let sweep = kind.sweep_ms() * p.depth * spread;
    let omega = std::f32::consts::TAU * frequency / DISPLAY_RATE;
    let samples = |ms: f32| (ms * 0.001 * DISPLAY_RATE).max(1.0);
    let (d0, d1) = (samples(p.delay + sweep), samples(p.delay - sweep));
    // wet = T / (1 - fb * T) with T the mean of the two delayed taps.
    let t = (
        0.5 * ((-omega * d0).cos() + (-omega * d1).cos()),
        0.5 * ((-omega * d0).sin() + (-omega * d1).sin()),
    );
    let (nr, ni) = (t.0, t.1);
    let (dr, di) = (1.0 - p.feedback * t.0, -p.feedback * t.1);
    let norm = (dr * dr + di * di).max(1e-6);
    let wet = ((nr * dr + ni * di) / norm, (ni * dr - nr * di) / norm);
    let (hr, hi) = ((1.0 - p.mix) + p.mix * wet.0, p.mix * wet.1);
    20.0 * (hr * hr + hi * hi).sqrt().max(1e-4).log10()
}

fn frequency_axis(
    frame: &mut canvas::Frame,
    plot: Rectangle,
    labels: bool,
) {
    let named: &[f32] = if plot.width >= WIDE_AXIS_W {
        &[
            20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0,
        ]
    } else {
        &[20.0, 100.0, 1_000.0, 10_000.0, 20_000.0]
    };
    for &frequency in named {
        let x = plot.x + freq_to_norm(frequency) * plot.width;
        frame.stroke(
            &canvas::Path::line(Point::new(x, plot.y), Point::new(x, plot.y + plot.height)),
            stroke(grid(), 1.0),
        );
        if labels {
            let text = if frequency >= 1_000.0 {
                format!("{:.0}k", frequency / 1_000.0)
            } else {
                format!("{frequency:.0}")
            };
            label(
                frame,
                text,
                Point::new(x, plot.y + plot.height + LABEL_H / 2.0 + 1.0),
                Horizontal::Center,
            );
        }
    }
}

impl canvas::Program<Message> for CombPlot<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        panel(&mut frame, bounds.size());
        let (plot, labels) = area(bounds.size());
        let y = |db: f32| {
            plot.y + plot.height * (TOP_DB - db.clamp(FLOOR_DB, TOP_DB)) / (TOP_DB - FLOOR_DB)
        };
        for db in [12.0f32, 0.0, -12.0, -24.0] {
            frame.stroke(
                &canvas::Path::line(
                    Point::new(plot.x, y(db)),
                    Point::new(plot.x + plot.width, y(db)),
                ),
                stroke(grid(), 1.0),
            );
            if labels {
                label(
                    &mut frame,
                    format!("{db:.0}"),
                    Point::new(LABEL_W - 4.0, y(db)),
                    Horizontal::Right,
                );
            }
        }
        frequency_axis(&mut frame, plot, labels);
        let p = delays(self.kind, self.params);
        let steps = (plot.width * 1.5).clamp(160.0, 640.0) as usize;
        let curve = |spread: f32| {
            canvas::Path::new(|builder| {
                for step in 0..=steps {
                    let position = step as f32 / steps as f32;
                    let db = comb_db(self.kind, &p, spread, norm_to_freq(position));
                    let point = Point::new(plot.x + position * plot.width, y(db));
                    if step == 0 {
                        builder.move_to(point);
                    } else {
                        builder.line_to(point);
                    }
                }
            })
        };
        frame.stroke(&curve(1.0), stroke(th::text_muted(), 1.5));
        frame.stroke(&curve(0.0), stroke(self.accent, 2.0));
        vec![frame.into_geometry()]
    }
}

/// The transposition on a -12..+12 semitone ruler: where the input sits
/// (0) and where the output lands.
pub struct PitchPlot<'a> {
    pub params: &'a [f32],
    pub accent: Color,
}

impl canvas::Program<Message> for PitchPlot<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let d = vibez_dsp::pitch_shifter::PITCH_SHIFTER_PARAMS;
        let semitones = value(self.params, d, 0);
        let fine = value(self.params, d, 1);
        let shift = semitones + fine / 100.0;
        let ratio = 2.0f32.powf(shift / 12.0);

        let mut frame = canvas::Frame::new(renderer, bounds.size());
        panel(&mut frame, bounds.size());
        let (plot, labels) = area(bounds.size());
        let x = |st: f32| plot.x + plot.width * (st.clamp(-12.0, 12.0) + 12.0) / 24.0;
        let mid = plot.y + plot.height * 0.62;
        frame.stroke(
            &canvas::Path::line(Point::new(plot.x, mid), Point::new(plot.x + plot.width, mid)),
            stroke(grid(), 1.0),
        );
        for st in -12..=12 {
            let octave = st % 12 == 0;
            let half = if octave {
                6.0
            } else if st % 3 == 0 {
                4.0
            } else {
                2.5
            };
            frame.stroke(
                &canvas::Path::line(
                    Point::new(x(st as f32), mid - half),
                    Point::new(x(st as f32), mid + half),
                ),
                stroke(if octave { th::text_dim() } else { grid() }, 1.0),
            );
            if labels && st % 6 == 0 {
                label(
                    &mut frame,
                    format!("{st:+}"),
                    Point::new(x(st as f32), plot.y + plot.height + LABEL_H / 2.0 + 1.0),
                    Horizontal::Center,
                );
            }
        }
        // The span from the input to the output.
        frame.stroke(
            &canvas::Path::line(Point::new(x(0.0), mid), Point::new(x(shift), mid)),
            stroke(dim(self.accent), 4.0),
        );
        frame.fill(
            &canvas::Path::circle(Point::new(x(0.0), mid), 4.0),
            th::text_dim(),
        );
        frame.fill(
            &canvas::Path::circle(Point::new(x(shift), mid), 6.0),
            self.accent,
        );
        if plot.height >= 36.0 {
            frame.fill_text(canvas::Text {
                content: format!("{shift:+.2} st  ·  x{ratio:.3}"),
                position: Point::new(plot.x + plot.width * 0.5, plot.y + 8.0),
                color: th::text(),
                size: 11.0.into(),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        }
        vec![frame.into_geometry()]
    }
}

/// What a 1 kHz tone comes out as: a ring modulator swaps it for the sum
/// and difference with the carrier, a frequency shifter slides it by the
/// shift (a negative result folds back around 0 Hz).
#[derive(Clone, Copy, PartialEq)]
pub enum Shifting {
    Ring,
    Frequency,
}

pub struct SidebandPlot<'a> {
    pub kind: Shifting,
    pub params: &'a [f32],
    pub accent: Color,
}

impl canvas::Program<Message> for SidebandPlot<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        panel(&mut frame, bounds.size());
        let (plot, labels) = area(bounds.size());
        frequency_axis(&mut frame, plot, labels);
        let x = |hz: f32| plot.x + freq_to_norm(hz.clamp(20.0, 20_000.0)) * plot.width;
        let base = plot.y + plot.height;
        let line = |frame: &mut canvas::Frame, hz: f32, height: f32, color: Color, width: f32| {
            if height <= 0.001 {
                return;
            }
            let top = base - plot.height * height.min(1.0) * 0.9;
            frame.stroke(
                &canvas::Path::line(Point::new(x(hz), base), Point::new(x(hz), top)),
                stroke(color, width),
            );
            frame.fill(
                &canvas::Path::circle(Point::new(x(hz), top), 2.5),
                color,
            );
        };
        // The test tone, as it goes in.
        line(&mut frame, TEST_HZ, 1.0, dim(th::text_dim()), 1.0);
        match self.kind {
            Shifting::Ring => {
                let d = vibez_dsp::ring_modulator::RING_MODULATOR_PARAMS;
                let carrier = value(self.params, d, 0);
                let mix = value(self.params, d, 1);
                line(&mut frame, TEST_HZ, 1.0 - mix, th::text_muted(), 2.0);
                line(&mut frame, (TEST_HZ - carrier).abs(), mix * 0.5, self.accent, 2.0);
                line(&mut frame, TEST_HZ + carrier, mix * 0.5, self.accent, 2.0);
            }
            Shifting::Frequency => {
                let d = vibez_dsp::frequency_shifter::FREQUENCY_SHIFTER_PARAMS;
                let shift = value(self.params, d, 0);
                let mix = value(self.params, d, 1);
                line(&mut frame, TEST_HZ, 1.0 - mix, th::text_muted(), 2.0);
                let shifted = TEST_HZ + shift;
                // Past 0 Hz the tone reflects, so it reads |f|.
                line(&mut frame, shifted.abs(), mix, self.accent, 2.0);
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_comb_is_flat() {
        let p = Delays {
            rate: 1.0,
            depth: 0.5,
            delay: 4.0,
            feedback: 0.4,
            mix: 0.0,
        };
        for hz in [100.0, 1_000.0, 9_000.0] {
            assert!(comb_db(Delayed::Flanger, &p, 1.0, hz).abs() < 1e-3);
        }
    }

    #[test]
    fn mixed_comb_notches_where_dry_and_wet_cancel() {
        // Dry plus a 1 ms echo at equal level nulls at 500 Hz and is back to 0 dB at 1 kHz.
        let p = Delays {
            rate: 1.0,
            depth: 0.0,
            delay: 1.0,
            feedback: 0.0,
            mix: 0.5,
        };
        assert!(comb_db(Delayed::Flanger, &p, 0.0, 500.0) < -20.0);
        assert!((comb_db(Delayed::Flanger, &p, 0.0, 1_000.0) - 0.0).abs() < 0.1);
    }
}
