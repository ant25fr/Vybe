//! Tests of rack editing.

use std::sync::Arc;

use super::*;
use crate::domains::test_support::RecordingEngine;
use crate::state::ProjectTrack;

const SR: u32 = 48_000;

fn project() -> (Arc<ProjectTracksState>, TrackId, EffectId) {
    let mut tracks = ProjectTracksState::default();
    let mut track = ProjectTrack::new(TrackId::new(), "Bus".into(), 0);
    let effect_id = EffectId::new();
    let rack = RackState::default();
    track.effects.push(UiEffect {
        reconfiguration_failed: false,
        latency_samples: Some(0),
        inactive_sidechains: Default::default(),
        sidechains: Default::default(),
        external_inputs: Default::default(),
        id: effect_id,
        effect_type: EffectType::Rack,
        bypass: false,
        params: RACK_PARAMS.iter().map(|d| d.default).collect(),
        descriptors: RACK_PARAMS,
        plugin_name: None,
        has_plugin_gui: false,
        plugin_ref: None,
        rack: Some(rack),
    });
    let track_id = track.id;
    tracks.tracks.push(track);
    (Arc::new(tracks), track_id, effect_id)
}

fn apply(
    state: &mut RackUiState,
    engine: &mut RecordingEngine,
    tracks: &mut Arc<ProjectTracksState>,
    track_id: TrackId,
    effect_id: EffectId,
    edit: RackEdit,
) -> bool {
    state.update(
        RackMsg {
            track_id,
            effect_id,
            edit,
        },
        engine,
        tracks,
        SR,
    )
}

fn rack(tracks: &ProjectTracksState, track_id: TrackId) -> (RackState, Vec<f32>) {
    let effect = &tracks.find(track_id).unwrap().effects[0];
    (effect.rack.clone().unwrap(), effect.params.clone())
}

#[test]
fn a_structural_edit_rebuilds_and_a_knob_does_not() {
    let (mut tracks, track_id, effect_id) = project();
    let mut state = RackUiState::default();
    let mut engine = RecordingEngine::default();
    let add = RackEdit::AddDevice {
        chain: 0,
        effect_type: EffectType::Filter,
    };
    assert!(apply(
        &mut state,
        &mut engine,
        &mut tracks,
        track_id,
        effect_id,
        add
    ));
    assert!(matches!(
        engine.0.as_slice(),
        [
            EngineCommand::RemoveEffect(..),
            EngineCommand::InsertEffect {
                position: Some(0),
                ..
            },
            EngineCommand::SetEffectBypass { .. },
        ]
    ));
    engine.0.clear();
    let turn = RackEdit::SetDeviceParam {
        chain: 0,
        device: 0,
        param: 0,
        value: 500.0,
    };
    assert!(!apply(
        &mut state,
        &mut engine,
        &mut tracks,
        track_id,
        effect_id,
        turn
    ));
    assert!(matches!(
        engine.0.as_slice(),
        [EngineCommand::SetEffectParam { param_index, .. }]
            if *param_index == inner_param(0, 0, 0)
    ));
    assert_eq!(
        rack(&tracks, track_id).0.chains[0].devices[0].params[0],
        500.0
    );
}

#[test]
fn mapping_a_parameter_keeps_its_value_and_hands_it_to_the_macro() {
    let (mut tracks, track_id, effect_id) = project();
    let mut state = RackUiState::default();
    let mut engine = RecordingEngine::default();
    for edit in [
        RackEdit::AddDevice {
            chain: 0,
            effect_type: EffectType::Filter,
        },
        RackEdit::Map {
            chain: 0,
            device: 0,
            param: 0,
            macro_index: Some(1),
        },
    ] {
        apply(
            &mut state,
            &mut engine,
            &mut tracks,
            track_id,
            effect_id,
            edit,
        );
    }
    let (rack_state, params) = rack(&tracks, track_id);
    let descriptor = &descriptors(EffectType::Filter)[0];
    let value = device_value(&rack_state, &params, 0, 0, 0, descriptor);
    assert!((value - descriptor.default).abs() < 0.5);
    let turn = RackMsg {
        track_id,
        effect_id,
        edit: RackEdit::SetDeviceParam {
            chain: 0,
            device: 0,
            param: 0,
            value: 100.0,
        },
    };
    assert!(turn.edited(&tracks).is_none());
}

#[test]
fn the_last_chain_stays_and_removed_chains_shift_their_volumes() {
    let (mut tracks, track_id, effect_id) = project();
    let mut state = RackUiState::default();
    let mut engine = RecordingEngine::default();
    let remove = |chain| RackMsg {
        track_id,
        effect_id,
        edit: RackEdit::RemoveChain(chain),
    };
    assert!(remove(0).edited(&tracks).is_none());
    apply(
        &mut state,
        &mut engine,
        &mut tracks,
        track_id,
        effect_id,
        RackEdit::AddChain,
    );
    {
        let tracks = Arc::make_mut(&mut tracks);
        tracks.find_mut(track_id).unwrap().effects[0].params[CHAIN_VOLUME + 1] = 0.25;
    }
    apply(
        &mut state,
        &mut engine,
        &mut tracks,
        track_id,
        effect_id,
        RackEdit::RemoveChain(0),
    );
    assert_eq!(rack(&tracks, track_id).1[CHAIN_VOLUME], 0.25);
}
