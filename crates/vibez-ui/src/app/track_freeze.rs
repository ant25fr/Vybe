//! Freeze and bounce in place: a track's clips and devices rendered to
//! audio before its fader. A frozen track plays that audio itself, its
//! clips and devices set aside until it is unfrozen; bouncing replaces the
//! track with an audio track holding it.

use vibez_core::automation::AutomationTarget;
use vibez_core::id::{ClipId, TrackId};
use vibez_core::track::TrackInfo;
use vibez_engine::render::{BounceMode, BounceRequest};

use crate::domains::arrangement::ArrangementMsg;
use crate::message::BounceOutcome;
use crate::state::{ArrangementSelection, UiClip};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TrackRender {
    pub source: TrackId,
    /// Replace the track with the audio instead of freezing it.
    pub replace: bool,
}

/// Mixer controls stay live on the rendered audio, so they are left out of
/// the render.
fn is_mixer_target(target: &AutomationTarget) -> bool {
    matches!(
        target,
        AutomationTarget::TrackGain
            | AutomationTarget::TrackPan
            | AutomationTarget::TrackMute
            | AutomationTarget::Send { .. }
    )
}

/// Makes the render of `track` its signal ahead of the fader and pan.
fn take_before_the_fader(request: &mut BounceRequest, track: TrackId) {
    let Some(info): Option<&mut TrackInfo> =
        request.tracks.iter_mut().find(|info| info.id == track)
    else {
        return;
    };
    // The centre pan law passes 1/sqrt(2) a side; this makes it unity.
    info.gain = std::f32::consts::SQRT_2;
    info.pan = 0.5;
    info.mute = false;
    info.sends.clear();
    info.output = None;
    info.automation
        .retain(|lane| !is_mixer_target(&lane.target));
    for modulator in &mut info.modulators {
        modulator
            .links
            .retain(|link| !is_mixer_target(&link.target));
    }
}

impl App {
    pub(super) fn toggle_track_freeze(&mut self, track_id: TrackId) -> Task<Message> {
        let Some(track) = self
            .state
            .project_tracks
            .tracks
            .iter()
            .find(|track| track.id == track_id)
        else {
            self.state.status_text = "Only tracks can be frozen".into();
            return Task::none();
        };
        match track.frozen_clip {
            Some(clip) => {
                self.unfreeze_track(track_id, clip);
                Task::none()
            }
            None => self.render_track_in_place(track_id, false),
        }
    }

    pub(super) fn render_track_in_place(
        &mut self,
        track_id: TrackId,
        replace: bool,
    ) -> Task<Message> {
        if self.export_job.is_some() || self.export_render_progress.is_some() {
            self.state.status_text = "A render is already in progress".into();
            return Task::none();
        }
        let Some(track) = self
            .state
            .project_tracks
            .tracks
            .iter()
            .find(|track| track.id == track_id)
        else {
            self.state.status_text = "Only tracks can be bounced in place".into();
            return Task::none();
        };
        if track.frozen_clip.is_some() {
            self.state.status_text = "Unfreeze the track first".into();
            return Task::none();
        }
        // The render would be placed back on the timeline and re-pitched a
        // second time.
        if self.state.tempo_lane().is_some() {
            self.state.status_text =
                "Remove the tempo automation to freeze or bounce a track".into();
            return Task::none();
        }
        let name = track.name.clone();
        self.stop_take_audition();
        let total = self.state.total_duration_samples();
        let empty = self
            .state
            .arrange_content(track_id)
            .is_none_or(|content| content.clips.is_empty() && content.note_clips.is_empty());
        if total == 0 || empty {
            self.state.status_text = format!("'{name}' has no clips to render");
            return Task::none();
        }
        let mut request = self.offline_render_request(BounceMode::Track(track_id), (0, total));
        take_before_the_fader(&mut request, track_id);
        let path = self.next_bounce_path();
        let task = self.start_offline_render(request, path, Some((name, 0)));
        if self.export_job.is_some() {
            self.pending_track_render = Some(TrackRender {
                source: track_id,
                replace,
            });
        }
        task
    }

    /// Puts the rendered audio on the track and freezes the track onto it.
    pub(super) fn finish_freeze(&mut self, track_id: TrackId, outcome: BounceOutcome) {
        if !self
            .state
            .project_tracks
            .tracks
            .iter()
            .any(|track| track.id == track_id)
        {
            self.state.status_text = "The track to freeze is gone".into();
            return;
        }
        let clip_id = ClipId::new();
        let duration = outcome.audio.num_frames() as u64;
        self.send_command(EngineCommand::AddClip {
            insertion: vibez_engine::clip_insertion::PreparedClipInsertion::new(),
            track_id,
            clip_id,
            audio: Arc::clone(&outcome.audio),
            position: 0,
            source_offset: 0,
            start_marker: 0,
            duration,
            loop_enabled: false,
            loop_start: 0,
            loop_end: 0,
            linear_gain: 1.0,
            fades: Default::default(),
            playback_direction: Default::default(),
            warp_markers: Default::default(),
        });
        self.state.arrange_content_mut(track_id).clips.push(UiClip {
            color_index: None,
            id: clip_id,
            name: format!("{} (frozen)", outcome.clip_name),
            audio: outcome.audio,
            source: outcome.source,
            position: 0,
            source_offset: 0,
            start_marker: 0,
            duration,
            loop_enabled: false,
            loop_start: 0,
            loop_end: 0,
            gain_db: Default::default(),
            fades: Default::default(),
            playback_direction: Default::default(),
            transient_markers: Default::default(),
            warp_markers: Default::default(),
            transpose: Default::default(),
            original_bpm: None,
            warped: false,
            warped_to_bpm: None,
            original_audio: None,
        });
        // The clips set aside cannot be edited while hidden.
        self.state.arrangement.selected_clips.retain(|selected| {
            !matches!(
                selected,
                ArrangementSelection::AudioClip { track_id: of, .. }
                    | ArrangementSelection::NoteClip { track_id: of, .. }
                    if *of == track_id
            )
        });
        if let Some(track) = Arc::make_mut(&mut self.state.project_tracks)
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
        {
            track.frozen_clip = Some(clip_id);
        }
        self.send_command(EngineCommand::SetTrackFrozen(track_id, Some(clip_id)));
        self.mark_project_dirty();
        self.state.status_text = format!("Froze '{}'", outcome.clip_name);
    }

    fn unfreeze_track(&mut self, track_id: TrackId, clip_id: ClipId) {
        let snapshot = self.state.project_snapshot();
        self.state.project.history.push_edit(snapshot, None);
        self.send_command(EngineCommand::SetTrackFrozen(track_id, None));
        self.send_command(EngineCommand::RemoveClip(track_id, clip_id));
        self.state
            .arrange_content_mut(track_id)
            .clips
            .retain(|clip| clip.id != clip_id);
        self.state
            .arrangement
            .selected_clips
            .remove(&ArrangementSelection::AudioClip { track_id, clip_id });
        if let Some(track) = Arc::make_mut(&mut self.state.project_tracks)
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
        {
            track.frozen_clip = None;
        }
        self.mark_project_dirty();
        self.state.status_text = "Unfrozen".into();
    }

    /// Replaces the rendered track's source with `audio_track`, which takes
    /// its place and mixer.
    pub(super) fn finish_track_render(&mut self, render: TrackRender, audio_track: TrackId) {
        let tracks = &self.state.project_tracks.tracks;
        let Some(source) = tracks.iter().find(|t| t.id == render.source).cloned() else {
            return;
        };
        let lanes: Vec<_> = self
            .state
            .arrange_content(source.id)
            .map(|content| {
                content
                    .automation
                    .iter()
                    .filter(|lane| is_mixer_target(&lane.target))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let project = Arc::make_mut(&mut self.state.project_tracks);
        let Some(from) = project.tracks.iter().position(|t| t.id == audio_track) else {
            return;
        };
        let mut audio = project.tracks.remove(from);
        audio.name = source.name.clone();
        audio.color_index = source.color_index;
        audio.gain = source.gain;
        audio.pan = source.pan;
        audio.mute = source.mute;
        audio.solo = source.solo;
        audio.sends = source.sends.clone();
        audio.pre_fader_sends = source.pre_fader_sends.clone();
        audio.output = source.output;
        // The render left the mixer out, so what moves it comes along.
        let mixer_modulators: Vec<_> = source
            .modulators
            .iter()
            .map(|modulator| {
                let mut kept = modulator.clone();
                kept.links.retain(|link| is_mixer_target(&link.target));
                kept
            })
            .filter(|modulator| !modulator.links.is_empty())
            .collect();
        crate::domains::modulation::sync_modulators(
            &mut crate::domains::EngineTx(&mut self.cmd_tx),
            audio_track,
            &audio.modulators,
            &mixer_modulators,
        );
        audio.modulators = mixer_modulators;
        let at = project
            .tracks
            .iter()
            .position(|t| t.id == source.id)
            .map_or(project.tracks.len(), |index| index + 1);
        project.tracks.insert(at, audio);
        let order = project.tracks.iter().map(|t| t.id).collect();

        self.send_command(EngineCommand::ReorderTracks(order));
        self.send_command(EngineCommand::SetTrackGain(audio_track, source.gain));
        self.send_command(EngineCommand::SetTrackPan(audio_track, source.pan));
        self.send_command(EngineCommand::SetTrackMute(audio_track, source.mute));
        self.send_command(EngineCommand::SetTrackSolo(audio_track, source.solo));
        for bus_id in source.pre_fader_sends {
            self.send_command(EngineCommand::SetSendPreFader {
                track_id: audio_track,
                bus_id,
                pre_fader: true,
            });
        }
        for (bus_id, amount) in source.sends {
            self.send_command(EngineCommand::SetSend {
                track_id: audio_track,
                bus_id,
                amount,
            });
        }
        for lane in lanes {
            self.send_command(EngineCommand::SetAutomationLane {
                track_id: audio_track,
                insertion: vibez_engine::clip_insertion::PreparedAutomationLaneInsertion::new(
                    lane.clone(),
                ),
            });
            self.state
                .arrange_content_mut(audio_track)
                .automation
                .push(lane);
        }
        // Part of the edit already asked for: no delete confirmation.
        self.state.arrangement.pending_project_track_deletion = Some(source.id);
        let _ = self.update(Message::Arrangement(ArrangementMsg::ConfirmRemoveTrack(
            source.id,
        )));
        self.state.arrangement.selected_track = Some(audio_track);
        self.state.status_text = format!("Bounced '{}' in place", source.name);
        self.mark_project_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ProjectTrack;
    use vibez_core::audio_buffer::DecodedAudio;

    #[test]
    fn a_track_freezes_onto_its_own_clip_and_back() {
        let mut app = super::super::test_support::app();
        let source = ProjectTrack::new(TrackId::new(), "Keys".into(), 1);
        let id = source.id;
        Arc::make_mut(&mut app.state.project_tracks)
            .tracks
            .push(source);
        app.finish_freeze(
            id,
            BounceOutcome {
                audio: Arc::new(DecodedAudio {
                    sample_rate: 48_000,
                    channels: vec![vec![0.1; 64]; 2],
                }),
                path: "frozen.wav".into(),
                source: vibez_core::track::MediaSourceRef::LocalFile {
                    path: "frozen.wav".into(),
                },
                clip_name: "Keys".into(),
                insert_position_samples: 0,
            },
        );
        let frozen = app.state.project_tracks.tracks[0].frozen_clip.unwrap();
        assert_eq!(app.state.project_tracks.tracks.len(), 1);
        let clips = &app.state.arrange_content(id).unwrap().clips;
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].id, frozen);

        let _ = app.update(Message::ToggleTrackFreeze(id));
        assert_eq!(app.state.project_tracks.tracks[0].frozen_clip, None);
        assert!(app.state.arrange_content(id).unwrap().clips.is_empty());
    }
}
