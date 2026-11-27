//! A frozen track's clips are set aside until it is unfrozen, as in Live:
//! no edit, drop, paste or take may change them.

use std::sync::Arc;

use vibez_core::id::TrackId;

use crate::domains::arrangement::{ArrangementCtx, ArrangementMsg};
use crate::domains::piano_roll::PianoRollMsg;
use crate::domains::DiscardingEngine;
use crate::state::{ArrangementSelection, TimelineEditorState};

use super::*;

impl App {
    fn is_frozen(&self, track_id: TrackId) -> bool {
        self.state
            .project_tracks
            .tracks
            .iter()
            .any(|track| track.id == track_id && track.frozen_clip.is_some())
    }

    fn explain_frozen(&mut self, track_id: TrackId) {
        let name = self
            .state
            .find_track(track_id)
            .map_or_else(String::new, |track| track.name.clone());
        self.state.status_text = format!("'{name}' is frozen: unfreeze it to edit its clips");
    }

    /// Whether `track_id` is frozen, saying why nothing happens when it is.
    pub(super) fn refuse_frozen_track(&mut self, track_id: TrackId) -> bool {
        let frozen = self.is_frozen(track_id);
        if frozen {
            self.explain_frozen(track_id);
        }
        frozen
    }

    /// Whether `message` would change a frozen track's Arrange clips; checked
    /// before it runs, so a refused edit leaves no undo step.
    pub(super) fn refuse_frozen_edit(&mut self, message: &Message) -> bool {
        // Frozen audio would be re-pitched by the tempo it was not rendered at.
        if let Message::Automation(crate::domains::automation::AutomationMsg::AddTempoLane {
            ..
        }) = message
        {
            let frozen = self
                .state
                .project_tracks
                .tracks
                .iter()
                .any(|track| track.frozen_clip.is_some());
            if frozen {
                self.state.status_text = "Unfreeze every track to automate the tempo".into();
            }
            return frozen;
        }
        if self.state.view.workspace == crate::state::Workspace::Perform {
            return false;
        }
        let changed = match message {
            Message::Arrangement(msg)
                if msg.is_timeline_editor_message() || msg.is_clipboard_message() =>
            {
                self.frozen_track_changed_by(msg)
            }
            Message::PianoRoll(PianoRollMsg::AddNoteClipToTrack(track_id))
                if self.is_frozen(*track_id) =>
            {
                Some(*track_id)
            }
            Message::Rack(msg)
                if !matches!(msg.edit, crate::domains::rack::RackEdit::Select { .. })
                    && self.is_frozen(msg.track_id) =>
            {
                Some(msg.track_id)
            }
            Message::Modulation(msg)
                if !matches!(msg, crate::domains::modulation::ModulationMsg::Select(..))
                    && self.is_frozen(msg.track_id()) =>
            {
                Some(msg.track_id())
            }
            _ => None,
        };
        if let Some(track_id) = changed {
            self.explain_frozen(track_id);
        }
        changed.is_some()
    }

    /// Rehearses `msg` on a copy of the Arrange timeline.
    fn frozen_track_changed_by(&self, msg: &ArrangementMsg) -> Option<TrackId> {
        let frozen: Vec<_> = self
            .state
            .project_tracks
            .tracks
            .iter()
            .filter(|track| track.frozen_clip.is_some())
            .map(|track| track.id)
            .collect();
        if frozen.is_empty() {
            return None;
        }
        let mut editor: TimelineEditorState = self.state.arrangement.editor.clone();
        let mut project_tracks = Arc::clone(&self.state.project_tracks);
        let samples_per_beat = self.state.transport.samples_per_beat();
        let ctx = ArrangementCtx {
            samples_per_beat,
            playhead_samples: self.state.transport.position_samples,
            playhead_beats: self.state.transport.position_beats(),
        };
        let mut engine = DiscardingEngine;
        let msg = msg.clone();
        if msg.is_clipboard_message() {
            let mut clipboard = self.state.clip_clipboard.clone();
            editor.update_clipboard(&project_tracks, msg, &mut clipboard, &mut engine, ctx);
        } else {
            editor.update(Arc::make_mut(&mut project_tracks), msg, &mut engine, ctx);
        }
        let before = &self.state.arrangement.editor.timeline;
        frozen.into_iter().find(|&track_id| {
            // A deleted track may go, frozen or not.
            project_tracks.tracks.iter().any(|t| t.id == track_id)
                && clips_differ(before.get(track_id), editor.timeline.get(track_id))
        })
    }

    /// The Arrange plays its comps again.
    pub(super) fn stop_take_audition(&mut self) {
        self.state
            .arrangement
            .editor
            .stop_take_audition(&mut crate::domains::EngineTx(&mut self.cmd_tx));
    }

    /// Select All and box selection pass frozen tracks by.
    pub(super) fn deselect_frozen_clips(&mut self) {
        if !self
            .state
            .project_tracks
            .tracks
            .iter()
            .any(|track| track.frozen_clip.is_some())
        {
            return;
        }
        let tracks = Arc::clone(&self.state.project_tracks);
        self.state.arrangement.selected_clips.retain(|selected| {
            let (ArrangementSelection::AudioClip { track_id, .. }
            | ArrangementSelection::NoteClip { track_id, .. }) = selected;
            !tracks
                .tracks
                .iter()
                .any(|track| track.id == *track_id && track.frozen_clip.is_some())
        });
    }
}

fn clips_differ(
    before: Option<&crate::state::TrackTimelineContent>,
    after: Option<&crate::state::TrackTimelineContent>,
) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => {
            before.clips != after.clips || before.note_clips != after.note_clips
        }
        (before, after) => [before, after]
            .into_iter()
            .flatten()
            .any(|content| !content.clips.is_empty() || !content.note_clips.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Workspace;

    #[test]
    fn a_frozen_track_refuses_clip_edits_and_keeps_its_selection_empty() {
        let mut app = super::super::test_support::app();
        let track = crate::state::ProjectTrack::new(TrackId::new(), "Keys".into(), 0);
        let track_id = track.id;
        Arc::make_mut(&mut app.state.project_tracks)
            .tracks
            .push(track);
        let audio = crate::state::UiClip::test_clip(0, 64);
        let clip = audio.id;
        app.state.arrange_content_mut(track_id).clips.push(audio);
        app.state.view.workspace = Workspace::Arrange;
        Arc::make_mut(&mut app.state.project_tracks)
            .find_mut(track_id)
            .unwrap()
            .frozen_clip = Some(clip);

        let _ = app.update(Message::Arrangement(ArrangementMsg::RemoveClip(
            track_id, clip,
        )));
        assert_eq!(app.state.arrange_content(track_id).unwrap().clips.len(), 1);
        assert!(app.state.status_text.contains("frozen"));
        assert!(!app.state.project.history.can_undo());

        let _ = app.update(Message::Arrangement(ArrangementMsg::SelectAllClips));
        assert!(app.state.arrangement.selected_clips.is_empty());
    }
}
