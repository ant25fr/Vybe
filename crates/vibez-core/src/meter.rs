//! Time signatures: how quarter-note beats group into bars along a timeline.

use serde::{Deserialize, Serialize};

/// Signature changes one timeline can hold. Fixed, so a [`MeterMap`] moves to
/// the audio thread without allocating.
pub const MAX_METER_CHANGES: usize = 32;

/// Beats are quarter notes everywhere; a signature only groups them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "(u8, u8)", into = "(u8, u8)")]
pub struct TimeSignature {
    numerator: u8,
    denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

impl TimeSignature {
    pub const MAX_NUMERATOR: u8 = 32;

    /// A signature with 1 to 32 beats of a whole, half, quarter, eighth or
    /// sixteenth note.
    pub fn new(numerator: u8, denominator: u8) -> Option<Self> {
        ((1..=Self::MAX_NUMERATOR).contains(&numerator)
            && matches!(denominator, 1 | 2 | 4 | 8 | 16))
        .then_some(Self {
            numerator,
            denominator,
        })
    }

    /// Text such as "7/8".
    pub fn parse(text: &str) -> Option<Self> {
        let (numerator, denominator) = text.trim().split_once('/')?;
        Self::new(
            numerator.trim().parse().ok()?,
            denominator.trim().parse().ok()?,
        )
    }

    pub fn numerator(self) -> u8 {
        self.numerator
    }

    pub fn denominator(self) -> u8 {
        self.denominator
    }

    /// Length of one counted beat, in quarter notes.
    pub fn beat_length(self) -> f64 {
        4.0 / f64::from(self.denominator)
    }

    /// Length of one bar, in quarter notes.
    pub fn bar_length(self) -> f64 {
        f64::from(self.numerator) * self.beat_length()
    }
}

impl std::fmt::Display for TimeSignature {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/{}", self.numerator, self.denominator)
    }
}

impl TryFrom<(u8, u8)> for TimeSignature {
    type Error = String;

    fn try_from((numerator, denominator): (u8, u8)) -> Result<Self, String> {
        Self::new(numerator, denominator)
            .ok_or_else(|| format!("Unsupported time signature {numerator}/{denominator}"))
    }
}

impl From<TimeSignature> for (u8, u8) {
    fn from(signature: TimeSignature) -> Self {
        (signature.numerator, signature.denominator)
    }
}

/// A signature taking effect at the start of a bar, counted from 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeterChange {
    pub bar: u32,
    pub signature: TimeSignature,
}

/// Where a beat falls among the bars.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarPosition {
    /// Bar index, counted from 0.
    pub bar: u32,
    /// Beat on which that bar starts.
    pub start_beat: f64,
    pub signature: TimeSignature,
}

/// The signatures of a timeline, always starting with one at bar 0.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(from = "Vec<MeterChange>", into = "Vec<MeterChange>")]
pub struct MeterMap {
    changes: [MeterChange; MAX_METER_CHANGES],
    len: usize,
}

impl Default for MeterMap {
    fn default() -> Self {
        Self {
            changes: [MeterChange::default(); MAX_METER_CHANGES],
            len: 1,
        }
    }
}

impl PartialEq for MeterMap {
    fn eq(&self, other: &Self) -> bool {
        self.changes() == other.changes()
    }
}

impl Eq for MeterMap {}

impl From<Vec<MeterChange>> for MeterMap {
    fn from(changes: Vec<MeterChange>) -> Self {
        let mut map = Self::default();
        for change in changes {
            map.set_change(change.bar, change.signature);
        }
        map
    }
}

impl From<MeterMap> for Vec<MeterChange> {
    fn from(map: MeterMap) -> Self {
        map.changes().to_vec()
    }
}

/// Absorbs float error, so a beat a hair under a bar line counts as on it.
const EPSILON: f64 = 1e-6;

impl MeterMap {
    pub fn changes(&self) -> &[MeterChange] {
        &self.changes[..self.len]
    }

    /// Whether the whole timeline is in 4/4.
    pub fn is_default(&self) -> bool {
        self.changes() == Self::default().changes()
    }

    pub fn signature_at_bar(&self, bar: u32) -> TimeSignature {
        self.changes()
            .iter()
            .rev()
            .find(|change| change.bar <= bar)
            .map_or_else(TimeSignature::default, |change| change.signature)
    }

    pub fn has_change_at(&self, bar: u32) -> bool {
        self.changes().iter().any(|change| change.bar == bar)
    }

    /// Put `signature` into effect from `bar`. Returns whether the map
    /// changed; a full map refuses a new bar.
    pub fn set_change(&mut self, bar: u32, signature: TimeSignature) -> bool {
        let index = self.changes().partition_point(|change| change.bar < bar);
        if index < self.len && self.changes[index].bar == bar {
            if self.changes[index].signature == signature {
                return false;
            }
            self.changes[index].signature = signature;
            return true;
        }
        if self.len == MAX_METER_CHANGES || self.signature_at_bar(bar) == signature {
            return false;
        }
        self.changes.copy_within(index..self.len, index + 1);
        self.changes[index] = MeterChange { bar, signature };
        self.len += 1;
        true
    }

    /// Remove the change at `bar`. Bar 0 always keeps a signature.
    pub fn remove_change(&mut self, bar: u32) -> bool {
        let Some(index) = self.changes().iter().position(|change| change.bar == bar) else {
            return false;
        };
        if index == 0 {
            return false;
        }
        self.changes.copy_within(index + 1..self.len, index);
        self.len -= 1;
        self.changes[self.len] = MeterChange::default();
        true
    }

    /// Beat on which `bar` starts.
    pub fn bar_start(&self, bar: u32) -> f64 {
        let mut beat = 0.0;
        for (index, change) in self.changes().iter().enumerate() {
            let next = self
                .changes()
                .get(index + 1)
                .map_or(u32::MAX, |next| next.bar);
            let bars = bar.min(next).saturating_sub(change.bar);
            beat += f64::from(bars) * change.signature.bar_length();
            if bar <= next {
                break;
            }
        }
        beat
    }

    /// The bar holding `beat`.
    pub fn position_at(&self, beat: f64) -> BarPosition {
        let beat = beat.max(0.0) + EPSILON;
        let mut start = 0.0;
        for (index, change) in self.changes().iter().enumerate() {
            let length = change.signature.bar_length();
            if let Some(next) = self.changes().get(index + 1) {
                let end = start + f64::from(next.bar - change.bar) * length;
                if beat >= end {
                    start = end;
                    continue;
                }
            }
            let bars = ((beat - start) / length).floor();
            return BarPosition {
                bar: change.bar.saturating_add(bars as u32),
                start_beat: start + bars * length,
                signature: change.signature,
            };
        }
        BarPosition {
            bar: 0,
            start_beat: 0.0,
            signature: TimeSignature::default(),
        }
    }

    /// Whether `beat` is a bar line.
    pub fn is_bar_line(&self, beat: f64) -> bool {
        (beat - self.position_at(beat).start_beat).abs() < 1e-4
    }

    /// A position as bar, beat and sixteenth counted from 1, such as
    /// "33.1.1".
    pub fn position_label(&self, beat: f64) -> String {
        let position = self.position_at(beat);
        let inside = (beat.max(0.0) - position.start_beat).max(0.0) + EPSILON;
        let unit = position.signature.beat_length();
        let counted = (inside / unit).floor();
        let sixteenth = ((inside - counted * unit) * 4.0).floor();
        format!(
            "{}.{}.{}",
            u64::from(position.bar) + 1,
            counted as u64 + 1,
            sixteenth as u64 + 1
        )
    }
}

#[cfg(test)]
#[path = "meter_tests.rs"]
mod tests;
