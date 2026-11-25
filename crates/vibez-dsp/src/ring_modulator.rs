//! Ring Modulator: multiplies the signal by a sine carrier, which splits every
//! input frequency into a sum and a difference pair around the carrier.

use vibez_core::effect::{EffectType, ParamDescriptor};

use crate::effect::AudioEffect;

pub mod param {
    pub const FREQUENCY: usize = 0;
    pub const MIX: usize = 1;
    pub const COUNT: usize = 2;
}

pub static RING_MODULATOR_PARAMS: &[ParamDescriptor] = &[
    ParamDescriptor {
        name: "Frequency",
        min: 10.0,
        max: 8_000.0,
        default: 440.0,
        unit: "Hz",
    },
    ParamDescriptor {
        name: "Mix",
        min: 0.0,
        max: 1.0,
        default: 1.0,
        unit: "",
    },
];

pub struct RingModulatorEffect {
    params: [f32; param::COUNT],
    phase: f32,
    sample_rate: f32,
}

impl RingModulatorEffect {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate >= 1.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            params: std::array::from_fn(|index| RING_MODULATOR_PARAMS[index].default),
            phase: 0.0,
            sample_rate,
        }
    }
}

impl AudioEffect for RingModulatorEffect {
    fn supports_level_meter(&self) -> bool {
        true
    }

    fn activation_sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate as u32)
    }

    fn effect_type(&self) -> EffectType {
        EffectType::RingModulator
    }

    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        RING_MODULATOR_PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        let Some(descriptor) = RING_MODULATOR_PARAMS.get(index) else {
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
        let frequency = self.params[param::FREQUENCY];
        let mix = self.params[param::MIX];
        let inc = std::f32::consts::TAU * frequency / self.sample_rate;
        for frame in buffer.chunks_exact_mut(channels) {
            // One carrier value per frame, shared by every channel, so the
            // carrier's frequency does not depend on the channel count.
            let carrier = self.phase.sin();
            for sample in frame.iter_mut() {
                let dry = *sample;
                *sample = dry * (1.0 - mix) + dry * carrier * mix;
                if !sample.is_finite() {
                    *sample = 0.0;
                }
            }
            self.phase += inc;
            if self.phase >= std::f32::consts::TAU {
                self.phase -= std::f32::consts::TAU;
            }
        }
    }

    fn reset(&mut self) {
        self.phase = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_zero_is_identity() {
        let mut fx = RingModulatorEffect::new(48_000.0);
        fx.set_param(param::MIX, 0.0);
        let orig = vec![0.4_f32; 512];
        let mut buf = orig.clone();
        fx.process(&mut buf, 2);
        for (a, b) in buf.iter().zip(orig.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn shifts_a_sine_up_by_the_carrier() {
        let sr = 48_000.0;
        let mut fx = RingModulatorEffect::new(sr);
        fx.set_param(param::FREQUENCY, 1_000.0);
        fx.set_param(param::MIX, 1.0);
        let input_hz = 100.0;
        let mut buf = vec![0.0_f32; sr as usize * 2];
        for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
            let s = (i as f32 * std::f32::consts::TAU * input_hz / sr).sin() * 0.8;
            frame[0] = s;
            frame[1] = s;
        }
        fx.process(&mut buf, 2);
        // 100 Hz ring-modulated at 1 kHz yields 900 Hz and 1100 Hz, and no
        // 100 Hz left; compare Goertzel power at the three frequencies.
        let tail: Vec<f32> = buf[(sr as usize / 2) * 2..]
            .chunks_exact(2)
            .map(|f| f[0])
            .collect();
        let power = |hz: f64| {
            let w = std::f64::consts::TAU * hz / f64::from(sr);
            let (mut re, mut im) = (0.0_f64, 0.0_f64);
            for (i, &x) in tail.iter().enumerate() {
                re += f64::from(x) * (w * i as f64).cos();
                im -= f64::from(x) * (w * i as f64).sin();
            }
            re * re + im * im
        };
        let p100 = power(100.0);
        assert!(
            power(900.0) > p100 * 1_000.0 && power(1_100.0) > p100 * 1_000.0,
            "p100={p100} p900={} p1100={}",
            power(900.0),
            power(1_100.0)
        );
    }
}
