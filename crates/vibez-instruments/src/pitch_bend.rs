//! The pitch wheel as an instrument hears it: glided over a few
//! milliseconds so the controller's steps do not zipper.

/// How far the wheel bends at either end, in semitones (the usual default
/// of hardware and of other instruments' bend ranges).
pub const PITCH_BEND_RANGE_SEMITONES: f32 = 2.0;
const GLIDE_SECONDS: f32 = 0.005;

#[derive(Debug, Clone, Copy)]
pub(crate) struct PitchBend {
    target: f32,
    value: f32,
    coefficient: f32,
}

impl PitchBend {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            target: 0.0,
            value: 0.0,
            coefficient: 1.0 - (-1.0 / (GLIDE_SECONDS * sample_rate)).exp(),
        }
    }

    pub fn set(&mut self, semitones: f32) {
        if semitones.is_finite() {
            self.target = semitones.clamp(-48.0, 48.0);
        }
    }

    /// Jumps to the target, for an instrument with nothing sounding.
    pub fn snap(&mut self) {
        self.value = self.target;
    }

    /// The bend for the next sample, in semitones.
    pub fn tick(&mut self) -> f32 {
        if self.value != self.target {
            self.value += (self.target - self.value) * self.coefficient;
            if (self.target - self.value).abs() < 1e-3 {
                self.value = self.target;
            }
        }
        self.value
    }
}
