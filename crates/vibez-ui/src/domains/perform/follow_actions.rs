//! Which playing Clips reach their follow action, and what each one launches.

use vibez_core::follow_action::{FollowKind, FollowTime};
use vibez_core::id::{ClipId, TrackId};
use vibez_core::perform::MusicalBoundary;

use super::{ClipLaunchRequest, ClipStore, PerformState};

/// A follow action to launch on the next `boundary`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DueFollow {
    pub request: ClipLaunchRequest,
    pub boundary: MusicalBoundary,
}

impl ClipStore {
    /// The Clips in the filled slots touching `id`'s in its column, top first.
    pub fn follow_group(&self, id: ClipId) -> Vec<ClipId> {
        let Some(clip) = self.by_id(id) else {
            return Vec::new();
        };
        let at = |row: u32| self.at(clip.track_id, row).map(|other| other.id);
        let mut first = clip.row;
        while first > 0 && at(first - 1).is_some() {
            first -= 1;
        }
        (first..).map_while(at).collect()
    }
}

impl PerformState {
    /// Follow actions due within the last grid step before their time, at
    /// Perform `position` in samples. Each is returned once; a launch the
    /// performer queued on the Track outranks it.
    pub fn due_follow_actions(&mut self, position: u64, samples_per_beat: f64) -> Vec<DueFollow> {
        let mut due = Vec::new();
        if !self.clip_editor.running || samples_per_beat <= 0.0 {
            return due;
        }
        let recording = self
            .clip_record
            .session
            .as_ref()
            .map(|session| session.working.track_id);
        let mut tracks: Vec<TrackId> = self.clip_editor.playing.keys().copied().collect();
        tracks.sort_by_key(|track| track.raw());
        for track in tracks {
            if recording == Some(track) || self.clip_editor.queued.contains_key(&track) {
                continue;
            }
            let Some(playing) = self.clip_editor.playing.get(&track) else {
                continue;
            };
            let id = playing.id;
            // The stored Clip holds the current settings.
            let Some(clip) = self.clips.by_id(id) else {
                continue;
            };
            let Some(follow) = clip.launch.follow.filter(|follow| follow.is_active()) else {
                continue;
            };
            let Some(&started) = self.clip_editor.started_at.get(&track) else {
                continue;
            };
            let clip_beats = clip.length_and_loop(samples_per_beat).0 as f64 / samples_per_beat;
            let beats = follow.time.beats(clip_beats);
            let period = (beats * samples_per_beat).round().max(1.0) as u64;
            let boundary = FollowTime::boundary(beats);
            let step = boundary.beats().unwrap_or(0.25);
            let lead = (step.min(beats) * samples_per_beat * 0.75) as u64;
            let elapsed = position.saturating_sub(started);
            // The end of the period `lead` samples ahead falls in.
            let upcoming = (elapsed + lead) / period;
            let fired = self.clip_editor.followed.get(&track).copied();
            if upcoming == 0 || fired == Some((id, started, upcoming)) {
                continue;
            }
            self.clip_editor
                .followed
                .insert(track, (id, started, upcoming));
            self.clip_editor.follow_draws = self.clip_editor.follow_draws.wrapping_add(1);
            let random = scramble(
                self.clip_editor
                    .follow_draws
                    .wrapping_add(started)
                    .wrapping_add(id.raw() << 32),
            );
            let kind = follow.choose(random);
            let request = match kind {
                FollowKind::None => continue,
                FollowKind::Stop => ClipLaunchRequest::Stop(track),
                kind => {
                    let group = self.clips.follow_group(id);
                    let Some(next) = group
                        .iter()
                        .position(|other| *other == id)
                        .and_then(|current| kind.pick(current, group.len(), random >> 8))
                    else {
                        continue;
                    };
                    ClipLaunchRequest::Clip(group[next])
                }
            };
            due.push(DueFollow { request, boundary });
        }
        due
    }
}

/// Spreads successive draws over the whole range.
fn scramble(seed: u64) -> u64 {
    let mut state = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    state = (state ^ (state >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    state = (state ^ (state >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    state ^ (state >> 31)
}

#[cfg(test)]
#[path = "follow_actions_tests.rs"]
mod tests;
