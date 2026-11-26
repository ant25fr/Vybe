//! The Plucked String card's scopes and its pluckable strings: an ADSR
//! envelope plot, the feedback loop's damping low-pass response, and a set
//! of horizontal strings the mouse can pluck and strum (x along a string is
//! an octave of pitch, y picks the string).

use iced::mouse;
use iced::widget::canvas;
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use vibez_core::id::TrackId;
use vibez_instruments::plucked::param;

use crate::message::Message;
use crate::theme as th;

/// The strings' open tunings, low to high: E2 A2 D3 G3 B3 E4.
pub const STRINGS: [u8; 6] = [40, 45, 50, 55, 59, 64];
/// Semitones reachable along one string.
const FRETS: f32 = 12.0;
const X_INSET: f32 = 8.0;

fn value(params: &[f32], index: usize) -> f32 {
    params
        .get(index)
        .copied()
        .unwrap_or(vibez_instruments::plucked::PLUCKED_PARAMS[index].default)
}

fn plot_rect(size: Size) -> Rectangle {
    Rectangle::new(
        Point::new(6.0, 4.0),
        Size::new((size.width - 12.0).max(1.0), (size.height - 10.0).max(1.0)),
    )
}

/// Faint horizontal grid lines across the plot, like the other scopes.
fn grid(frame: &mut canvas::Frame, plot: Rectangle, levels: &[f32]) {
    for &level in levels {
        let y = plot.y + level * plot.height;
        frame.stroke(
            &canvas::Path::line(Point::new(plot.x, y), Point::new(plot.x + plot.width, y)),
            canvas::Stroke::default()
                .with_color(th::with_alpha(th::border(), 0.4))
                .with_width(1.0),
        );
    }
}

/// The ADSR envelope as a curve over time.
pub struct EnvelopePlot<'a> {
    pub params: &'a [f32],
    pub accent: Color,
}

pub struct PlotState {
    cache: canvas::Cache,
    drawn: std::cell::RefCell<Vec<f32>>,
    epoch: std::cell::Cell<u64>,
}

impl Default for PlotState {
    fn default() -> Self {
        Self {
            cache: canvas::Cache::new(),
            drawn: std::cell::RefCell::new(Vec::new()),
            epoch: std::cell::Cell::new(u64::MAX),
        }
    }
}

impl PlotState {
    fn redraws(&self, params: &[f32]) -> bool {
        let mut drawn = self.drawn.borrow_mut();
        if *drawn == params && self.epoch.get() == th::epoch() {
            return false;
        }
        *drawn = params.to_vec();
        self.epoch.set(th::epoch());
        true
    }
}

impl canvas::Program<Message> for EnvelopePlot<'_> {
    type State = PlotState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        if state.redraws(self.params) {
            state.cache.clear();
        }
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            let size = frame.size();
            frame.fill_rectangle(Point::ORIGIN, size, th::bg_dark());
            let plot = plot_rect(size);
            grid(frame, plot, &[0.25, 0.5, 0.75]);
            let attack = value(self.params, param::ATTACK);
            let decay = value(self.params, param::DECAY);
            let sustain = value(self.params, param::SUSTAIN);
            let release = value(self.params, param::RELEASE);
            let hold = decay.max(0.2);
            let total = (attack + decay + hold + release).max(0.05);
            let x = |t: f32| plot.x + t / total * plot.width;
            let y = |level: f32| plot.y + (1.0 - level.clamp(0.0, 1.0)) * plot.height;
            // Sustain level guide.
            frame.stroke(
                &canvas::Path::line(
                    Point::new(plot.x, y(sustain)),
                    Point::new(plot.x + plot.width, y(sustain)),
                ),
                canvas::Stroke::default()
                    .with_color(th::with_alpha(th::text_muted(), 0.35))
                    .with_width(1.0),
            );
            let steps = 24;
            let mut points = Vec::with_capacity(2 * steps + 4);
            points.push(Point::new(x(0.0), y(0.0)));
            points.push(Point::new(x(attack), y(1.0)));
            for i in 1..=steps {
                let t = attack + decay * i as f32 / steps as f32;
                let level =
                    sustain + (1.0 - sustain) * (-3.0 * (t - attack) / decay.max(1e-3)).exp();
                points.push(Point::new(x(t), y(level)));
            }
            points.push(Point::new(x(attack + decay + hold), y(sustain)));
            for i in 1..=steps {
                let t = attack + decay + hold + release * i as f32 / steps as f32;
                let level =
                    sustain * (-4.0 * (t - attack - decay - hold) / release.max(1e-3)).exp();
                points.push(Point::new(x(t), y(level)));
            }
            let line = |close: bool| {
                canvas::Path::new(|builder| {
                    builder.move_to(points[0]);
                    for p in &points[1..] {
                        builder.line_to(*p);
                    }
                    if close {
                        let last = points[points.len() - 1];
                        builder.line_to(Point::new(last.x, plot.y + plot.height));
                        builder.line_to(Point::new(points[0].x, plot.y + plot.height));
                        builder.close();
                    }
                })
            };
            frame.fill(&line(true), th::with_alpha(self.accent, 0.16));
            frame.stroke(
                &line(false),
                canvas::Stroke::default()
                    .with_color(self.accent)
                    .with_width(2.0)
                    .with_line_join(canvas::LineJoin::Round),
            );
            // Stage boundaries: peak, end of decay, start of release.
            for (t, level) in [
                (attack, 1.0),
                (attack + decay, sustain),
                (attack + decay + hold, sustain),
            ] {
                let p = Point::new(x(t), y(level));
                frame.fill(&canvas::Path::circle(p, 3.0), th::bg_dark());
                frame.stroke(
                    &canvas::Path::circle(p, 3.0),
                    canvas::Stroke::default()
                        .with_color(self.accent)
                        .with_width(1.5),
                );
            }
        });
        vec![geometry]
    }
}

/// The loop's damping low-pass response, where Damping sets the one-pole
/// coefficient k = 1 - 0.99*damping and so the cutoff k*sr/2pi.
pub struct LoopFilterPlot<'a> {
    pub params: &'a [f32],
    pub sample_rate: f32,
    pub accent: Color,
}

const FLOOR_DB: f32 = -60.0;

impl canvas::Program<Message> for LoopFilterPlot<'_> {
    type State = PlotState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        if state.redraws(self.params) {
            state.cache.clear();
        }
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            let size = frame.size();
            frame.fill_rectangle(Point::ORIGIN, size, th::bg_dark());
            let plot = plot_rect(size);
            // Grid at 0, -20, -40, -60 dB.
            let db_to_y =
                |db: f32| plot.y + (3.0 - db.clamp(FLOOR_DB, 3.0)) / (3.0 - FLOOR_DB) * plot.height;
            for db in [0.0, -20.0, -40.0, FLOOR_DB] {
                let y = db_to_y(db);
                frame.stroke(
                    &canvas::Path::line(Point::new(plot.x, y), Point::new(plot.x + plot.width, y)),
                    canvas::Stroke::default()
                        .with_color(th::with_alpha(th::border(), 0.4))
                        .with_width(1.0),
                );
            }
            let sr = if self.sample_rate > 0.0 {
                self.sample_rate
            } else {
                48_000.0
            };
            let damping = value(self.params, param::DAMPING);
            let k = 1.0 - 0.99 * damping.clamp(0.0, 1.0);
            let freq_to_x = |f: f32| {
                let norm = (f.log10() - 20.0_f32.log10()) / ((sr * 0.5).log10() - 20.0_f32.log10());
                plot.x + norm.clamp(0.0, 1.0) * plot.width
            };
            let db_to_y =
                |db: f32| plot.y + (3.0 - db.clamp(FLOOR_DB, 3.0)) / (3.0 - FLOOR_DB) * plot.height;
            let mut points = Vec::with_capacity(49);
            for i in 0..=48 {
                let f = 20.0 * (sr * 0.5 / 20.0).powf(i as f32 / 48.0);
                let w = std::f32::consts::TAU * f / sr;
                let mag = k / (1.0 - (1.0 - k) * w.cos()).hypot((1.0 - k) * w.sin());
                let db = 20.0 * mag.max(1e-6).log10();
                points.push(Point::new(freq_to_x(f), db_to_y(db)));
            }
            let line = |close: bool| {
                canvas::Path::new(|builder| {
                    builder.move_to(points[0]);
                    for p in &points[1..] {
                        builder.line_to(*p);
                    }
                    if close {
                        let bottom = plot.y + plot.height;
                        builder.line_to(Point::new(points[48].x, bottom));
                        builder.line_to(Point::new(points[0].x, bottom));
                        builder.close();
                    }
                })
            };
            frame.fill(&line(true), th::with_alpha(self.accent, 0.16));
            frame.stroke(
                &line(false),
                canvas::Stroke::default()
                    .with_color(self.accent)
                    .with_width(2.0)
                    .with_line_join(canvas::LineJoin::Round),
            );
            // Cutoff marker: |H| = k/sqrt(2) near k*sr/2pi for small k.
            let cutoff = (k * sr / std::f32::consts::TAU).clamp(20.0, sr * 0.5);
            let x = freq_to_x(cutoff);
            frame.stroke(
                &canvas::Path::line(Point::new(x, plot.y), Point::new(x, plot.y + plot.height)),
                canvas::Stroke::default()
                    .with_color(th::with_alpha(self.accent, 0.4))
                    .with_width(1.0),
            );
            let w = std::f32::consts::TAU * cutoff / sr;
            let mag = k / (1.0 - (1.0 - k) * w.cos()).hypot((1.0 - k) * w.sin());
            let dot = Point::new(x, db_to_y(20.0 * mag.max(1e-6).log10()));
            frame.fill(&canvas::Path::circle(dot, 3.5), self.accent);
        });
        vec![geometry]
    }
}

/// The horizontal strings: press plucks the pitch under the cursor, dragging
/// along a string glissandos through its octave, dragging across strings
/// strums; releasing lets every plucked pitch go.
pub struct PluckedStrings {
    pub track_id: TrackId,
    pub accent: Color,
}

#[derive(Default)]
pub struct StringsState {
    /// Pitches plucked during the current gesture, released on mouse-up.
    held: Option<Vec<u8>>,
    /// The string under the cursor, highlighted while held.
    last_string: Option<usize>,
}

impl PluckedStrings {
    fn string_at(&self, y: f32, bounds: Rectangle) -> usize {
        let sh = bounds.height / STRINGS.len() as f32;
        ((y / sh).floor() as usize).min(STRINGS.len() - 1)
    }

    fn pitch_at(&self, pos: Point, bounds: Rectangle) -> Option<u8> {
        if bounds.height < 4.0 * STRINGS.len() as f32 {
            return None;
        }
        let string = self.string_at(pos.y, bounds);
        let x0 = X_INSET;
        let x1 = (bounds.width - X_INSET).max(x0 + 1.0);
        let xn = ((pos.x - x0) / (x1 - x0)).clamp(0.0, 1.0);
        let pitch = STRINGS[string] as i32 + (xn * FRETS).round() as i32;
        Some(pitch.clamp(0, 127) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings() -> PluckedStrings {
        PluckedStrings {
            track_id: TrackId::new(),
            accent: Color::WHITE,
        }
    }

    #[test]
    fn x_along_a_string_spans_an_octave_from_its_open_pitch() {
        let s = strings();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(408.0, 60.0));
        // First string (E2 = 40), left edge = open note, right edge = +12.
        let y = 5.0;
        assert_eq!(s.pitch_at(Point::new(8.0, y), bounds), Some(40));
        assert_eq!(s.pitch_at(Point::new(400.0, y), bounds), Some(52));
        // Middle of the top string (E4 = 64) is +6.
        let y_top = 55.0;
        assert_eq!(s.pitch_at(Point::new(204.0, y_top), bounds), Some(70));
    }
}

impl canvas::Program<Message> for PluckedStrings {
    type State = StringsState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(pos) = cursor.position_in(bounds) else {
                    return (canvas::event::Status::Ignored, None);
                };
                let Some(pitch) = self.pitch_at(pos, bounds) else {
                    return (canvas::event::Status::Ignored, None);
                };
                state.last_string = Some(self.string_at(pos.y, bounds));
                state.held = Some(vec![pitch]);
                (
                    canvas::event::Status::Captured,
                    Some(Message::audition_note(self.track_id, pitch, true)),
                )
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(held) = &mut state.held else {
                    return (canvas::event::Status::Ignored, None);
                };
                let Some(pos) = cursor.position_in(bounds) else {
                    return (canvas::event::Status::Ignored, None);
                };
                let Some(pitch) = self.pitch_at(pos, bounds) else {
                    return (canvas::event::Status::Ignored, None);
                };
                state.last_string = Some(self.string_at(pos.y, bounds));
                if held.last() == Some(&pitch) {
                    return (canvas::event::Status::Captured, None);
                }
                held.push(pitch);
                (
                    canvas::event::Status::Captured,
                    Some(Message::audition_note(self.track_id, pitch, true)),
                )
            }
            canvas::Event::Mouse(
                mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft,
            ) => {
                if let Some(held) = state.held.take() {
                    state.last_string = None;
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::release_audition_notes(self.track_id, held)),
                    );
                }
                (canvas::event::Status::Ignored, None)
            }
            _ => (canvas::event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), th::bg_dark());
        let sh = bounds.height / STRINGS.len() as f32;
        let x0 = X_INSET;
        let x1 = (bounds.width - X_INSET).max(x0 + 1.0);
        let fret_x = |n: f32| x0 + n / FRETS * (x1 - x0);
        let pointer = cursor.position_in(bounds);
        // Fret wires, with heavier ones at the octave and the inlay frets.
        for n in 1..=FRETS as usize {
            let x = fret_x(n as f32);
            let strong = matches!(n, 5 | 7 | 12);
            frame.stroke(
                &canvas::Path::line(Point::new(x, 2.0), Point::new(x, bounds.height - 2.0)),
                canvas::Stroke::default()
                    .with_color(th::with_alpha(
                        th::border(),
                        if strong { 0.55 } else { 0.28 },
                    ))
                    .with_width(if n == 12 { 1.6 } else { 1.0 }),
            );
        }
        // Inlay dots between the wires, centred on the board.
        let mid = bounds.height / 2.0;
        for n in [3.0_f32, 5.0, 7.0, 9.0] {
            let x = (fret_x(n - 1.0) + fret_x(n)) / 2.0;
            frame.fill(
                &canvas::Path::circle(Point::new(x, mid), 2.0),
                th::with_alpha(th::text_muted(), 0.22),
            );
        }
        for (i, _) in STRINGS.iter().enumerate() {
            let y = (i as f32 + 0.5) * sh;
            let active = state.held.is_some() && state.last_string == Some(i);
            // Bass strings are thicker, like wound strings.
            let base = 1.0 + (STRINGS.len() - 1 - i) as f32 * 0.35;
            let path = match pointer.filter(|_| active) {
                // A plucked string bows towards the pointer.
                Some(p) => {
                    let px = p.x.clamp(x0 + 1.0, x1 - 1.0);
                    let pull = ((p.y - y) * 0.6).clamp(-sh * 0.45, sh * 0.45);
                    canvas::Path::new(|b| {
                        b.move_to(Point::new(x0, y));
                        b.line_to(Point::new(px, y + pull));
                        b.line_to(Point::new(x1, y));
                    })
                }
                None => canvas::Path::line(Point::new(x0, y), Point::new(x1, y)),
            };
            if active {
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(th::with_alpha(self.accent, 0.25))
                        .with_width(base + 5.0),
                );
            }
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(if active {
                        self.accent
                    } else {
                        th::with_alpha(th::text_dim(), 0.65)
                    })
                    .with_width(if active { base + 1.0 } else { base }),
            );
            // Nut.
            frame.fill_rectangle(
                Point::new(x0 - 3.0, y - sh * 0.35),
                Size::new(2.0, sh * 0.7),
                th::with_alpha(th::text_muted(), 0.5),
            );
        }
        // Where the pitch under the pointer sits on the held string.
        if let (Some(p), Some(i)) = (pointer, state.last_string.filter(|_| state.held.is_some())) {
            let x = fret_x((((p.x - x0) / (x1 - x0)).clamp(0.0, 1.0) * FRETS).round());
            frame.fill(
                &canvas::Path::circle(Point::new(x, (i as f32 + 0.5) * sh), 4.0),
                self.accent,
            );
        }
        vec![frame.into_geometry()]
    }
}
