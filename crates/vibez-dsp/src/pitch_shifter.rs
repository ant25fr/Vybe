//! Pitch Shifter: overlap-add of two grains read from the input history.
//! Grain m starts at input position `m * hop` and reads forward at `ratio`
//! for `hop` output samples, so at grain boundaries the read pointer jumps
//! back by `hop * (ratio - 1)`; the crossfade between the old and new grain
//! hides the jump. At ratio 1 both grains read the current sample, so 0
//! semitones is transparent apart from the two-grain lookback delay.

use vibez_core::effect::{EffectType, ParamDescriptor};

use crate::effect::AudioEffect;

pub mod param {
    pub const SEMITONES: usize = 0;
    pub const FINE: usize = 1;
    pub const GRAIN: usize = 2;
    pub const COUNT: usize = 3;
}

/// Longest grain, in milliseconds; also the lookback (two half-grains).
const MAX_GRAIN_MS: f32 = 200.0;

pub static PITCH_SHIFTER_PARAMS: &[ParamDescriptor] = &[
    ParamDescriptor {
        name: "Semitones",
        min: -12.0,
        max: 12.0,
        default: 0.0,
        unit: "st",
    },
    ParamDescriptor {
        name: "Fine",
        min: -100.0,
        max: 100.0,
        default: 0.0,
        unit: "ct",
    },
    ParamDescriptor {
        name: "Grain",
        min: 20.0,
        max: MAX_GRAIN_MS,
        default: 80.0,
        unit: "ms",
    },
];

/// One channel's grain reader over the input history.
struct Voice {
    buf: Vec<f32>,
    written: u64,
    /// Input position where the current grain starts; advances one `hop`
    /// per grain, so it tracks the output position minus the lookback.
    start: f64,
    /// How far the grains read behind the newest input, in samples: fixed,
    /// whatever the Grain, so the effect's latency is a constant it can
    /// declare.
    lookback: f64,
    /// Output position inside the current grain's crossfade.
    t: f64,
    /// Crossfade length in output samples (half the grain).
    hop: f64,
}

impl Voice {
    fn new(sample_rate: f32) -> Self {
        // Lookback is one grain, and an up-shift reads up to one more grain
        // ahead within the crossfade.
        let capacity = ((MAX_GRAIN_MS * 2.0 / 1000.0) + 0.05) * sample_rate;
        let lookback = (MAX_GRAIN_MS / 1000.0 * sample_rate) as f64;
        Self {
            buf: vec![0.0; (capacity as usize).max(4096)],
            written: 0,
            start: -lookback,
            lookback,
            t: 0.0,
            hop: 1_000.0,
        }
    }

    fn write(&mut self, value: f32) {
        let index = (self.written as usize) % self.buf.len();
        self.buf[index] = value;
        self.written += 1;
    }

    /// Linear-interpolated input sample at absolute position `pos`, clamped
    /// to what has been written.
    fn read_abs(&self, pos: f64) -> f32 {
        let newest = self.written.saturating_sub(1) as f64;
        let pos = pos.clamp(0.0, newest);
        let len = self.buf.len() as f64;
        let p = pos.rem_euclid(len);
        let i0 = p.floor() as usize % self.buf.len();
        let frac = (p - p.floor()) as f32;
        let i1 = (i0 + 1) % self.buf.len();
        self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
    }

    /// One output sample from `input`; `ratio` is the playback rate and
    /// `hop` the crossfade length in output samples.
    fn process_sample(&mut self, input: f32, ratio: f64, hop: f64) -> f32 {
        self.write(input);
        while self.t >= hop {
            self.start += hop;
            self.t -= hop;
        }
        let frac = (self.t / hop).clamp(0.0, 1.0) as f32;
        // The current grain reads from `start`, the previous one from
        // `start - hop`; both advance at `ratio` inside the crossfade.
        let cur = self.start + self.t * ratio;
        let prev = self.start - hop + (self.t + hop) * ratio;
        let out = self.read_abs(cur) * (1.0 - frac) + self.read_abs(prev) * frac;
        self.t += 1.0;
        out
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
        self.written = 0;
        self.start = -self.lookback;
        self.t = 0.0;
    }
}

pub struct PitchShifterEffect {
    params: [f32; param::COUNT],
    voices: Vec<Voice>,
    sample_rate: f32,
}

impl PitchShifterEffect {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate >= 1.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            params: std::array::from_fn(|index| PITCH_SHIFTER_PARAMS[index].default),
            voices: vec![Voice::new(sample_rate), Voice::new(sample_rate)],
            sample_rate,
        }
    }
}

impl AudioEffect for PitchShifterEffect {
    fn supports_level_meter(&self) -> bool {
        true
    }

    fn activation_sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate as u32)
    }

    /// The grains read a fixed longest-grain behind the write cursor, so
    /// the output lags the input by exactly that, at any Grain.
    fn latency_samples(&self) -> u32 {
        (MAX_GRAIN_MS / 1000.0 * self.sample_rate) as u32
    }

    fn effect_type(&self) -> EffectType {
        EffectType::PitchShifter
    }

    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        PITCH_SHIFTER_PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        let Some(descriptor) = PITCH_SHIFTER_PARAMS.get(index) else {
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
        let semitones = self.params[param::SEMITONES] as f64;
        let fine = self.params[param::FINE] as f64;
        let ratio = 2.0_f64.powf((semitones + fine / 100.0) / 12.0);
        let hop =
            (self.params[param::GRAIN] as f64 * 0.001 * self.sample_rate as f64 * 0.5).max(8.0);
        let voices = &mut self.voices;
        for frame in buffer.chunks_exact_mut(channels) {
            for (ch, sample) in frame.iter_mut().enumerate() {
                let voice = &mut voices[ch.min(1)];
                // The lookback is fixed, so a new Grain only changes how often
                // the grains crossfade; the output does not jump.
                voice.hop = hop;
                let input = *sample;
                *sample = voice.process_sample(input, ratio, hop);
                if !sample.is_finite() {
                    *sample = 0.0;
                }
            }
        }
    }

    fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_shift_is_transparent() {
        let sr = 48_000.0;
        let mut fx = PitchShifterEffect::new(sr);
        let orig: Vec<f32> = (0..sr as usize)
            .map(|i| (i as f32 * std::f32::consts::TAU * 100.0 / sr).sin() * 0.8)
            .collect();
        // Interleave as stereo with the same signal on both sides.
        let mut stereo = Vec::with_capacity(orig.len() * 2);
        for &s in &orig {
            stereo.push(s);
            stereo.push(s);
        }
        fx.process(&mut stereo, 2);
        // At ratio 1 both grains read the same sample, so the output equals
        // the input delayed by the fixed lookback the effect declares.
        let delay = fx.latency_samples() as usize;
        for n in delay..orig.len() {
            assert!(
                (stereo[n * 2] - orig[n - delay]).abs() < 1e-4,
                "frame {n}: {} vs {}",
                stereo[n * 2],
                orig[n - delay]
            );
        }
    }

    #[test]
    fn the_declared_latency_holds_at_every_grain() {
        let sr = 48_000.0;
        for grain in [20.0, 80.0, 200.0] {
            let mut fx = PitchShifterEffect::new(sr);
            fx.set_param(param::GRAIN, grain);
            let delay = fx.latency_samples() as usize;
            let mut buf = vec![0.0_f32; (delay + 4_000) * 2];
            buf[0] = 1.0;
            buf[1] = 1.0;
            fx.process(&mut buf, 2);
            let peak = buf
                .chunks_exact(2)
                .enumerate()
                .max_by(|a, b| a.1[0].abs().total_cmp(&b.1[0].abs()))
                .map(|(i, _)| i)
                .unwrap();
            assert!(
                peak.abs_diff(delay) <= 1,
                "grain {grain}: impulse at {peak}, declared {delay}"
            );
        }
    }

    #[test]
    fn twelve_semitones_up_doubles_the_frequency() {
        let sr = 48_000.0;
        let mut fx = PitchShifterEffect::new(sr);
        fx.set_param(param::SEMITONES, 12.0);
        let input_hz = 100.0;
        let mut buf = vec![0.0_f32; sr as usize * 2];
        for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
            let s = (i as f32 * std::f32::consts::TAU * input_hz / sr).sin() * 0.8;
            frame[0] = s;
            frame[1] = s;
        }
        fx.process(&mut buf, 2);
        let tail = &buf[(sr as usize / 2) * 2..];
        let samples: Vec<f32> = tail.chunks_exact(2).map(|f| f[0]).collect();
        let crossings = samples
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let est_hz = crossings as f32 * 2.0; // tail is half a second
        assert!(
            (est_hz - 200.0).abs() < 40.0,
            "estimated {est_hz} Hz, expected ~200 Hz"
        );
    }

    #[test]
    fn output_stays_finite_across_the_range() {
        let sr = 48_000.0;
        for semitones in [-12.0, -7.0, 0.0, 3.0, 12.0] {
            let mut fx = PitchShifterEffect::new(sr);
            fx.set_param(param::SEMITONES, semitones);
            let mut buf = vec![0.6_f32; 24_000 * 2];
            fx.process(&mut buf, 2);
            assert!(buf.iter().all(|s| s.is_finite()));
        }
    }
}
