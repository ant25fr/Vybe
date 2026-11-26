//! Audio Clip fade and gain handles: hit testing and reciprocal drag
//! geometry.

use iced::Point;

use crate::state::{AudioClipFadeEdge, UndoGestureId};
use vibez_core::id::ClipId;
use vibez_core::track::{ClipGainDb, FadeCurve};

use super::clips::TrackClipCanvas;
use super::{TimelineClip, CLIP_TITLE_HEIGHT, CLIP_Y, FADE_HANDLE_HIT_RADIUS, FADE_HANDLE_Y};

/// The gain handle sits just under the title.
const GAIN_HANDLE_Y: f32 = CLIP_Y + CLIP_TITLE_HEIGHT + 3.0;
const GAIN_HANDLE_HIT_HEIGHT: f32 = 4.0;
/// How near a fade handle's height the pointer must be on an unselected Clip.
const HOVER_HANDLE_HIT_HEIGHT: f32 = 3.5;
/// Half the width of the gain handle.
pub(super) const GAIN_HANDLE_HALF_WIDTH: f32 = 8.0;
/// A narrower Clip has no room for a gain handle between its fade handles.
const GAIN_HANDLE_MIN_CLIP_PX: f32 = 48.0;
/// A narrower unselected Clip has no fade handles under the pointer.
pub(super) const HOVER_HANDLES_MIN_CLIP_PX: f32 = 28.0;
const GAIN_DB_PER_PIXEL: f32 = 0.25;
const GAIN_FINE_DIVISOR: f32 = 5.0;

#[derive(Debug, Clone)]
pub struct FadeClipDrag {
    pub undo_gesture: UndoGestureId,
    pub clip_id: ClipId,
    pub edge: AudioClipFadeEdge,
    clip_x: f32,
    clip_width: f32,
    duration_frames: u64,
    handle: Point,
    snap: Option<FadeSnap>,
}

#[derive(Debug, Clone, Copy)]
struct FadeSnap {
    x: f32,
    frames: u64,
}

#[derive(Debug, Clone)]
pub struct FadeCurveDrag {
    pub undo_gesture: UndoGestureId,
    pub clip_id: ClipId,
    pub edge: AudioClipFadeEdge,
    top: f32,
    bottom: f32,
    handle: Point,
}

/// A drag of the gain handle: up is louder, and Shift moves finely.
#[derive(Debug, Clone)]
pub struct GainDrag {
    pub undo_gesture: UndoGestureId,
    pub clip_id: ClipId,
    /// The gain under the pointer, before rounding.
    db: f32,
    /// The last gain sent, so a still pointer sends nothing.
    sent: f32,
    last_y: f32,
    handle: Point,
}

impl GainDrag {
    /// The gain to send for the pointer at `y`, when it changed.
    pub(super) fn drag_to(&mut self, y: f32, fine: bool) -> Option<f32> {
        let step = if fine {
            GAIN_DB_PER_PIXEL / GAIN_FINE_DIVISOR
        } else {
            GAIN_DB_PER_PIXEL
        };
        self.db = (self.db + (self.last_y - y) * step).clamp(ClipGainDb::MIN, ClipGainDb::MAX);
        self.last_y = y;
        // Tenths, as the readout shows.
        let rounded = (self.db * 10.0).round() / 10.0;
        (rounded != self.sent).then(|| {
            self.sent = rounded;
            rounded
        })
    }

    /// The gain the Clip had when pressed, or was last sent.
    pub(super) fn current(&self) -> f32 {
        self.sent
    }
}

/// Where the gain handle sits: the top middle of the Clip's body.
pub(super) fn gain_handle(clip_x: f32, clip_width: f32) -> Option<Point> {
    (clip_width >= GAIN_HANDLE_MIN_CLIP_PX)
        .then(|| Point::new(clip_x + clip_width / 2.0, GAIN_HANDLE_Y))
}

pub(super) enum FadeControlDrag {
    Gain(GainDrag),
    Length(FadeClipDrag),
    Curve(FadeCurveDrag),
    CrossfadeCurve(CrossfadeCurveDrag),
}

impl FadeControlDrag {
    fn distance_squared(&self, point: Point) -> f32 {
        match self {
            Self::Gain(drag) => {
                (point.x - drag.handle.x).powi(2) + (point.y - drag.handle.y).powi(2)
            }
            Self::Length(drag) => drag.distance_squared(point),
            Self::Curve(drag) => drag.distance_squared(point),
            Self::CrossfadeCurve(drag) => drag.distance_squared(point),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CrossfadeCurveDrag {
    pub undo_gesture: UndoGestureId,
    pub outgoing_id: ClipId,
    pub incoming_id: ClipId,
    top: f32,
    bottom: f32,
    handle: Point,
}

impl CrossfadeCurveDrag {
    fn new(outgoing_id: ClipId, incoming_id: ClipId, canvas_height: f32, handle: Point) -> Self {
        Self {
            undo_gesture: UndoGestureId::new(),
            outgoing_id,
            incoming_id,
            top: CLIP_Y + CLIP_TITLE_HEIGHT + 2.0,
            bottom: canvas_height - CLIP_Y - 2.0,
            handle,
        }
    }

    fn distance_squared(&self, point: Point) -> f32 {
        (point.x - self.handle.x).powi(2) + (point.y - self.handle.y).powi(2)
    }

    pub(super) fn curve_at_y(&self, y: f32) -> FadeCurve {
        let height = (self.bottom - self.top).max(1.0);
        let min_gain = FadeCurve::new(-100).crossfade_gains(0.5).1;
        let max_gain = FadeCurve::new(100).crossfade_gains(0.5).1;
        let gain = ((self.bottom - y) / height).clamp(min_gain, max_gain);
        let warped = gain.asin() / std::f32::consts::FRAC_PI_2;
        let exponent = warped.ln() / 0.5_f32.ln();
        FadeCurve::new((-50.0 * exponent.log2()).round() as i16)
    }
}

impl FadeCurveDrag {
    fn new(clip_id: ClipId, edge: AudioClipFadeEdge, canvas_height: f32, handle: Point) -> Self {
        Self {
            undo_gesture: UndoGestureId::new(),
            clip_id,
            edge,
            top: CLIP_Y + CLIP_TITLE_HEIGHT + 2.0,
            bottom: canvas_height - CLIP_Y - 2.0,
            handle,
        }
    }

    fn distance_squared(&self, point: Point) -> f32 {
        (point.x - self.handle.x).powi(2) + (point.y - self.handle.y).powi(2)
    }

    pub(super) fn curve_at_y(&self, y: f32) -> FadeCurve {
        let height = (self.bottom - self.top).max(1.0);
        let gain = ((self.bottom - y) / height).clamp(
            FadeCurve::new(-100).gain(0.5),
            FadeCurve::new(100).gain(0.5),
        );
        let exponent = gain.ln() / 0.5_f32.ln();
        FadeCurve::new((-50.0 * exponent.log2()).round() as i16)
    }
}

impl FadeClipDrag {
    fn new(
        clip_id: ClipId,
        edge: AudioClipFadeEdge,
        clip_x: f32,
        clip_width: f32,
        duration_frames: u64,
        handle: Point,
        snap: Option<FadeSnap>,
    ) -> Self {
        Self {
            undo_gesture: UndoGestureId::new(),
            clip_id,
            edge,
            clip_x,
            clip_width,
            duration_frames,
            handle,
            snap,
        }
    }

    fn distance_squared(&self, point: Point) -> f32 {
        (point.x - self.handle.x).powi(2) + (point.y - self.handle.y).powi(2)
    }

    /// The fade that meets the overlapping neighbour, while the pointer at
    /// `x` is within reach of its edge.
    pub(super) fn crossfade_frames(&self, x: f32) -> Option<u64> {
        self.snap
            .filter(|snap| (x - snap.x).abs() <= FADE_HANDLE_HIT_RADIUS)
            .map(|snap| snap.frames)
    }

    /// Use the exact inverse of [`fade_handle_xs`] so a handle drawn at one
    /// frame cannot jitter to an adjacent frame when the pointer has not moved.
    pub(super) fn frames_at_x(&self, x: f32) -> u64 {
        if let Some(frames) = self.crossfade_frames(x) {
            return frames;
        }
        if self.clip_width <= 0.0 || self.duration_frames == 0 {
            return 0;
        }
        let progress = ((x - self.clip_x) / self.clip_width).clamp(0.0, 1.0);
        let progress = match self.edge {
            AudioClipFadeEdge::In => progress,
            AudioClipFadeEdge::Out => 1.0 - progress,
        };
        (progress as f64 * self.duration_frames as f64).round() as u64
    }
}

pub(super) fn fade_handle_xs(clip_x: f32, clip_width: f32, clip: &TimelineClip) -> (f32, f32) {
    if clip.duration == 0 {
        return (clip_x, clip_x + clip_width);
    }
    let pixels_per_frame = clip_width / clip.duration as f32;
    (
        clip_x + clip.fade_in_frames as f32 * pixels_per_frame,
        clip_x + clip_width - clip.fade_out_frames as f32 * pixels_per_frame,
    )
}

pub(super) fn fade_curve_handle(
    clip_x: f32,
    clip_width: f32,
    canvas_height: f32,
    clip: &TimelineClip,
    edge: AudioClipFadeEdge,
) -> Option<Point> {
    let (fade_in_x, fade_out_x) = fade_handle_xs(clip_x, clip_width, clip);
    let (start_x, end_x, curve, linked) = match edge {
        AudioClipFadeEdge::In => (
            clip_x,
            fade_in_x,
            clip.fade_in_curve,
            clip.crossfade_in_from.is_some(),
        ),
        AudioClipFadeEdge::Out => (
            fade_out_x,
            clip_x + clip_width,
            clip.fade_out_curve,
            clip.crossfade_out_to.is_some(),
        ),
    };
    if linked || (end_x - start_x).abs() <= f32::EPSILON {
        return None;
    }
    let top = CLIP_Y + CLIP_TITLE_HEIGHT + 2.0;
    let bottom = canvas_height - CLIP_Y - 2.0;
    Some(Point::new(
        (start_x + end_x) / 2.0,
        bottom - (bottom - top) * curve.gain(0.5),
    ))
}

impl TrackClipCanvas {
    fn crossfade_snap_for(&self, clip: &TimelineClip, edge: AudioClipFadeEdge) -> Option<FadeSnap> {
        let clip_end = clip.position.saturating_add(clip.duration);
        match edge {
            AudioClipFadeEdge::Out => self
                .clips
                .iter()
                .filter(|incoming| {
                    incoming.clip_id != clip.clip_id
                        && incoming.position > clip.position
                        && incoming.position < clip_end
                        && clip_end <= incoming.position.saturating_add(incoming.duration)
                })
                .max_by_key(|incoming| incoming.position)
                .map(|incoming| FadeSnap {
                    x: self.beat_to_x(incoming.position as f64 / self.spb()),
                    frames: clip_end - incoming.position,
                }),
            AudioClipFadeEdge::In => self
                .clips
                .iter()
                .filter(|outgoing| {
                    let outgoing_end = outgoing.position.saturating_add(outgoing.duration);
                    outgoing.clip_id != clip.clip_id
                        && outgoing.position < clip.position
                        && clip.position < outgoing_end
                        && outgoing_end <= clip_end
                })
                .min_by_key(|outgoing| outgoing.position.saturating_add(outgoing.duration))
                .map(|outgoing| {
                    let outgoing_end = outgoing.position.saturating_add(outgoing.duration);
                    FadeSnap {
                        x: self.beat_to_x(outgoing_end as f64 / self.spb()),
                        frames: outgoing_end - clip.position,
                    }
                }),
        }
    }

    pub(super) fn crossfade_curve_handles(
        &self,
        canvas_height: f32,
    ) -> impl Iterator<Item = (ClipId, ClipId, Point)> + '_ {
        let top = CLIP_Y + CLIP_TITLE_HEIGHT + 2.0;
        let bottom = canvas_height - CLIP_Y - 2.0;
        let spb = self.spb();
        self.crossfades.iter().map(move |crossfade| {
            let x = (self.beat_to_x(crossfade.overlap_start as f64 / spb)
                + self.beat_to_x(crossfade.overlap_end as f64 / spb))
                / 2.0;
            let incoming_gain = crossfade.curve.crossfade_gains(0.5).1;
            let y = bottom - (bottom - top) * incoming_gain;
            (
                crossfade.outgoing_id,
                crossfade.incoming_id,
                Point::new(x, y),
            )
        })
    }

    pub(super) fn crossfade_curve_hit(
        &self,
        pos: Point,
        canvas_height: f32,
    ) -> Option<CrossfadeCurveDrag> {
        self.crossfade_curve_handles(canvas_height)
            .into_iter()
            .find(|(outgoing_id, incoming_id, handle)| {
                (self.selected_clips.contains(outgoing_id)
                    || self.selected_clips.contains(incoming_id))
                    && (pos.x - handle.x).abs() <= FADE_HANDLE_HIT_RADIUS
                    && (pos.y - handle.y).abs() <= FADE_HANDLE_HIT_RADIUS
            })
            .map(|(outgoing_id, incoming_id, handle)| {
                CrossfadeCurveDrag::new(outgoing_id, incoming_id, canvas_height, handle)
            })
    }

    pub(super) fn fade_curve_hit(&self, pos: Point, canvas_height: f32) -> Option<FadeCurveDrag> {
        let spb = self.spb();
        for clip in &self.clips {
            if !self.selected_clips.contains(&clip.clip_id) {
                continue;
            }
            let clip_x = self.beat_to_x(clip.position as f64 / spb);
            let clip_width = self.geometry().width_for_beats(clip.duration as f64 / spb);
            for edge in [AudioClipFadeEdge::In, AudioClipFadeEdge::Out] {
                let Some(handle) = fade_curve_handle(clip_x, clip_width, canvas_height, clip, edge)
                else {
                    continue;
                };
                if (pos.x - handle.x).abs() <= FADE_HANDLE_HIT_RADIUS
                    && (pos.y - handle.y).abs() <= FADE_HANDLE_HIT_RADIUS
                {
                    return Some(FadeCurveDrag::new(
                        clip.clip_id,
                        edge,
                        canvas_height,
                        handle,
                    ));
                }
            }
        }
        None
    }

    pub(super) fn fade_handle_hit(&self, pos: Point) -> Option<FadeClipDrag> {
        if (pos.y - FADE_HANDLE_Y).abs() > FADE_HANDLE_HIT_RADIUS {
            return None;
        }
        let spb = self.spb();
        for clip in &self.clips {
            let start_beat = clip.position as f64 / spb;
            let clip_x = self.beat_to_x(start_beat);
            let clip_width = self.geometry().width_for_beats(clip.duration as f64 / spb);
            // An unselected Clip shows its handles under the pointer, when
            // wide enough to keep its resize and move zones.
            if !self.selected_clips.contains(&clip.clip_id)
                && clip_width < HOVER_HANDLES_MIN_CLIP_PX
            {
                continue;
            }
            if !self.selected_clips.contains(&clip.clip_id)
                && (pos.y - FADE_HANDLE_Y).abs() > HOVER_HANDLE_HIT_HEIGHT
            {
                continue;
            }
            if pos.x < clip_x || pos.x > clip_x + clip_width {
                continue;
            }
            let (fade_in_x, fade_out_x) = fade_handle_xs(clip_x, clip_width, clip);
            let in_distance = (pos.x - fade_in_x).abs();
            let out_distance = (pos.x - fade_out_x).abs();
            let edge = if in_distance <= FADE_HANDLE_HIT_RADIUS
                && (in_distance < out_distance
                    || (in_distance == out_distance && pos.x <= clip_x + clip_width / 2.0))
            {
                Some(AudioClipFadeEdge::In)
            } else if out_distance <= FADE_HANDLE_HIT_RADIUS {
                Some(AudioClipFadeEdge::Out)
            } else {
                None
            };
            if let Some(edge) = edge {
                let handle_x = match edge {
                    AudioClipFadeEdge::In => fade_in_x,
                    AudioClipFadeEdge::Out => fade_out_x,
                };
                return Some(FadeClipDrag::new(
                    clip.clip_id,
                    edge,
                    clip_x,
                    clip_width,
                    clip.duration,
                    Point::new(handle_x, FADE_HANDLE_Y),
                    self.crossfade_snap_for(clip, edge),
                ));
            }
        }
        None
    }

    pub(super) fn gain_handle_hit(&self, pos: Point) -> Option<GainDrag> {
        if (pos.y - GAIN_HANDLE_Y).abs() > GAIN_HANDLE_HIT_HEIGHT {
            return None;
        }
        let spb = self.spb();
        self.clips.iter().find_map(|clip| {
            let clip_x = self.beat_to_x(clip.position as f64 / spb);
            let clip_width = self.geometry().width_for_beats(clip.duration as f64 / spb);
            let handle = gain_handle(clip_x, clip_width)?;
            ((pos.x - handle.x).abs() <= GAIN_HANDLE_HALF_WIDTH + 2.0).then(|| GainDrag {
                undo_gesture: UndoGestureId::new(),
                clip_id: clip.clip_id,
                db: clip.gain_db,
                sent: clip.gain_db,
                last_y: pos.y,
                handle,
            })
        })
    }

    /// Resolve overlapping fade controls by their drawn distance. The
    /// fade-length handle wins an exact tie so a short steep fade can always
    /// be made longer again.
    pub(super) fn fade_control_hit(
        &self,
        pos: Point,
        canvas_height: f32,
    ) -> Option<FadeControlDrag> {
        let candidates = [
            self.fade_handle_hit(pos).map(FadeControlDrag::Length),
            self.fade_curve_hit(pos, canvas_height)
                .map(FadeControlDrag::Curve),
            self.crossfade_curve_hit(pos, canvas_height)
                .map(FadeControlDrag::CrossfadeCurve),
            self.gain_handle_hit(pos).map(FadeControlDrag::Gain),
        ];
        candidates
            .into_iter()
            .flatten()
            .fold(None, |nearest, candidate| {
                if nearest.as_ref().is_none_or(|current: &FadeControlDrag| {
                    candidate.distance_squared(pos) < current.distance_squared(pos)
                }) {
                    Some(candidate)
                } else {
                    nearest
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gain_drag_sends_tenths_up_to_the_clip_range() {
        let mut drag = GainDrag {
            undo_gesture: UndoGestureId::new(),
            clip_id: ClipId::new(),
            db: 0.0,
            sent: 0.0,
            last_y: 50.0,
            handle: Point::ORIGIN,
        };
        assert_eq!(drag.drag_to(46.0, false), Some(1.0));
        assert_eq!(drag.drag_to(46.0, false), None);
        assert_eq!(drag.drag_to(41.0, true), Some(1.3));
        assert_eq!(drag.drag_to(-1_000.0, false), Some(ClipGainDb::MAX));
    }

    #[test]
    fn a_narrow_clip_has_no_gain_handle() {
        assert_eq!(gain_handle(0.0, 40.0), None);
        assert_eq!(
            gain_handle(10.0, 100.0),
            Some(Point::new(60.0, GAIN_HANDLE_Y))
        );
    }

    #[test]
    fn drag_math_is_reciprocal_with_drawn_handle_positions() {
        let clip = TimelineClip {
            color: None,
            clip_id: ClipId::new(),
            position: 0,
            duration: 48_000,
            name: String::new(),
            peaks: Default::default(),
            peak_span_frames: None,
            loop_enabled: false,
            loop_start: 0,
            loop_end: 48_000,
            gain_db: 0.0,
            fade_in_frames: 12_345,
            fade_out_frames: 6_789,
            fade_in_curve: Default::default(),
            fade_out_curve: Default::default(),
            crossfade_in_from: None,
            crossfade_out_to: None,
            warp_stale: false,
        };
        let (fade_in_x, fade_out_x) = fade_handle_xs(37.0, 481.0, &clip);

        let fade_in = FadeClipDrag::new(
            clip.clip_id,
            AudioClipFadeEdge::In,
            37.0,
            481.0,
            clip.duration,
            Point::new(fade_in_x, FADE_HANDLE_Y),
            None,
        );
        let fade_out = FadeClipDrag::new(
            clip.clip_id,
            AudioClipFadeEdge::Out,
            37.0,
            481.0,
            clip.duration,
            Point::new(fade_out_x, FADE_HANDLE_Y),
            None,
        );

        assert_eq!(fade_in.frames_at_x(fade_in_x), clip.fade_in_frames);
        assert_eq!(fade_out.frames_at_x(fade_out_x), clip.fade_out_frames);
    }

    #[test]
    fn fade_drag_magnetically_reaches_a_crossfade_edge() {
        let drag = FadeClipDrag::new(
            ClipId::new(),
            AudioClipFadeEdge::Out,
            0.0,
            400.0,
            1_000,
            Point::new(400.0, FADE_HANDLE_Y),
            Some(FadeSnap {
                x: 300.0,
                frames: 250,
            }),
        );

        assert_eq!(drag.frames_at_x(300.0), 250);
        assert_eq!(drag.frames_at_x(300.0 + FADE_HANDLE_HIT_RADIUS), 250);
        assert_ne!(drag.frames_at_x(301.0 + FADE_HANDLE_HIT_RADIUS), 250);
    }
}
