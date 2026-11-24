//! Multiband dynamics: a three-band compressor (low / mid / high) split by
//! second-order Butterworth crossovers, each band with its own threshold,
//! ratio and makeup gain, plus shared attack/release and an output trim.
//!
//! The crossovers are the RBJ biquads of [`crate::butterworth_filter`] at
//! Q = 1/sqrt(2): the low band is the low-pass at the first crossover, the
//! high band the high-pass at the second, and the mid band what remains in
//! between. Each band runs a peak envelope follower whose dB level drives a
//! standard feed-forward gain computer; the resulting gain is smoothed with
//! the shared attack (reducing) and release (recovering) coefficients so
//! parameter changes never click.

use vibez_core::effect::{DynamicsMeter, EffectType, ParamDescriptor};

use crate::butterworth_filter::ButterworthFilter;
use crate::effect::AudioEffect;

pub mod param {
    pub const XOVER_LOW_MID: usize = 0;
    pub const XOVER_MID_HIGH: usize = 1;
    pub const ATTACK: usize = 2;
    pub const RELEASE: usize = 3;
    pub const OUTPUT_DB: usize = 4;
    pub const LOW_THRESHOLD: usize = 5;
    pub const LOW_RATIO: usize = 6;
    pub const LOW_MAKEUP: usize = 7;
    pub const MID_THRESHOLD: usize = 8;
    pub const MID_RATIO: usize = 9;
    pub const MID_MAKEUP: usize = 10;
    pub const HIGH_THRESHOLD: usize = 11;
    pub const HIGH_RATIO: usize = 12;
    pub const HIGH_MAKEUP: usize = 13;
    pub const COUNT: usize = 14;
}

const BANDS: usize = 3;
const BAND_LOW: usize = 0;
const BAND_MID: usize = 1;
const BAND_HIGH: usize = 2;

pub static MULTIBAND_DYNAMICS_PARAMS: &[ParamDescriptor] = &[
    ParamDescriptor {
        name: "Xover L/M",
        min: 40.0,
        max: 2_000.0,
        default: 250.0,
        unit: "Hz",
    },
    ParamDescriptor {
        name: "Xover M/H",
        min: 500.0,
        max: 12_000.0,
        default: 3_500.0,
        unit: "Hz",
    },
    ParamDescriptor {
        name: "Attack",
        min: 0.1,
        max: 100.0,
        default: 10.0,
        unit: "ms",
    },
    ParamDescriptor {
        name: "Release",
        min: 10.0,
        max: 1_000.0,
        default: 200.0,
        unit: "ms",
    },
    ParamDescriptor {
        name: "Output",
        min: -24.0,
        max: 24.0,
        default: 0.0,
        unit: "dB",
    },
    ParamDescriptor {
        name: "Low Thresh",
        min: -60.0,
        max: 0.0,
        default: -24.0,
        unit: "dB",
    },
    ParamDescriptor {
        name: "Low Ratio",
        min: 1.0,
        max: 20.0,
        default: 4.0,
        unit: ":1",
    },
    ParamDescriptor {
        name: "Low Makeup",
        min: 0.0,
        max: 24.0,
        default: 0.0,
        unit: "dB",
    },
    ParamDescriptor {
        name: "Mid Thresh",
        min: -60.0,
        max: 0.0,
        default: -24.0,
        unit: "dB",
    },
    ParamDescriptor {
        name: "Mid Ratio",
        min: 1.0,
        max: 20.0,
        default: 4.0,
        unit: ":1",
    },
    ParamDescriptor {
        name: "Mid Makeup",
        min: 0.0,
        max: 24.0,
        default: 0.0,
        unit: "dB",
    },
    ParamDescriptor {
        name: "High Thresh",
        min: -60.0,
        max: 0.0,
        default: -18.0,
        unit: "dB",
    },
    ParamDescriptor {
        name: "High Ratio",
        min: 1.0,
        max: 20.0,
        default: 6.0,
        unit: ":1",
    },
    ParamDescriptor {
        name: "High Makeup",
        min: 0.0,
        max: 24.0,
        default: 0.0,
        unit: "dB",
    },
];

/// Smoothing coefficient for a time constant in ms at `rate`.
fn time_coefficient(ms: f32, rate: f32) -> f32 {
    (1.0 - (-1.0 / (ms.max(0.01) * 0.001 * rate)).exp()).clamp(0.0, 1.0)
}

fn db_to_amplitude(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// One band's detector and gain smoother, per channel.
#[derive(Clone, Copy)]
struct BandState {
    envelope: [f32; 2],
    gain_db: [f32; 2],
}

pub struct MultibandDynamicsEffect {
    params: [f32; param::COUNT],
    sample_rate: f32,
    lp1: ButterworthFilter,
    hp1: ButterworthFilter,
    lp2: ButterworthFilter,
    hp2: ButterworthFilter,
    bands: [BandState; BANDS],
    attack_coefficient: f32,
    release_coefficient: f32,
    output_gain: f32,
    meter: DynamicsMeter,
}

impl MultibandDynamicsEffect {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate >= 2.0 {
            sample_rate
        } else {
            48000.0
        };
        let params = std::array::from_fn(|index| MULTIBAND_DYNAMICS_PARAMS[index].default);
        let mut effect = Self {
            params,
            sample_rate,
            lp1: ButterworthFilter::EMPTY,
            hp1: ButterworthFilter::EMPTY,
            lp2: ButterworthFilter::EMPTY,
            hp2: ButterworthFilter::EMPTY,
            bands: [BandState {
                envelope: [0.0; 2],
                gain_db: [0.0; 2],
            }; BANDS],
            attack_coefficient: 0.0,
            release_coefficient: 0.0,
            output_gain: 1.0,
            meter: DynamicsMeter::default(),
        };
        effect.reconfigure();
        effect
    }

    /// Recompute everything derived from the parameters.
    fn reconfigure(&mut self) {
        let rate = self.sample_rate;
        let fc1 = self.params[param::XOVER_LOW_MID];
        let fc2 = self.params[param::XOVER_MID_HIGH].max(fc1);
        self.lp1.configure(rate, fc1, false);
        self.hp1.configure(rate, fc1, true);
        self.lp2.configure(rate, fc2, false);
        self.hp2.configure(rate, fc2, true);
        self.attack_coefficient = time_coefficient(self.params[param::ATTACK], rate);
        self.release_coefficient = time_coefficient(self.params[param::RELEASE], rate);
        self.output_gain = db_to_amplitude(self.params[param::OUTPUT_DB]);
    }

    fn band_targets(&self, band: usize) -> (f32, f32, f32) {
        let (threshold, ratio, makeup) = match band {
            BAND_LOW => (
                self.params[param::LOW_THRESHOLD],
                self.params[param::LOW_RATIO],
                self.params[param::LOW_MAKEUP],
            ),
            BAND_MID => (
                self.params[param::MID_THRESHOLD],
                self.params[param::MID_RATIO],
                self.params[param::MID_MAKEUP],
            ),
            _ => (
                self.params[param::HIGH_THRESHOLD],
                self.params[param::HIGH_RATIO],
                self.params[param::HIGH_MAKEUP],
            ),
        };
        (threshold, ratio.max(1.0), makeup)
    }

    /// Run one band sample through its detector and gain smoother.
    fn compress(&mut self, sample: f32, band: usize, channel: usize) -> f32 {
        let (threshold, ratio, makeup) = self.band_targets(band);
        let state = &mut self.bands[band];
        let envelope = &mut state.envelope[channel];
        let gain_db = &mut state.gain_db[channel];
        let level = sample.abs();
        let detector_coef = if level > *envelope {
            self.attack_coefficient
        } else {
            self.release_coefficient
        };
        *envelope += (level - *envelope) * detector_coef;
        let envelope_db = 20.0 * (*envelope + 1e-9).log10();
        let over = (envelope_db - threshold).max(0.0);
        let reduction = over * (1.0 - 1.0 / ratio);
        let meter = &mut self.meter;
        meter.current_reduction_db[channel] = reduction.max(0.0);
        meter.band_reduction_db[band] = meter.band_reduction_db[band].max(reduction.max(0.0));
        meter.peak_reduction_db[channel] = meter.peak_reduction_db[channel].max(reduction.max(0.0));
        meter.detector_peak[channel] = meter.detector_peak[channel].max(level);
        let target = makeup - reduction;
        let gain_coef = if target < *gain_db {
            self.attack_coefficient
        } else {
            self.release_coefficient
        };
        *gain_db += (target - *gain_db) * gain_coef;
        sample * db_to_amplitude(*gain_db)
    }
}

impl AudioEffect for MultibandDynamicsEffect {
    fn supports_level_meter(&self) -> bool {
        true
    }
    fn take_dynamics_meter(&mut self) -> Option<DynamicsMeter> {
        let meter = self.meter;
        self.meter.peak_reduction_db = [0.0; 2];
        self.meter.detector_peak = [0.0; 2];
        self.meter.band_reduction_db = [0.0; 3];
        Some(meter)
    }
    fn effect_type(&self) -> EffectType {
        EffectType::MultibandDynamics
    }

    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        MULTIBAND_DYNAMICS_PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        let Some(descriptor) = MULTIBAND_DYNAMICS_PARAMS.get(index) else {
            return false;
        };
        if !value.is_finite() {
            return false;
        }
        let value = value.clamp(descriptor.min, descriptor.max);
        if self.params[index] == value {
            return true;
        }
        self.params[index] = value;
        self.reconfigure();
        true
    }

    fn get_param(&self, index: usize) -> f32 {
        self.params.get(index).copied().unwrap_or(0.0)
    }

    fn process(&mut self, buffer: &mut [f32], channels: usize) {
        if channels == 0 {
            return;
        }
        let ch = channels.min(2);
        for frame in buffer.chunks_exact_mut(channels) {
            for channel in 0..ch {
                let input = frame[channel];
                let low = self.lp1.process(input, channel);
                let rest = self.hp1.process(input, channel);
                let mid = self.lp2.process(rest, channel);
                let high = self.hp2.process(rest, channel);
                let low = self.compress(low, BAND_LOW, channel);
                let mid = self.compress(mid, BAND_MID, channel);
                let high = self.compress(high, BAND_HIGH, channel);
                frame[channel] = (low + mid + high) * self.output_gain;
            }
        }
    }

    fn reset(&mut self) {
        self.lp1.reset();
        self.hp1.reset();
        self.lp2.reset();
        self.hp2.reset();
        for band in &mut self.bands {
            band.envelope = [0.0; 2];
            band.gain_db = [0.0; 2];
        }
        self.meter = DynamicsMeter::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_input_stays_silent() {
        let mut fx = MultibandDynamicsEffect::new(48_000.0);
        let mut buf = vec![0.0_f32; 512];
        fx.process(&mut buf, 2);
        assert!(buf.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn loud_low_band_is_reduced_more_than_quiet_high_band() {
        let mut fx = MultibandDynamicsEffect::new(48_000.0);
        // Low threshold tight, high threshold loose: a strong low sine
        // should come out quieter than it went in, while a weak high sine
        // passes nearly untouched.
        fx.set_param(param::LOW_THRESHOLD, -40.0);
        fx.set_param(param::LOW_RATIO, 20.0);
        fx.set_param(param::HIGH_THRESHOLD, -6.0);
        fx.set_param(param::HIGH_RATIO, 1.0);
        fx.set_param(param::ATTACK, 1.0);
        fx.set_param(param::RELEASE, 50.0);

        // Feed only the low band a loud signal and verify the output
        // amplitude settles well below the input.
        let rate = 48_000.0;
        let mut fx = MultibandDynamicsEffect::new(rate);
        fx.set_param(param::LOW_THRESHOLD, -40.0);
        fx.set_param(param::LOW_RATIO, 20.0);
        fx.set_param(param::ATTACK, 0.5);
        fx.set_param(param::RELEASE, 100.0);
        let mut peak_in = 0.0_f32;
        let mut peak_out = 0.0_f32;
        for n in 0..rate as usize * 2 {
            let t = n as f32 / rate;
            let input = (std::f32::consts::TAU * 100.0 * t).sin() * 0.9;
            let mut frame = [input, input];
            fx.process(&mut frame, 2);
            peak_in = peak_in.max(input.abs());
            if n > rate as usize {
                peak_out = peak_out.max(frame[0].abs());
            }
        }
        assert!(peak_out < peak_in * 0.5, "{peak_out} vs {peak_in}");
    }

    #[test]
    fn params_clamp_to_their_descriptors() {
        let mut fx = MultibandDynamicsEffect::new(48_000.0);
        assert!(fx.set_param(param::XOVER_MID_HIGH, 1e9));
        assert_eq!(fx.get_param(param::XOVER_MID_HIGH), 12_000.0);
        assert!(!fx.set_param(99, 1.0));
        assert!(!fx.set_param(0, f32::NAN));
    }

    #[test]
    fn reset_clears_the_band_state() {
        let mut fx = MultibandDynamicsEffect::new(48_000.0);
        let mut buf = vec![0.8_f32; 256];
        fx.process(&mut buf, 2);
        fx.reset();
        assert!(fx
            .bands
            .iter()
            .all(|b| b.envelope == [0.0; 2] && b.gain_db == [0.0; 2]));
    }

    #[test]
    fn dynamics_meter_reports_the_low_band_only() {
        let mut fx = MultibandDynamicsEffect::new(48_000.0);
        assert!(AudioEffect::supports_level_meter(&fx));
        fx.set_param(param::LOW_THRESHOLD, -40.0);
        fx.set_param(param::LOW_RATIO, 20.0);
        fx.set_param(param::HIGH_THRESHOLD, -6.0);
        fx.set_param(param::HIGH_RATIO, 1.0);
        fx.set_param(param::ATTACK, 0.5);
        fx.set_param(param::RELEASE, 100.0);
        let rate = 48_000.0;
        for n in 0..rate as usize {
            let t = n as f32 / rate;
            let input = (std::f32::consts::TAU * 100.0 * t).sin() * 0.9;
            let mut frame = [input, input];
            fx.process(&mut frame, 2);
        }
        let meter = AudioEffect::take_dynamics_meter(&mut fx).unwrap();
        assert!(
            meter.band_reduction_db[0] > 1.0,
            "{:?}",
            meter.band_reduction_db
        );
        assert!(meter.band_reduction_db[2] < meter.band_reduction_db[0]);
        assert!(meter.peak_reduction_db[0] > 1.0);
        // Draining resets the peaks.
        let drained = AudioEffect::take_dynamics_meter(&mut fx).unwrap();
        assert_eq!(drained.band_reduction_db, [0.0; 3]);
    }
}
