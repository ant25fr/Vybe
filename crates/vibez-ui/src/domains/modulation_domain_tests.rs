//! Tests of modulator editing.

use std::sync::Arc;

use super::*;
use crate::domains::test_support::RecordingEngine;

fn project() -> (Arc<ProjectTracksState>, TrackId) {
    let mut tracks = ProjectTracksState::default();
    let track = ProjectTrack::new(TrackId::new(), "Lead".into(), 0);
    let id = track.id;
    tracks.tracks.push(track);
    (Arc::new(tracks), id)
}

#[test]
fn adding_a_modulator_and_a_link_reaches_the_engine() {
    let (mut tracks, id) = project();
    let mut state = ModulationState::default();
    let mut engine = RecordingEngine::default();
    state.update(
        ModulationMsg::Add(id, ModulatorKind::Lfo),
        &mut engine,
        &mut tracks,
    );
    state.update(
        ModulationMsg::AddLink(id, 0, AutomationTarget::TrackPan),
        &mut engine,
        &mut tracks,
    );
    assert!(matches!(
        engine.0.as_slice(),
        [
            EngineCommand::SetModulator {
                slot: 0,
                settings: Some(_),
                ..
            },
            EngineCommand::SetModulationLink {
                slot: 0,
                link: 0,
                value: Some(_),
                ..
            },
        ]
    ));
    assert_eq!(tracks.find(id).unwrap().modulators[0].links.len(), 1);
}

#[test]
fn an_edit_that_changes_nothing_is_no_edit() {
    let (mut tracks, id) = project();
    let mut state = ModulationState::default();
    let mut engine = RecordingEngine::default();
    state.update(
        ModulationMsg::Add(id, ModulatorKind::Lfo),
        &mut engine,
        &mut tracks,
    );
    let settings = tracks.find(id).unwrap().modulators[0].settings;
    assert!(ModulationMsg::SetSettings(id, 0, settings)
        .edit(&tracks)
        .is_none());
    assert!(ModulationMsg::Remove(id, 3).edit(&tracks).is_none());
    assert!(ModulationMsg::AddLink(id, 0, AutomationTarget::TrackMute)
        .edit(&tracks)
        .is_none());
}

#[test]
fn a_channel_holds_a_bounded_number_of_modulators() {
    let (mut tracks, id) = project();
    let mut state = ModulationState::default();
    let mut engine = RecordingEngine::default();
    for _ in 0..MAX_MODULATORS + 2 {
        state.update(
            ModulationMsg::Add(id, ModulatorKind::Random),
            &mut engine,
            &mut tracks,
        );
    }
    assert_eq!(tracks.find(id).unwrap().modulators.len(), MAX_MODULATORS);
    assert_eq!(state.selected(id, MAX_MODULATORS), MAX_MODULATORS - 1);
}

#[test]
fn curved_knobs_round_trip_their_values() {
    for field in [ModField::Rate, ModField::Attack] {
        let value = field.range().2;
        assert!((field.value_at(field.position_of(value)) - value).abs() < 1e-2);
    }
}
