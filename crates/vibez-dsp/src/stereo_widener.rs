//! Stereo Widener: mid/side processing. The side signal is scaled by Width
//! (1 is untouched, 0 collapses to mono, above 1 exaggerates the sides), and
//! optionally everything below Bass Mono is pulled out of the side so the
//! low end stays centred.

use vibez_core::effect::{EffectType, ParamDescriptor};

use crate::effect::AudioEffect;

pub mod param {
    pub const WIDTH: usize = 0;
    pub const BASS_MONO: usize = 1;
    pub const COUNT: usize = 2;
}

pub static STEREO_WIDENER_PARAMS: &[ParamDescriptor] = &[
    ParamDescriptor {
        name: "Width",
        min: 0.0,
        max: 2.0,
        default: 1.0,
        unit: "",
    },
    ParamDescriptor {
        name: "Bass Mono",
        min: 0.0,
        max: 400.0,
        default: 0.0,
        unit: "Hz",
    },
];

/// A second-order Butterworth high-pass (RBJ, Q = 1/sqrt(2)) on the side
/// signal: what it lets through is the side without the bass, which is what
/// Bass Mono keeps.
struct SideHighpass {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl SideHighpass {
    fn new() -> Self {
        let mut filter = Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        };
        filter.set_cutoff(0.0, 48_000.0);
        filter
    }

    /// New coefficients for cutoff `hz`, or all-pass bypass (b0 = 1) when
    /// `hz` is 0, i.e. Bass Mono off. The filter's memory is kept, so
    /// sweeping or automating the cutoff does not click.
    fn set_cutoff(&mut self, hz: f32, sample_rate: f32) {
        if hz <= 0.0 {
            (self.b0, self.b1, self.b2, self.a1, self.a2) = (1.0, 0.0, 0.0, 0.0, 0.0);
            return;
        }
        let w0 = std::f32::consts::TAU * hz / sample_rate;
        let alpha = w0.sin() * std::f32::consts::FRAC_1_SQRT_2;
        let a0 = 1.0 + alpha;
        let cos_w0 = w0.cos();
        self.b0 = (1.0 + cos_w0) * 0.5 / a0;
        self.b1 = -(1.0 + cos_w0) / a0;
        self.b2 = (1.0 + cos_w0) * 0.5 / a0;
        self.a1 = -2.0 * cos_w0 / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn clear(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

pub struct StereoWidenerEffect {
    params: [f32; param::COUNT],
    /// High-pass of the side signal, rebuilt when Bass Mono changes.
    side_lp: SideHighpass,
    side_lp_hz: f32,
    sample_rate: f32,
}

impl StereoWidenerEffect {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate >= 1.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            params: std::array::from_fn(|index| STEREO_WIDENER_PARAMS[index].default),
            side_lp: SideHighpass::new(),
            side_lp_hz: 0.0,
            sample_rate,
        }
    }
}

impl AudioEffect for StereoWidenerEffect {
    fn supports_level_meter(&self) -> bool {
        true
    }

    fn activation_sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate as u32)
    }

    fn effect_type(&self) -> EffectType {
        EffectType::StereoWidener
    }

    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        STEREO_WIDENER_PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        let Some(descriptor) = STEREO_WIDENER_PARAMS.get(index) else {
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
        if channels < 2 {
            // Mono in, mono out: nothing to widen.
            return;
        }
        let width = self.params[param::WIDTH];
        let bass_mono = self.params[param::BASS_MONO];
        if (bass_mono - self.side_lp_hz).abs() > 1e-6 {
            self.side_lp.set_cutoff(bass_mono, self.sample_rate);
            self.side_lp_hz = bass_mono;
        }
        for frame in buffer.chunks_exact_mut(channels) {
            let left = frame[0];
            let right = frame[1];
            let mid = (left + right) * 0.5;
            // Strip the bass out of the side so it stays centred.
            let mut side = self.side_lp.process((left - right) * 0.5);
            side *= width;
            frame[0] = mid + side;
            frame[1] = mid - side;
            for sample in frame.iter_mut().take(2) {
                if !sample.is_finite() {
                    *sample = 0.0;
                }
            }
        }
    }

    fn reset(&mut self) {
        self.side_lp.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_one_is_identity() {
        let mut fx = StereoWidenerEffect::new(48_000.0);
        let orig: Vec<f32> = (0..512)
            .flat_map(|i| {
                let t = i as f32 * 0.1;
                [t.sin() * 0.8, t.cos() * 0.6]
            })
            .collect();
        let mut buf = orig.clone();
        fx.process(&mut buf, 2);
        for (a, b) in buf.iter().zip(orig.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn width_zero_is_mono() {
        let mut fx = StereoWidenerEffect::new(48_000.0);
        fx.set_param(param::WIDTH, 0.0);
        let mut buf = vec![0.0_f32; 64];
        for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
            frame[0] = i as f32 * 0.1;
            frame[1] = 1.0 - i as f32 * 0.1;
        }
        fx.process(&mut buf, 2);
        for frame in buf.chunks_exact(2) {
            assert!((frame[0] - frame[1]).abs() < 1e-6);
        }
    }

    #[test]
    fn bass_mono_keeps_low_end_centred() {
        let sr = 48_000.0;
        let mut fx = StereoWidenerEffect::new(sr);
        fx.set_param(param::WIDTH, 2.0);
        fx.set_param(param::BASS_MONO, 200.0);
        // A slow (50 Hz) fully-panned signal: left only.
        let mut buf = vec![0.0_f32; sr as usize * 2];
        for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
            frame[0] = (i as f32 * std::f32::consts::TAU * 50.0 / sr).sin();
        }
        fx.process(&mut buf, 2);
        // After the filter settles, left and right must carry the same low
        // signal (side removed), i.e. the output is mono for this input.
        let tail = &buf[(sr as usize / 2) * 2..];
        let max_side = tail
            .chunks_exact(2)
            .map(|f| (f[0] - f[1]).abs())
            .fold(0.0_f32, f32::max);
        // A second-order high-pass at 200 Hz leaves 50 Hz down ~24 dB.
        assert!(max_side < 0.2, "side not removed: {max_side}");
    }

    #[test]
    fn moving_bass_mono_keeps_the_filter_state() {
        // A steady low side tone through the filter while the cutoff is
        // nudged: the output must stay as smooth as it was.
        let sr = 48_000.0;
        let mut fx = StereoWidenerEffect::new(sr);
        fx.set_param(param::BASS_MONO, 100.0);
        let tone = |i: usize| (i as f32 * std::f32::consts::TAU * 300.0 / sr).sin() * 0.5;
        let frames = |from: usize, count: usize| {
            let mut buf = vec![0.0_f32; count * 2];
            for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
                frame[0] = tone(from + i);
                frame[1] = -tone(from + i);
            }
            buf
        };
        let mut settle = frames(0, 9_600);
        fx.process(&mut settle, 2);
        let step = |buf: &[f32]| {
            buf.chunks_exact(2)
                .map(|f| f[0])
                .collect::<Vec<_>>()
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0_f32, f32::max)
        };
        let steady = step(&settle[settle.len() - 2_000..]);
        let mut after = frames(9_600, 64);
        fx.set_param(param::BASS_MONO, 110.0);
        fx.process(&mut after, 2);
        // The first frame is compared with the last settled one.
        let mut joined = vec![settle[settle.len() - 2], after[0]];
        joined.extend(after.chunks_exact(2).map(|f| f[0]).skip(1));
        let jump = joined
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f32, f32::max);
        assert!(jump < steady * 3.0, "step {jump} against a steady {steady}");
    }
}
