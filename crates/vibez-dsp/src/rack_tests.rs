//! Tests of the Effect Rack's parallel sum, macros and alignment.

use super::*;
use vibez_core::rack::{inner_param, RackChain, RackDevice};

fn gain_device(db: f32) -> RackDevice {
    RackDevice {
        effect_type: EffectType::Gain,
        params: vec![db],
        bypass: false,
    }
}

fn two_chains() -> RackState {
    RackState {
        chains: vec![RackChain::default(), RackChain::default()],
        mappings: Vec::new(),
    }
}

fn run(rack: &mut RackEffect, value: f32) -> f32 {
    let mut buffer = vec![value; 256];
    rack.process(&mut buffer, 2);
    // A second block, past any ramp.
    let mut buffer = vec![value; 256];
    rack.process(&mut buffer, 2);
    buffer[255]
}

#[test]
fn an_empty_chain_passes_the_signal_and_parallel_chains_add() {
    let mut one = RackEffect::new(&RackState::default(), 48_000.0, &[]);
    assert!((run(&mut one, 0.25) - 0.25).abs() < 1e-6);
    let mut two = RackEffect::new(&two_chains(), 48_000.0, &[]);
    assert!((run(&mut two, 0.25) - 0.5).abs() < 1e-6);
    two.set_param(CHAIN_ON + 1, 0.0);
    two.set_param(CHAIN_VOLUME, 0.5);
    assert!((run(&mut two, 0.25) - 0.125).abs() < 1e-6);
}

#[test]
fn a_macro_moves_the_parameters_it_is_mapped_to() {
    let state = RackState {
        chains: vec![RackChain {
            devices: vec![gain_device(1.0)],
        }],
        mappings: vec![MacroMapping {
            macro_index: 2,
            chain: 0,
            device: 0,
            param: 0,
            min: 0.25,
            max: 0.75,
        }],
    };
    let mut rack = RackEffect::new(&state, 48_000.0, &[]);
    let inner = inner_param(0, 0, 0);
    rack.set_param(2, 1.0);
    assert!((rack.get_param(inner) - 0.75).abs() < 1e-4);
    rack.set_param(2, 0.0);
    assert!((rack.get_param(inner) - 0.25).abs() < 1e-4);
}

#[test]
fn chains_wait_for_the_slowest_one() {
    let state = RackState {
        chains: vec![
            RackChain::default(),
            RackChain {
                devices: vec![RackDevice {
                    effect_type: EffectType::Limiter,
                    params: Vec::new(),
                    bypass: false,
                }],
            },
        ],
        mappings: Vec::new(),
    };
    let rack = RackEffect::new(&state, 48_000.0, &[]);
    assert!(rack.latency_samples() > 0);
    assert!(rack.chains[0].delay.is_some());
    assert!(rack.chains[1].delay.is_none());
}

#[test]
fn long_blocks_are_split() {
    let mut rack = RackEffect::new(&RackState::default(), 48_000.0, &[]);
    let mut buffer = vec![0.5; BLOCK_SAMPLES * 2 + 64];
    rack.process(&mut buffer, 2);
    assert!(buffer.iter().all(|sample| (sample - 0.5).abs() < 1e-6));
}
