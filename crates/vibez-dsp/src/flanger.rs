//! Flanger: the same two swept read taps as a chorus, but over a very short
//! delay (a few milliseconds) with strong feedback, so the comb notches sweep
//! through the spectrum instead of thickening the sound.

use vibez_core::effect::{EffectType, ParamDescriptor};

use crate::chorus::DelayLine;
use crate::effect::AudioEffect;

pub mod param {
    pub const RATE: usize = 0;
    pub const DEPTH: usize = 1;
    pub const DELAY: usize = 2;
    pub const FEEDBACK: usize = 3;
    pub const MIX: usize = 4;
    pub const COUNT: usize = 5;
}

/// Widest sweep of the read taps, in milliseconds, at Depth = 1.
const MAX_SWEEP_MS: f32 = 5.0;
/// Longest centre delay, in milliseconds.
const MAX_DELAY_MS: f32 = 10.0;

pub static FLANGER_PARAMS: &[ParamDescriptor] = &[
    ParamDescriptor {
        name: "Rate",
        min: 0.05,
        max: 10.0,
        default: 0.6,
        unit: "Hz",
    },
    ParamDescriptor {
        name: "Depth",
        min: 0.0,
        max: 1.0,
        default: 0.8,
        unit: "",
    },
    ParamDescriptor {
        name: "Delay",
        min: 0.5,
        max: MAX_DELAY_MS,
        default: 4.0,
        unit: "ms",
    },
    ParamDescriptor {
        name: "Feedback",
        min: -0.95,
        max: 0.95,
        default: 0.4,
        unit: "",
    },
    ParamDescriptor {
        name: "Mix",
        min: 0.0,
        max: 1.0,
        default: 0.5,
        unit: "",
    },
];

pub struct FlangerEffect {
    params: [f32; param::COUNT],
    lines: Vec<DelayLine>,
    lfo: [f32; 2],
    sample_rate: f32,
}

impl FlangerEffect {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate >= 1.0 {
            sample_rate
        } else {
            48_000.0
        };
        let capacity = ((MAX_DELAY_MS + MAX_SWEEP_MS) / 1000.0 * sample_rate) as usize + 8;
        Self {
            params: std::array::from_fn(|index| FLANGER_PARAMS[index].default),
            lines: vec![DelayLine::new(capacity), DelayLine::new(capacity)],
            lfo: [0.0, 0.9],
            sample_rate,
        }
    }
}

impl AudioEffect for FlangerEffect {
    fn supports_level_meter(&self) -> bool {
        true
    }

    fn activation_sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate as u32)
    }

    fn effect_type(&self) -> EffectType {
        EffectType::Flanger
    }

    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        FLANGER_PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        let Some(descriptor) = FLANGER_PARAMS.get(index) else {
            return false;
        };
        if !value.is_finite() {
            return false;
        }
        self.params[index] = value.clamp(descriptor.min, descriptor.max);
        true
    }

    fn get_param(&self, index: usize) -> f32 {
        self.params.get(index).copied().unwrap_or(0.0)
    }

    fn process(&mut self, buffer: &mut [f32], channels: usize) {
        if channels == 0 {
            return;
        }
        let rate = self.params[param::RATE];
        let depth = self.params[param::DEPTH];
        let base = self.params[param::DELAY] * 0.001 * self.sample_rate;
        let sweep = MAX_SWEEP_MS * 0.001 * self.sample_rate * depth;
        let feedback = self.params[param::FEEDBACK];
        let mix = self.params[param::MIX];
        let inc = std::f32::consts::TAU * rate / self.sample_rate;
        let lines = &mut self.lines;
        let lfo = &mut self.lfo;
        for frame in buffer.chunks_exact_mut(channels) {
            for (ch, sample) in frame.iter_mut().enumerate() {
                let ch = ch.min(1);
                let line = &mut lines[ch];
                let phase = lfo[ch];
                let off0 = (base + sweep * phase.sin()).max(1.0);
                let off1 = (base + sweep * (phase + std::f32::consts::PI).sin()).max(1.0);
                let wet = 0.5 * (line.read(off0) + line.read(off1));
                let input = *sample;
                line.write(input + feedback * wet);
                *sample = input * (1.0 - mix) + wet * mix;
                if !sample.is_finite() {
                    *sample = 0.0;
                }
                lfo[ch] += inc;
                if lfo[ch] >= std::f32::consts::TAU {
                    lfo[ch] -= std::f32::consts::TAU;
                }
            }
        }
    }

    fn reset(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
        self.lfo = [0.0, 0.9];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_zero_is_identity() {
        let mut fx = FlangerEffect::new(48_000.0);
        fx.set_param(param::MIX, 0.0);
        let orig = vec![0.4_f32; 512];
        let mut buf = orig.clone();
        fx.process(&mut buf, 2);
        for (a, b) in buf.iter().zip(orig.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn depth_zero_and_feedback_zero_is_plain_delay() {
        // With no sweep and no feedback, Mix = 1 gives exactly the centre
        // delay: an impulse comes back Delay ms later.
        let sr = 48_000.0;
        let mut fx = FlangerEffect::new(sr);
        fx.set_param(param::DEPTH, 0.0);
        fx.set_param(param::FEEDBACK, 0.0);
        fx.set_param(param::MIX, 1.0);
        fx.set_param(param::DELAY, 10.0);
        let delay_samples = (10.0 * 0.001 * sr) as usize; // 480
        let mut buf = vec![0.0_f32; 2 * (delay_samples + 64)];
        buf[0] = 1.0;
        buf[1] = 1.0;
        fx.process(&mut buf, 2);
        // The two taps sit at the same offset, so the impulse peaks there.
        let peak = delay_samples;
        assert!(
            buf[peak * 2] > 0.9,
            "left peak {} at {}",
            buf[peak * 2],
            peak
        );
        assert!(buf[peak * 2 + 1] > 0.9);
        assert!(buf[2] < 0.1, "leaked early: {}", buf[2]);
    }

    #[test]
    fn output_stays_finite_with_feedback() {
        let mut fx = FlangerEffect::new(48_000.0);
        fx.set_param(param::FEEDBACK, 0.95);
        fx.set_param(param::DEPTH, 1.0);
        fx.set_param(param::MIX, 1.0);
        let mut buf = vec![0.7_f32; 48_000 * 2];
        fx.process(&mut buf, 2);
        assert!(buf.iter().all(|s| s.is_finite()));
    }
}
