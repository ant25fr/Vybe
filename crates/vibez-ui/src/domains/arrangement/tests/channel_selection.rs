//! A set of mixer channels selected together, and edits that move the
//! whole set.

use super::*;

fn send(a: &mut ArrangementFixture, msg: ArrangementMsg) -> RecordingEngine {
    let mut engine = RecordingEngine::default();
    a.update(msg, &mut engine, ArrangementCtx::default());
    engine
}

fn gain(a: &ArrangementFixture, id: TrackId) -> f32 {
    a.tracks
        .iter()
        .map(|track| &**track)
        .chain(a.buses.iter().map(|bus| &**bus))
        .find(|track| track.id == id)
        .unwrap()
        .gain
}

#[test]
fn ctrl_and_shift_clicks_build_a_channel_set_in_mixer_order() {
    let mut a = arrangement_with_tracks(4);
    send(&mut a, ArrangementMsg::AddBus);
    let t: Vec<_> = a.tracks.iter().map(|track| track.id).collect();
    let bus = a.buses[0].id;

    send(&mut a, ArrangementMsg::SelectTrack(t[0]));
    assert_eq!(a.arrangement.selected_channels, vec![t[0]]);
    send(&mut a, ArrangementMsg::ToggleChannelSelection(t[2]));
    assert_eq!(a.arrangement.selected_channels, vec![t[0], t[2]]);
    send(&mut a, ArrangementMsg::ToggleChannelSelection(t[0]));
    assert_eq!(a.arrangement.selected_channels, vec![t[2]]);
    send(&mut a, ArrangementMsg::ToggleChannelSelection(t[0]));
    send(&mut a, ArrangementMsg::SelectChannelRange(bus));
    assert_eq!(
        a.arrangement.selected_channels,
        vec![t[0], t[1], t[2], t[3], bus],
        "a range runs from the last channel clicked, through the buses"
    );

    send(&mut a, ArrangementMsg::SelectTrack(t[1]));
    assert_eq!(a.arrangement.selected_channels, vec![t[1]]);
    send(&mut a, ArrangementMsg::SelectAllChannels);
    assert_eq!(
        a.arrangement.selected_channels,
        vec![t[0], t[1], t[2], t[3], bus]
    );

    send(&mut a, ArrangementMsg::SelectTrack(TrackId::MASTER));
    send(
        &mut a,
        ArrangementMsg::ToggleChannelSelection(TrackId::MASTER),
    );
    assert!(
        a.arrangement.selected_channels.is_empty(),
        "the master never joins a set"
    );
}

#[test]
fn moving_a_selected_channel_moves_the_whole_set_together() {
    let mut a = arrangement_with_tracks(3);
    let t: Vec<_> = a.tracks.iter().map(|track| track.id).collect();
    a.tracks[1].gain = 0.5;
    send(&mut a, ArrangementMsg::SelectTrack(t[0]));
    send(&mut a, ArrangementMsg::ToggleChannelSelection(t[1]));

    let engine = send(&mut a, ArrangementMsg::SetTrackGain(t[0], 0.5));
    assert_eq!(gain(&a, t[0]), 0.5);
    assert!(
        (gain(&a, t[1]) - 0.25).abs() < 1e-6,
        "same dB change, so the balance holds"
    );
    assert_eq!(gain(&a, t[2]), 1.0);
    assert!(engine
        .0
        .iter()
        .any(|command| matches!(command, EngineCommand::SetTrackGain(id, _) if *id == t[1])));

    send(&mut a, ArrangementMsg::SetTrackPan(t[1], 0.7));
    assert!((a.tracks[0].pan - 0.7).abs() < 1e-6);
    assert!((a.tracks[1].pan - 0.7).abs() < 1e-6);
    assert!((a.tracks[2].pan - 0.5).abs() < 1e-6);

    send(&mut a, ArrangementMsg::SetTrackMute(t[0]));
    assert!(a.tracks[0].mute && a.tracks[1].mute && !a.tracks[2].mute);
    send(&mut a, ArrangementMsg::SetTrackSolo(t[1]));
    assert!(a.tracks[0].solo && a.tracks[1].solo && !a.tracks[2].solo);

    send(&mut a, ArrangementMsg::SetTrackGain(t[2], 0.2));
    assert_eq!(gain(&a, t[2]), 0.2, "an unselected channel moves alone");
    assert_eq!(gain(&a, t[0]), 0.5);
}

#[test]
fn sends_on_a_selected_set_move_together() {
    let mut a = arrangement_with_tracks(2);
    send(&mut a, ArrangementMsg::AddBus);
    let t: Vec<_> = a.tracks.iter().map(|track| track.id).collect();
    let bus = a.buses[0].id;
    a.tracks[0].sends.push((bus, 0.2));
    a.tracks[1].sends.push((bus, 0.5));
    send(&mut a, ArrangementMsg::SelectTrack(t[0]));
    send(&mut a, ArrangementMsg::ToggleChannelSelection(t[1]));

    send(
        &mut a,
        ArrangementMsg::SetSend {
            track_id: t[0],
            bus_id: bus,
            amount: 0.4,
        },
    );
    assert!((a.tracks[0].sends[0].1 - 0.4).abs() < 1e-6);
    assert!((a.tracks[1].sends[0].1 - 0.7).abs() < 1e-6);
}

#[test]
fn the_pre_fader_switch_toggles_the_send_and_a_removed_bus_forgets_it() {
    let mut a = arrangement_with_tracks(1);
    send(&mut a, ArrangementMsg::AddBus);
    let track = a.tracks[0].id;
    let bus = a.buses[0].id;
    let toggle = || ArrangementMsg::ToggleSendPreFader {
        track_id: track,
        bus_id: bus,
    };
    send(&mut a, toggle());
    assert_eq!(a.tracks[0].pre_fader_sends, [bus]);
    send(&mut a, toggle());
    assert!(a.tracks[0].pre_fader_sends.is_empty());
    send(&mut a, toggle());
    send(&mut a, ArrangementMsg::RemoveBus(bus));
    assert!(a.tracks[0].pre_fader_sends.is_empty());
}

#[test]
fn a_return_sends_to_another_but_never_back_into_itself() {
    let mut a = arrangement_with_tracks(1);
    send(&mut a, ArrangementMsg::AddBus);
    send(&mut a, ArrangementMsg::AddBus);
    let (first, second) = (a.buses[0].id, a.buses[1].id);
    let set = |a: &mut ArrangementFixture, track_id, bus_id| {
        send(
            a,
            ArrangementMsg::SetSend {
                track_id,
                bus_id,
                amount: 0.5,
            },
        );
    };
    send(&mut a, ArrangementMsg::SelectTrack(first));
    set(&mut a, first, second);
    assert_eq!(a.buses[0].sends, [(second, 0.5)]);
    // Back again, or into itself, would feed the signal round.
    send(&mut a, ArrangementMsg::SelectTrack(second));
    set(&mut a, second, first);
    set(&mut a, second, second);
    assert!(a.buses[1].sends.is_empty());

    send(&mut a, ArrangementMsg::RemoveBus(second));
    assert!(a.buses[0].sends.is_empty());
}

#[test]
fn selected_tracks_group_and_ungroup() {
    let mut a = arrangement_with_tracks(3);
    send(&mut a, ArrangementMsg::AddBus);
    let ids: Vec<_> = a.tracks.iter().map(|track| track.id).collect();
    let ret = a.buses[0].id;
    send(&mut a, ArrangementMsg::SelectTrack(ids[0]));
    send(&mut a, ArrangementMsg::ToggleChannelSelection(ids[1]));
    send(&mut a, ArrangementMsg::ToggleChannelSelection(ret));
    send(&mut a, ArrangementMsg::GroupSelectedTracks);
    let group = a.buses[1].id;
    assert!(a.buses[1].is_group);
    let outputs: Vec<_> = a.tracks.iter().map(|track| track.output).collect();
    assert_eq!(outputs, [Some(group), Some(group), None]);
    // A group is not a send target.
    send(
        &mut a,
        ArrangementMsg::SetSend {
            track_id: ids[2],
            bus_id: group,
            amount: 0.5,
        },
    );
    assert!(a.tracks[2].sends.is_empty());
    // Ungrouping from a member dissolves its group only.
    send(&mut a, ArrangementMsg::SelectTrack(ids[1]));
    send(&mut a, ArrangementMsg::UngroupSelected);
    assert_eq!(a.buses.len(), 1);
    assert!(a.tracks.iter().all(|track| track.output.is_none()));
}

/// The fixture's channels as the project holds them.
fn project(a: &ArrangementFixture) -> crate::state::ProjectTracksState {
    crate::state::ProjectTracksState {
        tracks: a.tracks.iter().map(|track| (**track).clone()).collect(),
        buses: a.buses.iter().map(|bus| (**bus).clone()).collect(),
        ..Default::default()
    }
}

#[test]
fn groups_nest_gather_and_take_members() {
    let mut a = arrangement_with_tracks(4);
    let ids: Vec<_> = a.tracks.iter().map(|track| track.id).collect();
    let select = |a: &mut ArrangementFixture, chosen: &[TrackId]| {
        send(a, ArrangementMsg::SelectTrack(chosen[0]));
        for &id in &chosen[1..] {
            send(a, ArrangementMsg::ToggleChannelSelection(id));
        }
    };
    // Tracks 0 and 2 form a group and come together.
    select(&mut a, &[ids[0], ids[2]]);
    send(&mut a, ArrangementMsg::GroupSelectedTracks);
    let outer = a.buses[0].id;
    let order: Vec<_> = a.tracks.iter().map(|track| track.id).collect();
    assert_eq!(order, [ids[0], ids[2], ids[1], ids[3]]);
    // Grouping inside it nests the new group.
    select(&mut a, &[ids[2]]);
    send(&mut a, ArrangementMsg::GroupSelectedTracks);
    let inner = a.buses[1].id;
    assert_eq!(a.buses[1].output, Some(outer));
    assert_eq!(a.tracks[1].output, Some(inner));
    assert_eq!(project(&a).ancestors(ids[2]), [inner, outer]);

    // An existing group takes a track, never the group around it.
    select(&mut a, &[ids[3], outer]);
    send(&mut a, ArrangementMsg::AddSelectionToGroup(inner));
    assert_eq!(project(&a).sender(ids[3]).unwrap().output, Some(inner));
    assert_eq!(project(&a).sender(outer).unwrap().output, None);

    // Folding the outer group hides everything inside.
    send(&mut a, ArrangementMsg::ToggleGroupFold(outer));
    assert!(project(&a).is_folded_away(ids[3]) && project(&a).is_folded_away(ids[0]));
    assert!(!project(&a).is_folded_away(ids[1]));

    // Moving out goes one level up.
    select(&mut a, &[ids[3]]);
    send(&mut a, ArrangementMsg::RemoveSelectionFromGroup);
    assert_eq!(project(&a).sender(ids[3]).unwrap().output, Some(outer));

    // Removing the inner group lifts its members into the outer one.
    send(&mut a, ArrangementMsg::RemoveBus(inner));
    assert_eq!(project(&a).sender(ids[2]).unwrap().output, Some(outer));
}
