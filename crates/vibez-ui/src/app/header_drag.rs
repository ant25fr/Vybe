//! Dragging a track or group header in Arrange moves it, with the rest of
//! the selection, into or out of groups: dropped on a group it joins it,
//! dropped on a track it joins that track's group or leaves them all.

use vibez_core::id::TrackId;

use crate::domains::arrangement::ArrangementMsg;
use crate::widgets::track_header::header_click;

use super::*;

impl App {
    pub(super) fn press_header(&mut self, track_id: TrackId) -> Task<Message> {
        self.dragged_header = Some(track_id);
        let held = self.state.view.held_modifiers;
        // Pressing a selected header keeps the selection, to drag it whole.
        let selected = self.state.arrangement.selected_channels.contains(&track_id);
        if selected && held.is_empty() {
            return Task::none();
        }
        self.update(header_click(track_id, held))
    }

    /// Whether the header being dragged is over `track_id` and a release
    /// there would move it.
    pub(super) fn drop_would_move(&self, track_id: TrackId) -> bool {
        let Some(source) = self.dragged_header else {
            return false;
        };
        let selection = &self.state.arrangement.selected_channels;
        if self.state.view.hovered_track_header != Some(track_id)
            || source == track_id
            || selection.contains(&track_id)
        {
            return false;
        }
        let project = &self.state.project_tracks;
        match project.sender(track_id) {
            Some(channel) if channel.is_group || channel.output.is_some() => true,
            // A top-level track only takes something out of its group.
            Some(_) => project
                .sender(source)
                .is_some_and(|channel| channel.output.is_some()),
            None => false,
        }
    }

    pub(super) fn release_over_header(&mut self, target: TrackId) -> Task<Message> {
        let Some(source) = self.dragged_header.take() else {
            return Task::none();
        };
        let project = &self.state.project_tracks;
        let selection = &self.state.arrangement.selected_channels;
        if source == target || selection.contains(&target) || !selection.contains(&source) {
            return Task::none();
        }
        let group = match project.sender(target) {
            Some(channel) if channel.is_group => Some(target),
            Some(channel) => channel.output,
            None => return Task::none(),
        };
        let edit = match group {
            Some(group) => ArrangementMsg::AddSelectionToGroup(group),
            // A top-level track: only something in a group has anywhere to go.
            None if project
                .sender(source)
                .is_some_and(|channel| channel.output.is_some()) =>
            {
                ArrangementMsg::MoveSelectionToTopLevel
            }
            None => return Task::none(),
        };
        self.update(Message::Arrangement(edit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ProjectTrack;

    #[test]
    fn a_header_dragged_onto_a_group_joins_it_and_off_it_leaves() {
        let mut app = super::super::test_support::app();
        let mut group = ProjectTrack::new(TrackId::new(), "Group 1".into(), 0);
        group.is_group = true;
        let kick = ProjectTrack::new(TrackId::new(), "Kick".into(), 0);
        let bass = ProjectTrack::new(TrackId::new(), "Bass".into(), 0);
        let (group_id, kick_id, bass_id) = (group.id, kick.id, bass.id);
        let project = Arc::make_mut(&mut app.state.project_tracks);
        project.tracks.extend([kick, bass]);
        project.buses.push(group);
        let output = |app: &App| app.state.project_tracks.sender(kick_id).unwrap().output;

        let _ = app.update(Message::HeaderPressed(kick_id));
        let _ = app.update(Message::HeaderReleased(group_id));
        assert_eq!(output(&app), Some(group_id));

        let _ = app.update(Message::HeaderPressed(kick_id));
        let _ = app.update(Message::HeaderReleased(bass_id));
        assert_eq!(output(&app), None);

        // A release with nothing dragged changes nothing.
        let _ = app.update(Message::HeaderReleased(group_id));
        assert_eq!(output(&app), None);
    }
}
