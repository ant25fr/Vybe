//! The Effect Rack: parallel chains of built-in effects summed together, with
//! macros moving their parameters.

use vibez_core::audio_context::DeviceAudioContext;
use vibez_core::effect::{EffectType, ParamDescriptor};
use vibez_core::rack::{
    split_inner_param, MacroMapping, RackState, CHAIN_ON, CHAIN_VOLUME, MACROS, MAX_CHAINS,
    PARAM_COUNT, RACK_PARAMS,
};

use crate::compensation_delay::CompensationDelay;
use crate::effect::AudioEffect;
use crate::factory::create_effect_with_params;

/// Samples processed at once; longer blocks are split, so nothing grows on the
/// audio thread.
const BLOCK_SAMPLES: usize = 8_192;
const CHANNELS: usize = 2;

struct Chain {
    devices: Vec<(Box<dyn AudioEffect>, bool)>,
    /// Waits this chain for the slowest one, so they sum in time.
    delay: Option<CompensationDelay>,
    /// The gain reached at the end of the last block.
    gain: f32,
}

pub struct RackEffect {
    params: [f32; PARAM_COUNT],
    chains: Vec<Chain>,
    mappings: Vec<MacroMapping>,
    latency: u32,
    input: Vec<f32>,
    work: Vec<f32>,
    sum: Vec<f32>,
}

impl RackEffect {
    /// Build the rack `state` describes, with the effect's saved `params`.
    pub fn new(state: &RackState, sample_rate: f32, params: &[f32]) -> Self {
        let state = state.clone().sanitized();
        let mut chains: Vec<Chain> = state
            .chains
            .iter()
            .map(|chain| Chain {
                devices: chain
                    .devices
                    .iter()
                    .map(|device| {
                        (
                            create_effect_with_params(
                                device.effect_type,
                                sample_rate,
                                &device.params,
                            ),
                            device.bypass,
                        )
                    })
                    .collect(),
                delay: None,
                gain: 1.0,
            })
            .collect();
        let latency_of = |chain: &Chain| {
            chain
                .devices
                .iter()
                .filter(|(_, bypass)| !bypass)
                .map(|(device, _)| device.latency_samples())
                .sum::<u32>()
        };
        let latency = chains.iter().map(latency_of).max().unwrap_or(0);
        for chain in &mut chains {
            let wait = latency - latency_of(chain);
            if wait > 0 {
                let samples = wait as usize * CHANNELS;
                chain.delay = CompensationDelay::prepare(wait, CHANNELS, samples * 2).ok();
            }
        }
        let mut rack = Self {
            params: std::array::from_fn(|index| RACK_PARAMS[index].default),
            chains,
            mappings: state.mappings,
            latency,
            input: vec![0.0; BLOCK_SAMPLES],
            work: vec![0.0; BLOCK_SAMPLES],
            sum: vec![0.0; BLOCK_SAMPLES],
        };
        for (index, value) in params.iter().copied().take(PARAM_COUNT).enumerate() {
            if value.is_finite() {
                rack.params[index] = value.clamp(RACK_PARAMS[index].min, RACK_PARAMS[index].max);
            }
        }
        // Mapped parameters belong to their macro, even one left at zero.
        for index in 0..MACROS {
            rack.apply_macro(index, false);
        }
        for (index, chain) in rack.chains.iter_mut().enumerate() {
            chain.gain = target_gain(&rack.params, index);
        }
        rack
    }

    fn apply_macro(&mut self, index: usize, automated: bool) {
        let amount = self.params[index];
        for mapping in &self.mappings {
            if usize::from(mapping.macro_index) != index {
                continue;
            }
            let Some((device, _)) = self
                .chains
                .get_mut(usize::from(mapping.chain))
                .and_then(|chain| chain.devices.get_mut(usize::from(mapping.device)))
            else {
                continue;
            };
            let value = mapping.value_at(amount);
            let param = usize::from(mapping.param);
            if automated {
                device.set_automated_param(param, value);
            } else {
                device.set_param(param, value);
            }
        }
    }

    fn edit(&mut self, index: usize, value: f32, automated: bool) -> bool {
        if !value.is_finite() {
            return false;
        }
        if let Some((chain, device, param)) = split_inner_param(index) {
            return self
                .chains
                .get_mut(chain)
                .and_then(|chain| chain.devices.get_mut(device))
                .is_some_and(|(device, _)| {
                    if automated {
                        device.set_automated_param(param, value)
                    } else {
                        device.set_param(param, value)
                    }
                });
        }
        let Some(descriptor) = RACK_PARAMS.get(index) else {
            return false;
        };
        self.params[index] = value.clamp(descriptor.min, descriptor.max);
        if index < MACROS {
            self.apply_macro(index, automated);
        }
        true
    }

    /// Process one block no longer than [`BLOCK_SAMPLES`].
    fn process_block(&mut self, buffer: &mut [f32], channels: usize) {
        let length = buffer.len();
        self.input[..length].copy_from_slice(buffer);
        self.sum[..length].fill(0.0);
        let frames = length / channels;
        for (index, chain) in self.chains.iter_mut().enumerate() {
            let work = &mut self.work[..length];
            work.copy_from_slice(&self.input[..length]);
            for (device, bypass) in &mut chain.devices {
                if !*bypass {
                    device.process(work, channels);
                }
            }
            if let Some(delay) = &mut chain.delay {
                delay.process_layout(work, channels);
            }
            // Ramped, so switching or moving a chain does not click.
            let (from, to) = (chain.gain, target_gain(&self.params, index));
            for frame in 0..frames {
                let gain = from + (to - from) * (frame + 1) as f32 / frames as f32;
                for channel in 0..channels {
                    let sample = frame * channels + channel;
                    self.sum[sample] += work[sample] * gain;
                }
            }
            chain.gain = to;
        }
        buffer.copy_from_slice(&self.sum[..length]);
    }
}

fn target_gain(params: &[f32; PARAM_COUNT], chain: usize) -> f32 {
    if chain >= MAX_CHAINS || params[CHAIN_ON + chain] < 0.5 {
        0.0
    } else {
        params[CHAIN_VOLUME + chain]
    }
}

impl AudioEffect for RackEffect {
    fn effect_type(&self) -> EffectType {
        EffectType::Rack
    }

    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        RACK_PARAMS
    }

    fn latency_samples(&self) -> u32 {
        self.latency
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        self.edit(index, value, false)
    }

    fn set_automated_param(&mut self, index: usize, value: f32) -> bool {
        self.edit(index, value, true)
    }

    fn get_param(&self, index: usize) -> f32 {
        if let Some((chain, device, param)) = split_inner_param(index) {
            return self
                .chains
                .get(chain)
                .and_then(|chain| chain.devices.get(device))
                .map_or(0.0, |(device, _)| device.get_param(param));
        }
        self.params.get(index).copied().unwrap_or(0.0)
    }

    fn set_audio_context(&mut self, context: DeviceAudioContext) {
        for chain in &mut self.chains {
            for (device, _) in &mut chain.devices {
                device.set_audio_context(context);
            }
        }
    }

    fn process(&mut self, buffer: &mut [f32], channels: usize) {
        // A rack without chains is empty hardware: the signal passes.
        if self.chains.is_empty() || !(1..=CHANNELS).contains(&channels) {
            return;
        }
        for block in buffer.chunks_mut(BLOCK_SAMPLES) {
            self.process_block(block, channels);
        }
    }

    fn reset(&mut self) {
        for chain in &mut self.chains {
            for (device, _) in &mut chain.devices {
                device.reset();
            }
            if let Some(delay) = &mut chain.delay {
                delay.clear();
            }
        }
    }
}

#[cfg(test)]
#[path = "rack_tests.rs"]
mod tests;
