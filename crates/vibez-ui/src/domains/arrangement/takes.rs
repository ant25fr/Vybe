//! Take lanes and comping, as in Ableton Live: every recording keeps its
//! take on a lane under the track, and the track itself plays the comp,
//! made of the parts chosen from those lanes.

use std::sync::Arc;

use vibez_core::id::{ClipId, TrackId};
use vibez_core::track::ClipFades;
use vibez_engine::commands::EngineCommand;

use super::fragment_geometry::audio_fragment;
use super::{ArrangementAction, ArrangementCtx, EngineHandle};
use crate::state::{ArrangementSelection, TakeAudition, TakeLane, TimelineEditorState, UiClip};

/// Fade put on each edge of a comped part, so a join does not click.
const COMP_FADE_SECONDS: f64 = 0.004;

/// The stretches of the timeline one recording of `frames` covered: one per
/// pass when it looped over `loop_region`. Each is `(position, offset into
/// the recording, length)`.
pub(crate) fn recording_passes(
    start: u64,
    frames: u64,
    loop_region: Option<(u64, u64)>,
    shortest: u64,
) -> Vec<(u64, u64, u64)> {
    let Some((loop_start, loop_end)) = loop_region.filter(|&(from, to)| to > from && start < to)
    else {
        return vec![(start, 0, frames)];
    };
    let mut passes = Vec::new();
    let (mut position, mut offset) = (start, 0);
    while offset < frames {
        let length = (loop_end - position).min(frames - offset);
        // What is recorded in the instant before Stop is not a take.
        if length >= shortest || passes.is_empty() {
            passes.push((position, offset, length));
        }
        offset += length;
        position = loop_start;
    }
    passes
}

/// Whether `clip` plays the same audio as `take` at the same time.
fn plays_from(clip: &UiClip, take: &UiClip) -> bool {
    Arc::ptr_eq(&clip.audio, &take.audio)
        && clip.position as i128 - clip.source_offset as i128
            == take.position as i128 - take.source_offset as i128
}

fn overlap(clip: &UiClip, start: u64, end: u64) -> Option<(u64, u64)> {
    let from = clip.position.max(start);
    let to = clip.position.saturating_add(clip.duration).min(end);
    (to > from).then_some((from, to))
}

impl TimelineEditorState {
    fn take_lanes(&self, track_id: TrackId) -> &[TakeLane] {
        self.timeline
            .get(track_id)
            .map_or(&[], |content| content.take_lanes.as_slice())
    }

    /// Adds a lane holding `clips` under `track_id` and returns its number.
    pub(crate) fn add_take_lane(&mut self, track_id: TrackId, clips: Vec<UiClip>) -> u32 {
        let content = Arc::make_mut(&mut self.timeline).ensure(track_id);
        let id = 1 + content
            .take_lanes
            .iter()
            .map(|lane| lane.id)
            .max()
            .unwrap_or(0);
        content.take_lanes.push(TakeLane {
            id,
            name: format!("Take {id}"),
            clips,
        });
        id
    }

    pub(super) fn toggle_take_lanes(
        &mut self,
        engine: &mut impl EngineHandle,
        track_id: TrackId,
    ) -> ArrangementAction {
        // A take out of sight is not left playing.
        if self.open_take_lanes.contains(&track_id)
            && self
                .auditioned_take
                .as_ref()
                .is_some_and(|heard| heard.track_id == track_id)
        {
            self.stop_take_audition(engine);
        }
        if !self.open_take_lanes.remove(&track_id) {
            self.open_take_lanes.insert(track_id);
        }
        ArrangementAction::default()
    }

    /// Stops listening to a take: the track plays its comp again.
    pub(crate) fn stop_take_audition(&mut self, engine: &mut impl EngineHandle) {
        if let Some(audition) = self.auditioned_take.take() {
            for clip_id in audition.clips {
                engine.send(EngineCommand::RemoveClip(audition.track_id, clip_id));
            }
        }
    }

    /// Listens to one take in place of its track, or stops when it is the
    /// one already heard. Only one take is heard at a time.
    pub(super) fn op_audition_take(
        &mut self,
        engine: &mut impl EngineHandle,
        track_id: TrackId,
        lane: u32,
    ) -> ArrangementAction {
        let heard = self
            .auditioned_take
            .as_ref()
            .is_some_and(|heard| heard.track_id == track_id && heard.lane == lane);
        self.stop_take_audition(engine);
        if heard {
            return ArrangementAction::default();
        }
        let takes: Vec<UiClip> = self
            .take_lanes(track_id)
            .iter()
            .find(|candidate| candidate.id == lane)
            .map(|lane| lane.clips.iter().map(take_copy).collect())
            .unwrap_or_default();
        if takes.is_empty() {
            return ArrangementAction::default();
        }
        let mut clips = Vec::with_capacity(takes.len());
        for take in takes {
            engine.send(add_clip_command(
                track_id,
                &take,
                vibez_engine::clip_insertion::PreparedClipInsertion::new().for_audition(),
            ));
            clips.push(take.id);
        }
        self.auditioned_take = Some(TakeAudition {
            track_id,
            lane,
            clips,
        });
        ArrangementAction::default()
    }

    /// Puts what lane `lane` holds between `start` and `end` onto the track,
    /// over whatever the comp played there. Returns the new Clips.
    fn comp_range(
        &mut self,
        engine: &mut impl EngineHandle,
        samples_per_beat: f64,
        track_id: TrackId,
        lane: u32,
        (start, end): (u64, u64),
    ) -> Vec<ClipId> {
        if samples_per_beat <= 0.0 {
            return Vec::new();
        }
        let fade = (COMP_FADE_SECONDS * f64::from(engine.sample_rate())) as u64;
        let parts: Vec<UiClip> = self
            .take_lanes(track_id)
            .iter()
            .find(|candidate| candidate.id == lane)
            .into_iter()
            .flat_map(|lane| &lane.clips)
            .filter_map(|take| {
                let (from, to) = overlap(take, start, end)?;
                let mut part =
                    audio_fragment(take, take.name.clone(), from - take.position, to - from);
                part.fades = ClipFades::new(fade, fade, part.duration);
                Some(part)
            })
            .collect();
        let mut placed = Vec::with_capacity(parts.len());
        for part in parts {
            let beats = |samples: u64| samples as f64 / samples_per_beat;
            let range = (beats(part.position), beats(part.position + part.duration));
            self.overwrite_ranges(
                engine,
                vec![(track_id, range, Default::default())],
                samples_per_beat,
            );
            engine.send(add_clip_command(
                track_id,
                &part,
                vibez_engine::clip_insertion::PreparedClipInsertion::new(),
            ));
            placed.push(part.id);
            Arc::make_mut(&mut self.timeline)
                .ensure(track_id)
                .clips
                .push(part);
        }
        placed
    }

    fn select_comp(&mut self, track_id: TrackId, clips: &[ClipId]) {
        self.selected_clips = clips
            .iter()
            .map(|&clip_id| ArrangementSelection::AudioClip { track_id, clip_id })
            .collect();
        self.selected_track = Some(track_id);
        self.clear_time_selection();
    }

    pub(super) fn op_comp_from_take(
        &mut self,
        engine: &mut impl EngineHandle,
        ctx: ArrangementCtx,
        track_id: TrackId,
        lane: u32,
        start: u64,
        end: u64,
    ) -> ArrangementAction {
        let placed = self.comp_range(engine, ctx.samples_per_beat, track_id, lane, (start, end));
        if placed.is_empty() {
            return ArrangementAction {
                status: Some("That take holds nothing there".into()),
                ..Default::default()
            };
        }
        self.select_comp(track_id, &placed);
        ArrangementAction {
            mark_dirty: true,
            status: Some("Added the take to the comp".into()),
            ..Default::default()
        }
    }

    /// Each selected comp Clip takes the next (or previous) take's audio for
    /// the same stretch of time.
    pub(super) fn op_cycle_take(
        &mut self,
        engine: &mut impl EngineHandle,
        ctx: ArrangementCtx,
        forward: bool,
    ) -> ArrangementAction {
        let selected: Vec<_> = self
            .selected_clips
            .iter()
            .filter_map(|selection| match selection {
                ArrangementSelection::AudioClip { track_id, clip_id } => {
                    Some((*track_id, *clip_id))
                }
                ArrangementSelection::NoteClip { .. } => None,
            })
            .collect();
        let mut placed = Vec::new();
        for (track_id, clip_id) in selected {
            let Some(clip) = self
                .timeline
                .get(track_id)
                .and_then(|content| content.clips.iter().find(|clip| clip.id == clip_id))
            else {
                continue;
            };
            let range = (clip.position, clip.position.saturating_add(clip.duration));
            let covering: Vec<_> = self
                .take_lanes(track_id)
                .iter()
                .filter(|lane| {
                    lane.clips
                        .iter()
                        .any(|take| overlap(take, range.0, range.1).is_some())
                })
                .collect();
            if covering.is_empty() {
                continue;
            }
            let current = covering
                .iter()
                .position(|lane| lane.clips.iter().any(|take| plays_from(clip, take)));
            let count = covering.len();
            let next = match current {
                // Only one take covers it and it is already playing.
                Some(_) if count == 1 => continue,
                Some(at) if forward => (at + 1) % count,
                Some(at) => (at + count - 1) % count,
                None if forward => 0,
                None => count - 1,
            };
            let lane = covering[next].id;
            placed.extend(
                self.comp_range(engine, ctx.samples_per_beat, track_id, lane, range)
                    .into_iter()
                    .map(|id| (track_id, id)),
            );
        }
        if placed.is_empty() {
            return ArrangementAction {
                status: Some("Select a Clip that another take covers".into()),
                ..Default::default()
            };
        }
        self.selected_clips = placed
            .into_iter()
            .map(|(track_id, clip_id)| ArrangementSelection::AudioClip { track_id, clip_id })
            .collect();
        ArrangementAction {
            mark_dirty: true,
            ..Default::default()
        }
    }

    pub(super) fn op_delete_take_lane(
        &mut self,
        engine: &mut impl EngineHandle,
        track_id: TrackId,
        lane: u32,
    ) -> ArrangementAction {
        if !self.take_lanes(track_id).iter().any(|l| l.id == lane) {
            return ArrangementAction::default();
        }
        if self
            .auditioned_take
            .as_ref()
            .is_some_and(|heard| heard.track_id == track_id && heard.lane == lane)
        {
            self.stop_take_audition(engine);
        }
        // The comp keeps what it already took from the lane.
        if let Some(content) = Arc::make_mut(&mut self.timeline).get_mut(track_id) {
            content.take_lanes.retain(|l| l.id != lane);
            if content.take_lanes.is_empty() {
                self.open_take_lanes.remove(&track_id);
            }
        }
        ArrangementAction {
            mark_dirty: true,
            status: Some("Deleted the take".into()),
            ..Default::default()
        }
    }

    /// Copies the track's selected Clips to a new take lane, to comp from
    /// material that was imported rather than recorded.
    pub(super) fn op_selection_to_take_lane(&mut self, track_id: TrackId) -> ArrangementAction {
        let copies: Vec<UiClip> = self
            .timeline
            .get(track_id)
            .into_iter()
            .flat_map(|content| &content.clips)
            .filter(|clip| {
                self.selected_clips
                    .contains(&ArrangementSelection::AudioClip {
                        track_id,
                        clip_id: clip.id,
                    })
            })
            .map(take_copy)
            .collect();
        if copies.is_empty() {
            return ArrangementAction {
                status: Some("Select Audio Clips on this track first".into()),
                ..Default::default()
            };
        }
        self.add_take_lane(track_id, copies);
        self.open_take_lanes.insert(track_id);
        ArrangementAction {
            mark_dirty: true,
            status: Some("Kept the selection as a take".into()),
            ..Default::default()
        }
    }

    /// Lands a finished recording: each pass becomes a take, and the comp
    /// plays the latest one recorded at every point. Clips the recording
    /// covers that no take holds are kept as a take first. Returns the comp
    /// Clips now selected.
    pub(crate) fn land_recording(
        &mut self,
        engine: &mut impl EngineHandle,
        samples_per_beat: f64,
        track_id: TrackId,
        recording: &UiClip,
        passes: &[(u64, u64, u64)],
    ) -> Vec<ClipId> {
        let covered = |clip: &UiClip| {
            passes
                .iter()
                .any(|&(from, _, length)| overlap(clip, from, from + length).is_some())
        };
        let kept: Vec<UiClip> = self
            .timeline
            .get(track_id)
            .into_iter()
            .flat_map(|content| &content.clips)
            .filter(|clip| {
                covered(clip)
                    && !self
                        .take_lanes(track_id)
                        .iter()
                        .flat_map(|lane| &lane.clips)
                        .any(|take| plays_from(clip, take))
            })
            .map(take_copy)
            .collect();
        if !kept.is_empty() {
            self.add_take_lane(track_id, kept);
        }
        for (number, &(position, offset, length)) in passes.iter().enumerate() {
            let mut take = if passes.len() == 1 {
                recording.clone()
            } else {
                let mut take = audio_fragment(
                    recording,
                    format!("{} · pass {}", recording.name, number + 1),
                    offset,
                    length,
                );
                take.position = position;
                take
            };
            take.id = ClipId::new();
            let lane = self.add_take_lane(track_id, vec![take]);
            self.comp_range(
                engine,
                samples_per_beat,
                track_id,
                lane,
                (position, position + length),
            );
        }
        if passes.len() > 1 {
            self.open_take_lanes.insert(track_id);
        }
        // Later passes may have replaced earlier parts of the comp.
        let comp: Vec<ClipId> = self
            .timeline
            .get(track_id)
            .into_iter()
            .flat_map(|content| &content.clips)
            .filter(|clip| Arc::ptr_eq(&clip.audio, &recording.audio))
            .map(|clip| clip.id)
            .collect();
        self.select_comp(track_id, &comp);
        comp
    }
}

fn add_clip_command(
    track_id: TrackId,
    clip: &UiClip,
    insertion: Box<vibez_engine::clip_insertion::PreparedClipInsertion>,
) -> EngineCommand {
    EngineCommand::AddClip {
        insertion,
        track_id,
        clip_id: clip.id,
        audio: Arc::clone(&clip.audio),
        position: clip.position,
        source_offset: clip.source_offset,
        start_marker: clip.start_marker,
        duration: clip.duration,
        loop_enabled: clip.loop_enabled,
        loop_start: clip.loop_start,
        loop_end: clip.loop_end,
        linear_gain: clip.gain_db.linear(),
        fades: clip.fades,
        playback_direction: clip.playback_direction,
        warp_markers: clip.warp_markers.clone(),
    }
}

/// A Clip as a take holds it: its own identity, tied to no neighbour.
fn take_copy(clip: &UiClip) -> UiClip {
    let mut copy = clip.clone();
    copy.id = ClipId::new();
    copy.fades = copy.fades.unlinked();
    copy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::test_support::RecordingEngine;

    /// One beat is 1 000 samples in these tests.
    const SPB: f64 = 1_000.0;

    fn comp(editor: &TimelineEditorState, track: TrackId) -> Vec<(u64, u64, u64)> {
        let mut parts: Vec<_> = editor
            .timeline
            .get(track)
            .unwrap()
            .clips
            .iter()
            .map(|clip| (clip.position, clip.source_offset, clip.duration))
            .collect();
        parts.sort_unstable();
        parts
    }

    #[test]
    fn a_looped_recording_splits_into_its_passes() {
        assert_eq!(
            recording_passes(2_000, 14_000, Some((0, 8_000)), 100),
            [(2_000, 0, 6_000), (0, 6_000, 8_000)]
        );
        // Stop's tail is not a take; a recording past the loop never loops.
        assert_eq!(
            recording_passes(0, 8_050, Some((0, 8_000)), 100),
            [(0, 0, 8_000)]
        );
        assert_eq!(
            recording_passes(9_000, 500, Some((0, 8_000)), 100),
            [(9_000, 0, 500)]
        );
        assert_eq!(recording_passes(10, 50, None, 100), [(10, 0, 50)]);
    }

    #[test]
    fn the_latest_pass_plays_and_earlier_ones_can_be_comped_back() {
        let track = TrackId::new();
        let mut editor = TimelineEditorState::default();
        let mut engine = RecordingEngine::default();
        let recording = UiClip::test_clip(0, 20_000);
        let passes = recording_passes(0, 20_000, Some((0, 8_000)), 100);

        editor.land_recording(&mut engine, SPB, track, &recording, &passes);

        // Three passes; the unfinished third covers the start of the second.
        assert_eq!(editor.timeline.get(track).unwrap().take_lanes.len(), 3);
        assert_eq!(
            comp(&editor, track),
            [(0, 16_000, 4_000), (4_000, 12_000, 4_000)]
        );
        assert!(editor.open_take_lanes.contains(&track));

        let ctx = ArrangementCtx {
            samples_per_beat: SPB,
            ..Default::default()
        };
        editor.op_comp_from_take(&mut engine, ctx, track, 1, 2_000, 6_000);
        assert_eq!(
            comp(&editor, track),
            [
                (0, 16_000, 2_000),
                (2_000, 2_000, 4_000),
                (6_000, 14_000, 2_000)
            ]
        );

        // The middle Clip moves on to the next take of the same stretch.
        let middle = editor
            .timeline
            .get(track)
            .unwrap()
            .clips
            .iter()
            .find(|clip| clip.position == 2_000)
            .unwrap()
            .id;
        editor.selected_clips = [ArrangementSelection::AudioClip {
            track_id: track,
            clip_id: middle,
        }]
        .into_iter()
        .collect();
        editor.op_cycle_take(&mut engine, ctx, true);
        assert!(comp(&editor, track).contains(&(2_000, 10_000, 4_000)));

        // Deleting a take leaves the comp as it was.
        let before = comp(&editor, track);
        editor.op_delete_take_lane(&mut engine, track, 2);
        assert_eq!(comp(&editor, track), before);
        assert_eq!(editor.timeline.get(track).unwrap().take_lanes.len(), 2);
    }

    #[test]
    fn one_take_is_heard_at_a_time_and_stops_when_asked_again() {
        let track = TrackId::new();
        let mut editor = TimelineEditorState::default();
        let mut engine = RecordingEngine::default();
        let first = editor.add_take_lane(track, vec![UiClip::test_clip(0, 64)]);
        let second = editor.add_take_lane(track, vec![UiClip::test_clip(0, 64)]);

        editor.op_audition_take(&mut engine, track, first);
        let heard = editor.auditioned_take.clone().unwrap();
        editor.op_audition_take(&mut engine, track, second);
        assert_eq!(editor.auditioned_take.as_ref().unwrap().lane, second);
        assert!(engine.0.iter().any(|command| matches!(
            command,
            EngineCommand::RemoveClip(_, id) if *id == heard.clips[0]
        )));

        editor.op_audition_take(&mut engine, track, second);
        assert_eq!(editor.auditioned_take, None);
        // The takes themselves are untouched.
        assert_eq!(editor.timeline.get(track).unwrap().take_lanes.len(), 2);
        assert!(editor.timeline.get(track).unwrap().clips.is_empty());
    }

    #[test]
    fn recording_over_a_clip_keeps_it_as_a_take() {
        let track = TrackId::new();
        let mut editor = TimelineEditorState::default();
        let mut engine = RecordingEngine::default();
        let old = UiClip::test_clip(0, 8_000);
        Arc::make_mut(&mut editor.timeline)
            .ensure(track)
            .clips
            .push(old);
        let recording = UiClip::test_clip(2_000, 2_000);

        editor.land_recording(&mut engine, SPB, track, &recording, &[(2_000, 0, 2_000)]);

        let content = editor.timeline.get(track).unwrap();
        assert_eq!(content.take_lanes.len(), 2);
        assert_eq!(content.take_lanes[0].clips[0].duration, 8_000);
        assert_eq!(
            comp(&editor, track),
            [(0, 0, 2_000), (2_000, 0, 2_000), (4_000, 4_000, 4_000)]
        );
    }
}
