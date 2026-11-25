//! Frequency Shifter: single-sideband modulation. The signal is split into a
//! delayed copy and a Hilbert-transformed (90-degree) copy, then recombined
//! with a sine/cosine pair, which moves every frequency up or down by Shift
//! without changing the spacing between them (unlike a pitch shift).

use vibez_core::effect::{EffectType, ParamDescriptor};

use crate::effect::AudioEffect;

pub mod param {
    pub const SHIFT: usize = 0;
    pub const MIX: usize = 1;
    pub const COUNT: usize = 2;
}

/// Hilbert transformer length; odd, centred on a group delay of TAPS/2.
const TAPS: usize = 63;
const GROUP_DELAY: usize = (TAPS - 1) / 2;

pub static FREQUENCY_SHIFTER_PARAMS: &[ParamDescriptor] = &[
    ParamDescriptor {
        name: "Shift",
        min: -2_000.0,
        max: 2_000.0,
        default: 0.0,
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

/// Hamming-windowed Hilbert FIR: 2/(pi*k) on the odd taps around the centre.
fn hilbert_coeffs() -> [f32; TAPS] {
    let centre = GROUP_DELAY as f32;
    let mut coeffs = [0.0_f32; TAPS];
    for (n, coeff) in coeffs.iter_mut().enumerate() {
        let k = n as f32 - centre;
        if (k as i64) % 2 != 0 {
            let window = 0.54 - 0.46 * (std::f32::consts::TAU * n as f32 / (TAPS - 1) as f32).cos();
            *coeff = -2.0 / (std::f32::consts::PI * k) * window;
        }
    }
    coeffs
}

/// One channel's input history.
struct History {
    buf: Vec<f32>,
    write: usize,
}

impl History {
    fn new() -> Self {
        Self {
            buf: vec![0.0; TAPS],
            write: 0,
        }
    }

    fn push(&mut self, value: f32) {
        self.buf[self.write] = value;
        self.write += 1;
        if self.write == self.buf.len() {
            self.write = 0;
        }
    }

    /// Sample at `delay` positions behind the newest written sample.
    fn read_back(&self, delay: usize) -> f32 {
        let index = (self.write + self.buf.len() - 1 - delay % self.buf.len()) % self.buf.len();
        self.buf[index]
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
        self.write = 0;
    }
}

pub struct FrequencyShifterEffect {
    params: [f32; param::COUNT],
    /// One history per channel, so interleaved channels never share a FIR.
    channels: Vec<History>,
    coeffs: [f32; TAPS],
    phase: f32,
    sample_rate: f32,
}

impl FrequencyShifterEffect {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate >= 1.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            params: std::array::from_fn(|index| FREQUENCY_SHIFTER_PARAMS[index].default),
            channels: vec![History::new(), History::new()],
            coeffs: hilbert_coeffs(),
            phase: 0.0,
            sample_rate,
        }
    }
}

impl AudioEffect for FrequencyShifterEffect {
    fn supports_level_meter(&self) -> bool {
        true
    }

    fn activation_sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate as u32)
    }

    /// Everything passes the Hilbert transformer's group delay.
    fn latency_samples(&self) -> u32 {
        GROUP_DELAY as u32
    }

    fn effect_type(&self) -> EffectType {
        EffectType::FrequencyShifter
    }

    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        FREQUENCY_SHIFTER_PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        let Some(descriptor) = FREQUENCY_SHIFTER_PARAMS.get(index) else {
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
        let shift = self.params[param::SHIFT];
        let mix = self.params[param::MIX];
        let sign = if shift >= 0.0 { 1.0 } else { -1.0 };
        let inc = std::f32::consts::TAU * shift.abs() / self.sample_rate;
        while self.channels.len() < channels {
            self.channels.push(History::new());
        }
        for frame in buffer.chunks_exact_mut(channels) {
            // One carrier phase per frame, shared by every channel, so the
            // shift does not depend on the channel count.
            let (sin, cos) = self.phase.sin_cos();
            for (sample, history) in frame.iter_mut().zip(self.channels.iter_mut()) {
                history.push(*sample);
                // Time-aligned dry copy (delayed by the FIR's group delay).
                let dry = history.read_back(GROUP_DELAY);
                // Hilbert-transformed copy, aligned the same way.
                let mut analytic = 0.0_f32;
                for (k, coeff) in self.coeffs.iter().enumerate() {
                    if *coeff != 0.0 {
                        analytic += coeff * history.read_back(TAPS - 1 - k);
                    }
                }
                let shifted = dry * cos - sign * analytic * sin;
                *sample = dry * (1.0 - mix) + shifted * mix;
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
        for history in &mut self.channels {
            history.clear();
        }
        self.phase = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_shift_is_a_plain_delay() {
        let mut fx = FrequencyShifterEffect::new(48_000.0);
        fx.set_param(param::SHIFT, 0.0);
        let mut buf = vec![0.0_f32; 256];
        buf[0] = 1.0;
        fx.process(&mut buf, 1);
        // Impulse reappears GROUP_DELAY samples later, nowhere before.
        assert!(buf[..GROUP_DELAY].iter().all(|&s| s == 0.0));
        assert!((buf[GROUP_DELAY] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn positive_shift_moves_a_sine_up() {
        let sr = 48_000.0;
        let mut fx = FrequencyShifterEffect::new(sr);
        fx.set_param(param::SHIFT, 1_000.0);
        fx.set_param(param::MIX, 1.0);
        let input_hz = 100.0;
        let mut buf = vec![0.0_f32; sr as usize];
        for (i, s) in buf.iter_mut().enumerate() {
            *s = (i as f32 * std::f32::consts::TAU * input_hz / sr).sin() * 0.8;
        }
        fx.process(&mut buf, 1);
        let tail = &buf[sr as usize / 2..];
        let crossings = tail
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let est_hz = crossings as f32 * 2.0; // tail is half a second
        assert!(
            (est_hz - 1_100.0).abs() < 60.0,
            "estimated {est_hz} Hz, expected ~1100 Hz"
        );
    }

    #[test]
    fn negative_shift_moves_a_sine_down() {
        let sr = 48_000.0;
        let mut fx = FrequencyShifterEffect::new(sr);
        fx.set_param(param::SHIFT, -50.0);
        fx.set_param(param::MIX, 1.0);
        let input_hz = 200.0;
        let mut buf = vec![0.0_f32; sr as usize];
        for (i, s) in buf.iter_mut().enumerate() {
            *s = (i as f32 * std::f32::consts::TAU * input_hz / sr).sin() * 0.8;
        }
        fx.process(&mut buf, 1);
        let tail = &buf[sr as usize / 2..];
        let crossings = tail
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let est_hz = crossings as f32 * 2.0; // tail is half a second
        assert!(
            (est_hz - 150.0).abs() < 30.0,
            "estimated {est_hz} Hz, expected ~150 Hz"
        );
    }

    #[test]
    fn stereo_shifts_by_the_set_amount_on_both_channels() {
        let sr = 48_000.0;
        let mut fx = FrequencyShifterEffect::new(sr);
        fx.set_param(param::SHIFT, 500.0);
        fx.set_param(param::MIX, 1.0);
        let n = sr as usize;
        let mut buf = vec![0.0_f32; n * 2];
        for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
            let s = (i as f32 * std::f32::consts::TAU * 1_000.0 / sr).sin() * 0.5;
            frame[0] = s;
            frame[1] = s;
        }
        fx.process(&mut buf, 2);
        for channel in 0..2 {
            let crossings = buf
                .chunks_exact(2)
                .skip(n / 2)
                .map(|f| f[channel])
                .collect::<Vec<_>>()
                .windows(2)
                .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
                .count();
            let est_hz = crossings as f32 * 2.0;
            assert!(
                (est_hz - 1_500.0).abs() < 40.0,
                "channel {channel}: {est_hz} Hz, expected ~1500 Hz"
            );
        }
    }
}
