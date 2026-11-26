//! Modulators: a track's LFOs, envelopes and random sources, the parameters
//! they move, and the value each one has at a given moment.
//!
//! A modulator offsets a parameter around its set value, as in Bitwig, and
//! never overwrites it.

use serde::{Deserialize, Serialize};

use crate::automation::AutomationTarget;

/// Modulators one track can hold, and parameters each one can move. Fixed, so
/// the audio thread never grows storage for them.
pub const MAX_MODULATORS: usize = 4;
pub const MAX_MODULATION_LINKS: usize = 8;

pub const MIN_RATE_HZ: f32 = 0.01;
pub const MAX_RATE_HZ: f32 = 40.0;
pub const MAX_ENVELOPE_MS: f32 = 10_000.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModulatorKind {
    #[default]
    Lfo,
    /// ADSR gated by the track's playing notes.
    Envelope,
    /// A new random value every cycle.
    Random,
}

impl ModulatorKind {
    pub const ALL: [Self; 3] = [Self::Lfo, Self::Envelope, Self::Random];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lfo => "LFO",
            Self::Envelope => "Envelope",
            Self::Random => "Random",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModShape {
    #[default]
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
}

impl ModShape {
    pub const ALL: [Self; 5] = [
        Self::Sine,
        Self::Triangle,
        Self::SawUp,
        Self::SawDown,
        Self::Square,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Sine => "Sine",
            Self::Triangle => "Triangle",
            Self::SawUp => "Saw Up",
            Self::SawDown => "Saw Down",
            Self::Square => "Square",
        }
    }

    /// Value in -1..1 at `phase` of a cycle.
    pub fn value(self, phase: f64) -> f32 {
        let phase = phase.rem_euclid(1.0) as f32;
        match self {
            Self::Sine => (phase * std::f32::consts::TAU).sin(),
            Self::Triangle => {
                if phase < 0.25 {
                    phase * 4.0
                } else if phase < 0.75 {
                    2.0 - phase * 4.0
                } else {
                    phase * 4.0 - 4.0
                }
            }
            Self::SawUp => phase * 2.0 - 1.0,
            Self::SawDown => 1.0 - phase * 2.0,
            Self::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }
}

/// How long one cycle lasts when it follows the tempo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModDivision {
    /// Free: the rate is in hertz.
    #[default]
    Free,
    FourBars,
    TwoBars,
    OneBar,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
}

impl ModDivision {
    pub const ALL: [Self; 8] = [
        Self::Free,
        Self::FourBars,
        Self::TwoBars,
        Self::OneBar,
        Self::Half,
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::FourBars => "4/1",
            Self::TwoBars => "2/1",
            Self::OneBar => "1/1",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
        }
    }

    /// Cycle length in quarter notes; `None` when free.
    pub fn beats(self) -> Option<f64> {
        match self {
            Self::Free => None,
            Self::FourBars => Some(16.0),
            Self::TwoBars => Some(8.0),
            Self::OneBar => Some(4.0),
            Self::Half => Some(2.0),
            Self::Quarter => Some(1.0),
            Self::Eighth => Some(0.5),
            Self::Sixteenth => Some(0.25),
        }
    }
}

/// The settings of one modulator, without what it moves.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModulatorSettings {
    pub kind: ModulatorKind,
    pub enabled: bool,
    pub shape: ModShape,
    pub division: ModDivision,
    pub rate_hz: f32,
    /// Start of the cycle, 0..1.
    pub phase: f32,
    /// How much a random value glides into the next one, 0..1.
    pub smooth: f32,
    pub attack_ms: f32,
    pub decay_ms: f32,
    /// Sustain level, 0..1.
    pub sustain: f32,
    pub release_ms: f32,
}

impl Default for ModulatorSettings {
    fn default() -> Self {
        Self {
            kind: ModulatorKind::Lfo,
            enabled: true,
            shape: ModShape::Sine,
            division: ModDivision::Free,
            rate_hz: 1.0,
            phase: 0.0,
            smooth: 0.0,
            attack_ms: 10.0,
            decay_ms: 300.0,
            sustain: 0.5,
            release_ms: 300.0,
        }
    }
}

impl ModulatorSettings {
    pub fn of(kind: ModulatorKind) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    /// Every field brought into its range.
    pub fn clamped(self) -> Self {
        let finite = |value: f32, low: f32, high: f32, fallback: f32| {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                fallback
            }
        };
        Self {
            rate_hz: finite(self.rate_hz, MIN_RATE_HZ, MAX_RATE_HZ, 1.0),
            phase: finite(self.phase, 0.0, 1.0, 0.0),
            smooth: finite(self.smooth, 0.0, 1.0, 0.0),
            attack_ms: finite(self.attack_ms, 0.0, MAX_ENVELOPE_MS, 10.0),
            decay_ms: finite(self.decay_ms, 0.0, MAX_ENVELOPE_MS, 300.0),
            sustain: finite(self.sustain, 0.0, 1.0, 0.5),
            release_ms: finite(self.release_ms, 0.0, MAX_ENVELOPE_MS, 300.0),
            ..self
        }
    }

    /// Cycles elapsed at a moment given as song beats and seconds.
    pub fn cycles(&self, beat: f64, seconds: f64) -> f64 {
        let elapsed = match self.division.beats() {
            Some(length) => beat / length,
            None => seconds * f64::from(self.rate_hz),
        };
        elapsed + f64::from(self.phase)
    }

    /// Output of an LFO (-1..1) or random source (-1..1) after `cycles`.
    /// `seed` tells random sources apart. An envelope has no value here.
    pub fn cycle_value(&self, cycles: f64, seed: u64) -> f32 {
        match self.kind {
            ModulatorKind::Lfo => self.shape.value(cycles),
            ModulatorKind::Random => {
                let cycle = cycles.floor();
                let current = random_step(seed, cycle as i64);
                if self.smooth <= 0.0 {
                    return current;
                }
                // The glide takes the first `smooth` share of the cycle.
                let progress = ((cycles - cycle) as f32 / self.smooth).min(1.0);
                let previous = random_step(seed, cycle as i64 - 1);
                previous + (current - previous) * progress
            }
            ModulatorKind::Envelope => 0.0,
        }
    }
}

/// The same value for a seed and cycle every time, so a render repeats what
/// playback did.
fn random_step(seed: u64, cycle: i64) -> f32 {
    let mut state = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(cycle as u64);
    state = (state ^ (state >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    state = (state ^ (state >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    state ^= state >> 31;
    ((state >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
}

/// One parameter a modulator moves, by `amount` of its range (-1..1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModulationLink {
    pub target: AutomationTarget,
    pub amount: f32,
}

/// Whether the engine can modulate `target`. Plugin parameters, sends, mute,
/// swing and tempo cannot be.
pub fn can_modulate(target: AutomationTarget) -> bool {
    matches!(
        target,
        AutomationTarget::TrackGain
            | AutomationTarget::TrackPan
            | AutomationTarget::EffectParam { .. }
            | AutomationTarget::InstrumentParam { .. }
    )
}

/// A modulator as a project keeps it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Modulator {
    #[serde(flatten)]
    pub settings: ModulatorSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<ModulationLink>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_stay_in_range_and_start_where_expected() {
        for shape in ModShape::ALL {
            for step in 0..100 {
                let value = shape.value(f64::from(step) / 100.0);
                assert!((-1.0..=1.0).contains(&value), "{shape:?} {value}");
            }
        }
        assert_eq!(ModShape::Triangle.value(0.25), 1.0);
        assert_eq!(ModShape::SawUp.value(0.0), -1.0);
        assert_eq!(ModShape::Square.value(1.75), -1.0);
    }

    #[test]
    fn a_synced_cycle_follows_beats_and_a_free_one_seconds() {
        let mut settings = ModulatorSettings {
            rate_hz: 2.0,
            ..Default::default()
        };
        assert_eq!(settings.cycles(8.0, 3.0), 6.0);
        settings.division = ModDivision::OneBar;
        assert_eq!(settings.cycles(8.0, 3.0), 2.0);
    }

    #[test]
    fn random_values_repeat_and_glide() {
        let mut settings = ModulatorSettings::of(ModulatorKind::Random);
        let held = settings.cycle_value(3.2, 7);
        assert_eq!(held, settings.cycle_value(3.9, 7));
        assert_ne!(held, settings.cycle_value(4.2, 7));
        assert!((-1.0..=1.0).contains(&held));
        settings.smooth = 1.0;
        let previous = random_step(7, 2);
        assert!((settings.cycle_value(3.0, 7) - previous).abs() < 1e-6);
    }

    #[test]
    fn settings_clamp_and_round_trip() {
        let settings = ModulatorSettings {
            rate_hz: f32::NAN,
            sustain: 3.0,
            ..Default::default()
        }
        .clamped();
        assert_eq!((settings.rate_hz, settings.sustain), (1.0, 1.0));
        let modulator = Modulator {
            settings,
            links: vec![ModulationLink {
                target: AutomationTarget::TrackPan,
                amount: -0.5,
            }],
        };
        let json = serde_json::to_string(&modulator).unwrap();
        assert_eq!(serde_json::from_str::<Modulator>(&json).unwrap(), modulator);
    }
}
