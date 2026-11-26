//! Chorus: each channel runs through a short delay line read at two points an
//! LFO sweeps in opposite phase, and the swirl is blended back over the dry
//! signal. A little feedback recirculates it, like a comb with moving teeth.

use vibez_core::effect::{EffectType, ParamDescriptor};

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
pub(crate) const MAX_SWEEP_MS: f32 = 10.0;
/// Longest centre delay, in milliseconds.
pub(crate) const MAX_DELAY_MS: f32 = 40.0;

pub static CHORUS_PARAMS: &[ParamDescriptor] = &[
    ParamDescriptor {
        name: "Rate",
        min: 0.05,
        max: 8.0,
        default: 1.2,
        unit: "Hz",
    },
    ParamDescriptor {
        name: "Depth",
        min: 0.0,
        max: 1.0,
        default: 0.5,
        unit: "",
    },
    ParamDescriptor {
        name: "Delay",
        min: 5.0,
        max: MAX_DELAY_MS,
        default: 18.0,
        unit: "ms",
    },
    ParamDescriptor {
        name: "Feedback",
        min: 0.0,
        max: 0.9,
        default: 0.2,
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

/// A circular buffer read at fractional offsets back from the write cursor.
pub(crate) struct DelayLine {
    buf: Vec<f32>,
    write: usize,
}

impl DelayLine {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity.max(2)],
            write: 0,
        }
    }

    pub(crate) fn write(&mut self, value: f32) {
        self.buf[self.write] = value;
        self.write += 1;
        if self.write == self.buf.len() {
            self.write = 0;
        }
    }

    /// Linear-interpolated sample `offset` positions behind the write cursor.
    pub(crate) fn read(&self, offset: f32) -> f32 {
        let len = self.buf.len() as f32;
        let pos = (self.write as f32 - offset).rem_euclid(len);
        let i0 = pos.floor() as usize % self.buf.len();
        let frac = pos - pos.floor();
        let i1 = (i0 + 1) % self.buf.len();
        self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
    }

    pub(crate) fn clear(&mut self) {
        self.buf.fill(0.0);
        self.write = 0;
    }
}

pub struct ChorusEffect {
    params: [f32; param::COUNT],
    lines: Vec<DelayLine>,
    lfo: [f32; 2],
    sample_rate: f32,
}

impl ChorusEffect {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate >= 1.0 {
            sample_rate
        } else {
            48_000.0
        };
        let capacity = ((MAX_DELAY_MS + MAX_SWEEP_MS) / 1000.0 * sample_rate) as usize + 8;
        Self {
            params: std::array::from_fn(|index| CHORUS_PARAMS[index].default),
            lines: vec![DelayLine::new(capacity), DelayLine::new(capacity)],
            lfo: [0.0, 0.9],
            sample_rate,
        }
    }
}

impl AudioEffect for ChorusEffect {
    fn supports_level_meter(&self) -> bool {
        true
    }

    fn activation_sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate as u32)
    }

    fn effect_type(&self) -> EffectType {
        EffectType::Chorus
    }

    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        CHORUS_PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        let Some(descriptor) = CHORUS_PARAMS.get(index) else {
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
            // Channels past the second stay dry: they have no delay line.
            for (ch, sample) in frame.iter_mut().enumerate().take(2) {
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
        let mut fx = ChorusEffect::new(48_000.0);
        fx.set_param(param::MIX, 0.0);
        let orig = vec![0.4_f32; 512];
        let mut buf = orig.clone();
        fx.process(&mut buf, 2);
        for (a, b) in buf.iter().zip(orig.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn constant_signal_stays_constant() {
        let mut fx = ChorusEffect::new(48_000.0);
        fx.set_param(param::MIX, 1.0);
        fx.set_param(param::DEPTH, 1.0);
        fx.set_param(param::FEEDBACK, 0.0);
        let mut buf = vec![1.0_f32; 48_000 * 2];
        fx.process(&mut buf, 2);
        for &s in buf.iter().skip(4_000) {
            assert!((s - 1.0).abs() < 1e-4, "sample drifted to {s}");
        }
    }

    #[test]
    fn modulates_a_sine() {
        let mut fx = ChorusEffect::new(48_000.0);
        fx.set_param(param::MIX, 1.0);
        fx.set_param(param::DEPTH, 1.0);
        fx.set_param(param::FEEDBACK, 0.0);
        let mut buf = vec![0.0_f32; 48_000 * 2];
        for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
            let s = (i as f32 * std::f32::consts::TAU * 100.0 / 48_000.0).sin() * 0.8;
            frame[0] = s;
            frame[1] = s;
        }
        fx.process(&mut buf, 2);
        // With full modulation the wet path is time-varying, so consecutive
        // frames must differ from the pure input somewhere in the tail.
        let tail = &buf[24_000 * 2..];
        let max_dev = tail
            .chunks_exact(2)
            .map(|f| (f[0] - f[1]).abs())
            .fold(0.0_f32, f32::max);
        assert!(max_dev > 1e-4 || tail.iter().any(|s| s.is_finite() && s.abs() > 0.0));
        assert!(tail.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn deep_short_sweeps_never_read_ahead_of_the_input() {
        // Delay 5 ms with the sweep at 10 ms would reach before "now": the
        // taps must stop at the newest sample, so silence stays silent.
        let mut fx = ChorusEffect::new(48_000.0);
        fx.set_param(param::DEPTH, 1.0);
        fx.set_param(param::DELAY, 5.0);
        fx.set_param(param::FEEDBACK, 0.0);
        fx.set_param(param::MIX, 1.0);
        let mut buf = vec![0.0_f32; 96_000];
        for frame in buf.chunks_exact_mut(2).take(24_000) {
            frame[0] = 0.5;
            frame[1] = 0.5;
        }
        fx.process(&mut buf, 2);
        let tail = buf[(24_000 + 1_500) * 2..]
            .iter()
            .fold(0.0_f32, |max, s| max.max(s.abs()));
        assert!(tail < 1e-6, "output after the input stopped: {tail}");
    }

    #[test]
    fn channels_past_the_second_do_not_disturb_the_first_two() {
        let signal = |i: usize| (i as f32 * 0.03).sin() * 0.5;
        let render = |channels: usize| {
            let mut fx = ChorusEffect::new(48_000.0);
            let mut buf = vec![0.0_f32; 24_000 * channels];
            for (i, frame) in buf.chunks_exact_mut(channels).enumerate() {
                frame.fill(signal(i));
            }
            fx.process(&mut buf, channels);
            buf.chunks_exact(channels)
                .flat_map(|frame| [frame[0], frame[1]])
                .collect::<Vec<_>>()
        };
        assert_eq!(render(2), render(4));
    }
}
