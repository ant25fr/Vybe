//! The Plucked String: a Karplus-Strong voice per note. A noise burst fills a
//! delay line tuned to the note's pitch, then recirculates through a one-pole
//! low-pass whose cutoff is Damping and whose length Stretch pulls slightly
//! sharp (inharmonicity), while an ADSR shapes the amplitude. Each note takes
//! a free voice, or else the oldest.

use crate::pitch_bend::PitchBend;
use vibez_core::effect::ParamDescriptor;
use vibez_core::midi::InstrumentKind;

use crate::envelope::{Envelope, Times};
use crate::poly::PolyVoice;

/// The most voices a note can take.
pub const MAX_VOICES: usize = 16;
/// The lowest MIDI note, whose delay line is the longest.
const LOWEST_HZ: f32 = 8.175_8; // C0

pub mod param {
    pub const ATTACK: usize = 0;
    pub const DECAY: usize = 1;
    pub const SUSTAIN: usize = 2;
    pub const RELEASE: usize = 3;
    pub const DAMPING: usize = 4;
    pub const STRETCH: usize = 5;
    pub const VELOCITY: usize = 6;
    pub const VOICES: usize = 7;
    pub const VOLUME_DB: usize = 8;
    pub const COUNT: usize = 9;
}

pub static PLUCKED_PARAMS: &[ParamDescriptor] = &[
    ParamDescriptor {
        name: "Attack",
        min: 0.001,
        max: 0.2,
        default: 0.005,
        unit: "s",
    },
    ParamDescriptor {
        name: "Decay",
        min: 0.02,
        max: 5.0,
        default: 0.4,
        unit: "s",
    },
    ParamDescriptor {
        name: "Sustain",
        min: 0.0,
        max: 1.0,
        default: 0.6,
        unit: "",
    },
    ParamDescriptor {
        name: "Release",
        min: 0.02,
        max: 5.0,
        default: 0.25,
        unit: "s",
    },
    ParamDescriptor {
        name: "Damping",
        min: 0.0,
        max: 1.0,
        default: 0.6,
        unit: "",
    },
    ParamDescriptor {
        name: "Stretch",
        min: 0.0,
        max: 1.0,
        default: 0.0,
        unit: "",
    },
    ParamDescriptor {
        name: "Velocity",
        min: 0.0,
        max: 1.0,
        default: 0.8,
        unit: "",
    },
    ParamDescriptor {
        name: "Voices",
        min: 1.0,
        max: MAX_VOICES as f32,
        default: 8.0,
        unit: "",
    },
    ParamDescriptor {
        name: "Volume",
        min: -60.0,
        max: 6.0,
        default: -3.0,
        unit: "dB",
    },
];

/// How fast the volume follows an edit, so a moved knob does not click.
const SMOOTHING_SECONDS: f32 = 0.005;

/// The note's frequency in Hz.
fn frequency(pitch: u8) -> f32 {
    440.0 * 2.0_f32.powf((pitch as f32 - 69.0) / 12.0)
}

/// Deterministic noise for the pluck burst, so renders replay exactly: a
/// xorshift seeded from the note and its age.
fn noise(seed: u64) -> impl FnMut() -> f32 {
    let mut state = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(0x1234_5678_9ABC_DEF0)
        | 1;
    move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        // Map to -1..1.
        ((state >> 11) as f32 / (1u64 << 53) as f32) * 2.0 - 1.0
    }
}

#[derive(Clone)]
struct Voice {
    /// The recirculating delay line.
    ring: Vec<f32>,
    write: usize,
    /// Whole part of the loop delay in samples (the note's period plus
    /// Stretch) and the linear-interpolation weight of its fraction.
    delay_int: usize,
    delay_frac: f32,
    /// The unbent loop delay in samples.
    period: f64,
    /// One-pole low-pass state inside the feedback loop.
    loop_lp: f32,
    env: Envelope,
    pitch: u8,
    velocity: f32,
    age: u64,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            ring: Vec::new(),
            write: 0,
            delay_int: 1,
            delay_frac: 0.0,
            period: 1.0,
            loop_lp: 0.0,
            env: Envelope::default(),
            pitch: 0,
            velocity: 0.0,
            age: 0,
        }
    }
}

impl PolyVoice for Voice {
    fn pitch(&self) -> u8 {
        self.pitch
    }
    fn age(&self) -> u64 {
        self.age
    }
    fn active(&self) -> bool {
        self.env.active()
    }
    fn released(&self) -> bool {
        self.env.released()
    }
}

impl Voice {
    fn prepare(&mut self, sample_rate: f32) {
        let capacity = (sample_rate / LOWEST_HZ) as usize + 4;
        if self.ring.len() != capacity {
            self.ring = vec![0.0; capacity];
            self.write = 0;
            self.loop_lp = 0.0;
        }
    }

    /// Retunes the loop to the note's period bent by `semitones`.
    fn bend(&mut self, semitones: f32) {
        let longest = (self.ring.len().max(3) - 2) as f64;
        let delay = (self.period * f64::from(-semitones / 12.0).exp2()).clamp(1.0, longest);
        self.delay_int = delay.floor() as usize;
        self.delay_frac = (delay - delay.floor()) as f32;
    }

    fn start(
        &mut self,
        pitch: u8,
        velocity: u8,
        age: u64,
        sample_rate: f32,
        stretch: f32,
        bend: f32,
    ) {
        self.prepare(sample_rate);
        self.pitch = pitch;
        self.age = age;
        self.velocity = velocity as f32 / 127.0;
        self.period = f64::from(sample_rate) / f64::from(frequency(pitch)) + f64::from(stretch);
        self.bend(bend);
        // The pluck: a burst of noise shaped by the velocity. Only the loop's
        // own length is excited (the tail of the ring, which is what the first
        // reads wrap round to); the rest is written before it is ever read.
        let excited = (self.delay_int + 2).min(self.ring.len());
        let start = self.ring.len() - excited;
        let mut burst = noise(age ^ u64::from(pitch) << 32);
        for sample in &mut self.ring[start..] {
            *sample = burst() * self.velocity;
        }
        self.write = 0;
        self.loop_lp = 0.0;
        self.env.trigger();
    }

    fn release(&mut self) {
        self.env.release();
    }

    /// One sample of the string, with the envelope applied.
    fn generate(&mut self, times: &Times, k: f32) -> f32 {
        let len = self.ring.len();
        if len == 0 {
            return 0.0;
        }
        // Read the delay back, linearly interpolated for the stretch; the
        // index arithmetic stays in integers.
        let mut i0 = self.write + len - self.delay_int;
        if i0 >= len {
            i0 -= len;
        }
        let i1 = if i0 + 1 == len { 0 } else { i0 + 1 };
        let y = self.ring[i0] + (self.ring[i1] - self.ring[i0]) * self.delay_frac;
        // Recirculate through the damping low-pass.
        self.loop_lp += k * (y - self.loop_lp);
        self.ring[self.write] = self.loop_lp;
        self.write += 1;
        if self.write == len {
            self.write = 0;
        }
        y * self.env.tick(times)
    }
}

pub struct PluckedString {
    voices: [Voice; MAX_VOICES],
    sample_rate: f32,
    params: [f32; param::COUNT],
    /// The smoothed volume's current value; the rest read `params`.
    volume: f32,
    /// Linear gain of `volume`, recomputed only while the volume moves.
    gain: f32,
    times: Times,
    smoothing: f32,
    started: bool,
    age: u64,
    bend: PitchBend,
    /// The bend the voices are tuned to.
    tuned: f32,
}

impl PluckedString {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let params: [f32; param::COUNT] =
            std::array::from_fn(|index| PLUCKED_PARAMS[index].default);
        let mut synth = Self {
            voices: std::array::from_fn(|_| Voice::default()),
            sample_rate,
            volume: params[param::VOLUME_DB],
            gain: 10f32.powf(params[param::VOLUME_DB] / 20.0),
            params,
            times: Times::new(0.0, 0.0, 0.0, 0.0, sample_rate),
            smoothing: 1.0 - (-1.0 / (SMOOTHING_SECONDS * sample_rate)).exp(),
            started: false,
            age: 0,
            bend: PitchBend::new(sample_rate),
            tuned: 0.0,
        };
        synth.retime();
        synth
    }

    fn retime(&mut self) {
        let p = &self.params;
        self.times = Times::new(
            p[param::ATTACK],
            p[param::DECAY],
            p[param::SUSTAIN],
            p[param::RELEASE],
            self.sample_rate,
        );
    }

    pub fn set_param(&mut self, index: usize, value: f32) -> bool {
        let Some(descriptor) = PLUCKED_PARAMS.get(index) else {
            return false;
        };
        if !value.is_finite() {
            return false;
        }
        let value = if index == param::VOICES {
            value.round()
        } else {
            value
        }
        .clamp(descriptor.min, descriptor.max);
        self.params[index] = value;
        if index == param::VOLUME_DB && !self.started {
            self.volume = value;
            self.gain = 10f32.powf(value / 20.0);
        }
        self.retime();
        true
    }

    pub fn get_param(&self, index: usize) -> f32 {
        self.params.get(index).copied().unwrap_or(0.0)
    }

    pub fn note_on(&mut self, pitch: u8, velocity: u8) {
        if velocity == 0 {
            self.note_off(pitch);
            return;
        }
        self.age += 1;
        let count = (self.params[param::VOICES] as usize).clamp(1, MAX_VOICES);
        let index = crate::poly::choose(&self.voices[..count], pitch);
        let stretch = self.params[param::STRETCH];
        self.voices[index].start(
            pitch,
            velocity,
            self.age,
            self.sample_rate,
            stretch,
            self.tuned,
        );
    }

    pub fn note_off(&mut self, pitch: u8) {
        for voice in &mut self.voices {
            if voice.active() && voice.pitch == pitch {
                voice.release();
            }
        }
    }

    fn sample(&mut self) -> f32 {
        self.started = true;
        let target = self.params[param::VOLUME_DB];
        if self.volume != target {
            self.volume += (target - self.volume) * self.smoothing;
            if (target - self.volume).abs() < 1e-4 {
                self.volume = target;
            }
            self.gain = 10f32.powf(self.volume / 20.0);
        }
        let bend = self.bend.tick();
        if bend != self.tuned {
            self.tuned = bend;
            for voice in &mut self.voices {
                voice.bend(bend);
            }
        }
        let k = 1.0 - 0.99 * self.params[param::DAMPING].clamp(0.0, 1.0);
        let velocity_sense = self.params[param::VELOCITY];
        let mut sum = 0.0;
        for voice in &mut self.voices {
            if voice.active() {
                let gain = voice.velocity * (1.0 - velocity_sense)
                    + velocity_sense * (0.4 + 0.6 * voice.velocity);
                sum += voice.generate(&self.times, k) * gain;
            }
        }
        sum * self.gain
    }

    /// Adds the instrument's output to every channel of `buffer`; notes
    /// arrive through `note_on` and `note_off` beforehand.
    pub fn render(&mut self, buffer: &mut [f32], channels: usize) {
        // An idle string adds nothing; skip the per-sample work entirely and
        // let the volume settle, which is all `sample` would have done.
        if !self.voices.iter().any(Voice::active) {
            if self.started {
                self.volume = self.params[param::VOLUME_DB];
                self.gain = 10f32.powf(self.volume / 20.0);
            }
            return;
        }
        for frame in buffer.chunks_exact_mut(channels.max(1)) {
            let sample = self.sample();
            for channel in frame {
                *channel += sample;
            }
        }
    }

    pub fn reset(&mut self) {
        for voice in &mut self.voices {
            *voice = Voice::default();
        }
        self.volume = self.params[param::VOLUME_DB];
        self.gain = 10f32.powf(self.volume / 20.0);
    }
}

impl crate::Instrument for PluckedString {
    fn activation_sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate as u32)
    }

    fn instrument_kind(&self) -> InstrumentKind {
        InstrumentKind::PluckedString
    }

    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        PLUCKED_PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        self.set_param(index, value)
    }

    fn get_param(&self, index: usize) -> f32 {
        self.get_param(index)
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        self.note_on(pitch, velocity);
    }

    fn note_off(&mut self, pitch: u8) {
        self.note_off(pitch);
    }

    fn set_pitch_bend(&mut self, semitones: f32) {
        self.bend.set(semitones);
        // A silent string takes the bend at once, ready for the next pluck.
        if !self.voices.iter().any(Voice::active) {
            self.bend.snap();
            self.tuned = self.bend.tick();
        }
    }

    fn render(&mut self, buffer: &mut [f32], channels: usize) {
        self.render(buffer, channels);
    }

    fn reset(&mut self) {
        self.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_produces_a_decaying_pitched_sound() {
        let sr = 48_000.0;
        let mut plucked = PluckedString::new(sr);
        plucked.note_on(69, 100); // A4 = 440 Hz
        let mut buf = vec![0.0_f32; sr as usize];
        plucked.render(&mut buf, 1);
        // It sounds at first...
        let head_peak = buf[..4_800].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(head_peak > 0.05, "head peak {head_peak}");
        // ...and the string decays.
        let tail_peak = buf[buf.len() - 4_800..]
            .iter()
            .fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(
            tail_peak < head_peak,
            "tail {tail_peak} vs head {head_peak}"
        );
        // The pitch: the autocorrelation peak over 100..1000 Hz lands on
        // the string's period (the raw burst is noisy, so zero crossings
        // overcount).
        let x = &buf[4_800..24_000];
        let mut best_lag = 0_usize;
        let mut best = 0.0_f32;
        for lag in (sr / 1_000.0) as usize..(sr / 100.0) as usize {
            let mut sum = 0.0_f32;
            for i in 0..x.len() - lag {
                sum += x[i] * x[i + lag];
            }
            if sum > best {
                best = sum;
                best_lag = lag;
            }
        }
        let est_hz = sr / best_lag as f32;
        assert!(
            (est_hz - 440.0).abs() < 25.0,
            "estimated {est_hz} Hz (lag {best_lag}), expected ~440 Hz"
        );
    }

    #[test]
    fn note_off_releases_and_silences() {
        let sr = 48_000.0;
        let mut plucked = PluckedString::new(sr);
        plucked.set_param(param::RELEASE, 0.02);
        plucked.note_on(60, 127);
        let mut buf = vec![0.0_f32; 4_800];
        plucked.render(&mut buf, 1);
        plucked.note_off(60);
        let mut buf = vec![0.0_f32; 48_000];
        plucked.render(&mut buf, 1);
        let tail_peak = buf[24_000..].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(tail_peak < 1e-3, "still ringing: {tail_peak}");
    }

    #[test]
    fn params_clamp_and_reject_garbage() {
        let mut plucked = PluckedString::new(48_000.0);
        assert!(plucked.set_param(param::DAMPING, 5.0));
        assert_eq!(plucked.get_param(param::DAMPING), 1.0);
        assert!(!plucked.set_param(param::DAMPING, f32::NAN));
        assert!(!plucked.set_param(param::COUNT, 1.0));
    }

    #[test]
    fn more_damping_decays_faster() {
        let sr = 48_000.0;
        let render_tail = |damping: f32| {
            let mut plucked = PluckedString::new(sr);
            plucked.set_param(param::DAMPING, damping);
            plucked.set_param(param::SUSTAIN, 1.0);
            plucked.note_on(69, 127);
            let mut buf = vec![0.0_f32; sr as usize];
            plucked.render(&mut buf, 1);
            buf[sr as usize / 2..]
                .iter()
                .fold(0.0_f32, |m, s| m.max(s.abs()))
        };
        assert!(render_tail(0.95) < render_tail(0.05));
    }
}
