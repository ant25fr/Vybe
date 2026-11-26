//! Tests of channel modulation.

use super::*;
use vibez_core::effect::EffectType;
use vibez_core::id::EffectId;
use vibez_core::modulation::ModShape;

fn clock(beat: f64) -> ModulationClock {
    ModulationClock {
        beat,
        seconds: beat / 2.0,
        frames: 480,
        sample_rate: 48_000,
    }
}

fn square() -> ModulatorSettings {
    ModulatorSettings {
        shape: ModShape::Square,
        division: vibez_core::modulation::ModDivision::Quarter,
        ..Default::default()
    }
}

fn track_with_filter() -> (EngineTrack, AutomationTarget) {
    let mut track = EngineTrack::new(TrackId::new());
    let id = EffectId::new();
    track.effects.push(EffectSlot {
        id,
        effect: vibez_dsp::factory::create_effect(EffectType::Filter, 48_000.0),
        bypass: false,
        levels: Default::default(),
    });
    let target = AutomationTarget::EffectParam {
        effect_id: id,
        param_index: 0,
    };
    (track, target)
}

#[test]
fn a_parameter_moves_around_its_value_and_returns_to_it() {
    let (mut track, target) = track_with_filter();
    let AutomationTarget::EffectParam { effect_id, .. } = target else {
        unreachable!()
    };
    let stage = NodeStage::Effect(effect_id);
    let base = track.effects[0].effect.get_param(0);
    track.modulation.set_modulator(0, Some(square()));
    track.modulation.set_link(
        0,
        0,
        Some(ModulationLink {
            target,
            amount: 0.1,
        }),
    );

    track.advance_modulation(clock(0.0));
    track.apply_modulation(stage);
    let high = track.effects[0].effect.get_param(0);
    assert!(high > base);

    track.advance_modulation(clock(0.75));
    track.apply_modulation(stage);
    assert!(track.effects[0].effect.get_param(0) < base);

    // A hand edit while modulated becomes the new centre.
    track.effects[0].effect.set_param(0, 5_000.0);
    track.advance_modulation(clock(1.0));
    track.apply_modulation(stage);
    assert!(track.effects[0].effect.get_param(0) > 5_000.0);

    track.modulation.set_modulator(0, None);
    track.advance_modulation(clock(1.25));
    track.apply_modulation(stage);
    assert_eq!(track.effects[0].effect.get_param(0), 5_000.0);
}

#[test]
fn two_links_to_one_parameter_add_without_drifting() {
    let (mut track, target) = track_with_filter();
    let AutomationTarget::EffectParam { effect_id, .. } = target else {
        unreachable!()
    };
    let base = track.effects[0].effect.get_param(0);
    for slot in 0..2 {
        track.modulation.set_modulator(slot, Some(square()));
        track.modulation.set_link(
            slot,
            0,
            Some(ModulationLink {
                target,
                amount: 0.05,
            }),
        );
    }
    let mut values = Vec::new();
    for _ in 0..5 {
        track.advance_modulation(clock(0.0));
        track.apply_modulation(NodeStage::Effect(effect_id));
        values.push(track.effects[0].effect.get_param(0));
    }
    assert!(values.iter().all(|value| *value == values[0]));
    assert!(values[0] > base);
}

#[test]
fn gain_and_pan_move_without_changing_the_mixer() {
    let mut track = EngineTrack::new(TrackId::new());
    track.modulation.set_modulator(0, Some(square()));
    track.modulation.set_link(
        0,
        0,
        Some(ModulationLink {
            target: AutomationTarget::TrackPan,
            amount: 0.25,
        }),
    );
    track.advance_modulation(clock(0.0));
    let (gain, pan) = track.modulation.mixed(1.0, 0.5, 479, 480);
    assert_eq!((gain, pan), (1.0, 0.75));
    assert_eq!(track.pan, 0.5);
}

#[test]
fn an_envelope_follows_the_notes() {
    let mut state = ModulationState::default();
    state.set_modulator(
        0,
        Some(ModulatorSettings {
            attack_ms: 10.0,
            decay_ms: 10.0,
            sustain: 0.5,
            release_ms: 10.0,
            ..ModulatorSettings::of(ModulatorKind::Envelope)
        }),
    );
    state.advance(clock(0.0), 0, 1);
    assert_eq!(state.slots[0].value, 0.0);
    state.advance(clock(0.0), 1, 1);
    assert_eq!(state.slots[0].value, 1.0);
    state.advance(clock(0.0), 1, 1);
    assert_eq!(state.slots[0].value, 0.5);
    state.advance(clock(0.0), 0, 1);
    assert_eq!(state.slots[0].value, 0.0);
}
