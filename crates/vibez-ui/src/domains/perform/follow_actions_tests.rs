//! Tests of follow action timing and targets.

use std::sync::Arc;

use vibez_core::follow_action::{FollowAction, FollowKind, FollowTime};
use vibez_core::id::{ClipId, TrackId};
use vibez_core::perform::MusicalBoundary;

use super::*;
use crate::domains::perform::clip_record::empty_midi_clip;

const SPB: f64 = 100.0;

/// A column of three one-bar Clips; the first plays from sample 0.
fn playing(follow: FollowAction) -> (PerformState, TrackId, [ClipId; 3]) {
    let track = TrackId::new();
    let ids = [ClipId::new(), ClipId::new(), ClipId::new()];
    let mut perform = PerformState::default();
    for (row, id) in ids.into_iter().enumerate() {
        let mut clip = empty_midi_clip(id, track, row as u32, "Part".into(), 4.0);
        if row == 0 {
            clip.launch.follow = Some(follow);
        }
        Arc::make_mut(&mut perform.clips).clips.push(clip);
    }
    let clip = perform.clips.by_id(ids[0]).unwrap().clone();
    perform.clip_editor.running = true;
    perform.clip_editor.playing.insert(track, clip);
    perform.clip_editor.started_at.insert(track, 0);
    (perform, track, ids)
}

fn next() -> FollowAction {
    FollowAction {
        a: FollowKind::Next,
        ..Default::default()
    }
}

#[test]
fn the_next_clip_is_queued_once_in_the_last_bar() {
    let (mut perform, _, ids) = playing(FollowAction {
        time: FollowTime::TwoBars,
        ..next()
    });
    assert!(perform.due_follow_actions(400, SPB).is_empty());
    assert_eq!(
        perform.due_follow_actions(600, SPB),
        vec![DueFollow {
            request: ClipLaunchRequest::Clip(ids[1]),
            boundary: MusicalBoundary::OneBar,
        }]
    );
    assert!(perform.due_follow_actions(700, SPB).is_empty());
}

#[test]
fn a_clip_end_action_stops_the_track() {
    let (mut perform, track, _) = playing(FollowAction {
        a: FollowKind::Stop,
        ..Default::default()
    });
    assert_eq!(
        perform.due_follow_actions(150, SPB)[0].request,
        ClipLaunchRequest::Stop(track)
    );
}

#[test]
fn a_queued_launch_or_a_stopped_transport_outranks_the_action() {
    let (mut perform, track, ids) = playing(next());
    perform.clip_editor.queued.insert(track, Some(ids[2]));
    assert!(perform.due_follow_actions(350, SPB).is_empty());
    perform.clip_editor.queued.clear();
    perform.clip_editor.running = false;
    assert!(perform.due_follow_actions(350, SPB).is_empty());
}

#[test]
fn the_group_ends_at_an_empty_slot() {
    let (mut perform, track, ids) = playing(next());
    let mut apart = empty_midi_clip(ClipId::new(), track, 5, "Apart".into(), 4.0);
    apart.launch.follow = None;
    let apart_id = apart.id;
    Arc::make_mut(&mut perform.clips).clips.push(apart);
    assert_eq!(perform.clips.follow_group(ids[1]), ids.to_vec());
    assert_eq!(perform.clips.follow_group(apart_id), vec![apart_id]);
}
