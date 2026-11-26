//! A track's MIDI effects, ahead of its instrument: Chord, then Scale, then
//! the Arpeggiator. [`MidiProcessor`] runs them without allocating.

use serde::{Deserialize, Serialize};

use crate::scale::{ChordKind, Scale, CHORD_NOTES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArpRate {
    Quarter,
    Eighth,
    #[default]
    Sixteenth,
    SixteenthTriplet,
    ThirtySecond,
}

impl ArpRate {
    pub const ALL: [ArpRate; 5] = [
        ArpRate::Quarter,
        ArpRate::Eighth,
        ArpRate::Sixteenth,
        ArpRate::SixteenthTriplet,
        ArpRate::ThirtySecond,
    ];

    pub fn beats(self) -> f64 {
        match self {
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::SixteenthTriplet => 1.0 / 6.0,
            Self::ThirtySecond => 0.125,
        }
    }
}

impl std::fmt::Display for ArpRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::SixteenthTriplet => "1/16T",
            Self::ThirtySecond => "1/32",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArpPattern {
    #[default]
    Up,
    Down,
    UpDown,
    Random,
}

impl ArpPattern {
    pub const ALL: [ArpPattern; 4] = [
        ArpPattern::Up,
        ArpPattern::Down,
        ArpPattern::UpDown,
        ArpPattern::Random,
    ];
}

impl std::fmt::Display for ArpPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::UpDown => "Up / down",
            Self::Random => "Random",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arpeggiator {
    pub rate: ArpRate,
    pub pattern: ArpPattern,
    /// Octaves the held notes are walked over, 1 to 4.
    pub octaves: u8,
    /// Each step's note length as a percentage of the step, 1 to 100.
    pub gate: u8,
}

impl Default for Arpeggiator {
    fn default() -> Self {
        Self {
            rate: ArpRate::default(),
            pattern: ArpPattern::default(),
            octaves: 1,
            gate: 50,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MidiEffects {
    #[serde(default)]
    pub chord: ChordKind,
    #[serde(default)]
    pub scale: Option<Scale>,
    #[serde(default)]
    pub arpeggiator: Option<Arpeggiator>,
}

impl MidiEffects {
    /// Whether notes pass through untouched.
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }
}

/// What the effects send on to the instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiOut {
    On { pitch: u8, velocity: u8 },
    Off { pitch: u8 },
}

/// The running state of one track's [`MidiEffects`].
#[derive(Debug, Clone)]
pub struct MidiProcessor {
    settings: MidiEffects,
    /// The notes each held input key stands for.
    held: [([u8; CHORD_NOTES], u8); 128],
    /// How many held keys ask for each note, and the latest velocity.
    wanted: [(u8, u8); 128],
    /// Arpeggiator: frames to the next step and to the sounding note's end.
    next_step: Option<u32>,
    note_end: Option<u32>,
    sounding: Option<u8>,
    step: u32,
    random: u32,
    sample_rate: f32,
    bpm: f64,
    /// The song position in samples while the transport plays.
    position: Option<u64>,
}

impl Default for MidiProcessor {
    fn default() -> Self {
        Self {
            settings: MidiEffects::default(),
            held: [([0; CHORD_NOTES], 0); 128],
            wanted: [(0, 0); 128],
            next_step: None,
            note_end: None,
            sounding: None,
            step: 0,
            random: 0x2545_F491,
            sample_rate: 48_000.0,
            bpm: 120.0,
            position: None,
        }
    }
}

impl MidiProcessor {
    pub fn settings(&self) -> MidiEffects {
        self.settings
    }

    /// Whether the arpeggiator times notes itself.
    pub fn arpeggiating(&self) -> bool {
        self.settings.arpeggiator.is_some()
    }

    pub fn set_clock(&mut self, sample_rate: f32, bpm: f64) {
        if sample_rate > 0.0 && bpm > 0.0 {
            self.sample_rate = sample_rate;
            self.bpm = bpm;
        }
    }

    /// Where the song is as the next frames render, or `None` while it is
    /// stopped. Steps then fall on the song's grid.
    pub fn set_position(&mut self, position: Option<u64>) {
        self.position = position;
    }

    /// Frames from the song position to the next step of `rate`: zero on the
    /// grid, `None` while stopped.
    fn frames_to_grid(&self, rate: ArpRate) -> Option<u32> {
        let position = self.position? as f64;
        let step = self.step_frames(rate);
        let past = position % step;
        // Within a sample of a line counts as on it.
        Some(if past < 1.0 || step - past < 1.0 {
            0
        } else {
            (step - past).round() as u32
        })
    }

    fn step_frames(&self, rate: ArpRate) -> f64 {
        (rate.beats() * 60.0 / self.bpm * f64::from(self.sample_rate)).max(1.0)
    }

    /// New settings. Everything sounding is released first, so no note is
    /// left to a key-up that would now mean something else.
    pub fn set(&mut self, settings: MidiEffects, out: &mut impl FnMut(MidiOut)) {
        if settings != self.settings {
            self.release_all(out);
            self.settings = settings;
        }
    }

    pub fn release_all(&mut self, out: &mut impl FnMut(MidiOut)) {
        if self.arpeggiating() {
            if let Some(pitch) = self.sounding {
                out(MidiOut::Off { pitch });
            }
        } else {
            for (pitch, (count, _)) in self.wanted.iter().enumerate() {
                if *count > 0 {
                    out(MidiOut::Off { pitch: pitch as u8 });
                }
            }
        }
        self.clear();
    }

    /// Forgets every note without a word, as the instrument resets.
    pub fn clear(&mut self) {
        self.held = [([0; CHORD_NOTES], 0); 128];
        self.wanted = [(0, 0); 128];
        self.next_step = None;
        self.note_end = None;
        self.sounding = None;
        self.step = 0;
    }

    pub fn note_on(&mut self, pitch: u8, velocity: u8, out: &mut impl FnMut(MidiOut)) {
        let pitch = pitch.min(127);
        if velocity == 0 {
            return self.note_off(pitch, out);
        }
        if self.held[usize::from(pitch)].1 > 0 {
            self.note_off(pitch, out);
        }
        let (mut notes, count) = self.settings.chord.notes(pitch, self.settings.scale);
        if let Some(scale) = self.settings.scale {
            for note in &mut notes[..count] {
                *note = scale.snap(*note);
            }
        }
        self.held[usize::from(pitch)] = (notes, count as u8);
        let arpeggiating = self.arpeggiating();
        for &note in &notes[..count] {
            let entry = &mut self.wanted[usize::from(note)];
            entry.0 = entry.0.saturating_add(1);
            entry.1 = velocity;
            if !arpeggiating {
                out(MidiOut::On {
                    pitch: note,
                    velocity,
                });
            }
        }
        if let Some(arp) = self.settings.arpeggiator {
            if self.next_step.is_none() {
                self.next_step = Some(self.frames_to_grid(arp.rate).unwrap_or(0));
                self.step = 0;
            }
        }
    }

    pub fn note_off(&mut self, pitch: u8, out: &mut impl FnMut(MidiOut)) {
        let (notes, count) = std::mem::take(&mut self.held[usize::from(pitch.min(127))]);
        for &note in &notes[..usize::from(count)] {
            let entry = &mut self.wanted[usize::from(note)];
            entry.0 = entry.0.saturating_sub(1);
            if entry.0 == 0 && !self.arpeggiating() {
                out(MidiOut::Off { pitch: note });
            }
        }
        // The sounding step ends on its own; no new one starts.
        if self.arpeggiating() && self.wanted.iter().all(|(count, _)| *count == 0) {
            self.next_step = None;
        }
    }

    /// Frames until the arpeggiator has something to do.
    pub fn frames_to_event(&self) -> Option<u32> {
        match (self.next_step, self.note_end) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Moves time on by `frames`, at most [`Self::frames_to_event`].
    pub fn advance(&mut self, frames: u32) {
        if let Some(position) = &mut self.position {
            *position += u64::from(frames);
        }
        for timer in [&mut self.next_step, &mut self.note_end] {
            if let Some(left) = timer {
                *left = left.saturating_sub(frames);
            }
        }
    }

    /// Plays what is due now.
    pub fn fire(&mut self, out: &mut impl FnMut(MidiOut)) {
        let Some(settings) = self.settings.arpeggiator else {
            return;
        };
        if self.note_end == Some(0) || self.next_step == Some(0) {
            if let Some(pitch) = self.sounding.take() {
                out(MidiOut::Off { pitch });
            }
            self.note_end = None;
        }
        if self.next_step != Some(0) {
            return;
        }
        let Some((pitch, velocity)) = self.pick(settings) else {
            self.next_step = None;
            return;
        };
        out(MidiOut::On { pitch, velocity });
        self.sounding = Some(pitch);
        // To the next grid line while playing, so steps never drift off it.
        let whole = self.step_frames(settings.rate);
        let step = match self.position {
            Some(position) => {
                let next = ((position as f64 + 1.0) / whole).floor() + 1.0;
                (next * whole - position as f64).round().max(1.0) as u32
            }
            None => whole.round() as u32,
        };
        let gate = u64::from(step) * u64::from(settings.gate.clamp(1, 100)) / 100;
        self.note_end = Some((gate as u32).max(1));
        self.next_step = Some(step);
        self.step = self.step.wrapping_add(1);
    }

    /// The note of the current step, walking the held notes over the octaves.
    fn pick(&mut self, settings: Arpeggiator) -> Option<(u8, u8)> {
        let count = self.wanted.iter().filter(|(count, _)| *count > 0).count() as u32;
        if count == 0 {
            return None;
        }
        let total = count * u32::from(settings.octaves.clamp(1, 4));
        let index = match settings.pattern {
            ArpPattern::Up => self.step % total,
            ArpPattern::Down => total - 1 - self.step % total,
            ArpPattern::UpDown => {
                let period = (2 * total).saturating_sub(2).max(1);
                let at = self.step % period;
                at.min(period - at)
            }
            ArpPattern::Random => {
                self.random ^= self.random << 13;
                self.random ^= self.random >> 17;
                self.random ^= self.random << 5;
                self.random % total
            }
        };
        let (pitch, &(_, velocity)) = self
            .wanted
            .iter()
            .enumerate()
            .filter(|(_, (count, _))| *count > 0)
            .nth((index % count) as usize)?;
        let pitch = pitch as u32 + 12 * (index / count);
        Some((pitch.min(127) as u8, velocity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleKind;

    fn collect(run: impl FnOnce(&mut dyn FnMut(MidiOut))) -> Vec<MidiOut> {
        let mut events = Vec::new();
        run(&mut |event| events.push(event));
        events
    }

    fn on(pitch: u8) -> MidiOut {
        MidiOut::On {
            pitch,
            velocity: 100,
        }
    }

    fn off(pitch: u8) -> MidiOut {
        MidiOut::Off { pitch }
    }

    #[test]
    fn shared_chord_notes_stop_with_their_last_key() {
        let mut midi = MidiProcessor::default();
        let settings = MidiEffects {
            chord: ChordKind::Fifth,
            ..Default::default()
        };
        let events = collect(|mut out| {
            midi.set(settings, &mut out);
            midi.note_on(60, 100, &mut out);
            midi.note_on(67, 100, &mut out);
            midi.note_off(60, &mut out);
            midi.note_off(67, &mut out);
        });
        assert_eq!(
            events,
            [on(60), on(67), on(67), on(74), off(60), off(67), off(74)]
        );
    }

    #[test]
    fn the_scale_moves_notes_and_their_release() {
        let mut midi = MidiProcessor::default();
        let settings = MidiEffects {
            scale: Some(Scale {
                root: 0,
                kind: ScaleKind::Major,
            }),
            ..Default::default()
        };
        let events = collect(|mut out| {
            midi.set(settings, &mut out);
            midi.note_on(61, 100, &mut out);
            midi.note_off(61, &mut out);
        });
        assert_eq!(events, [on(60), off(60)]);
    }

    #[test]
    fn a_key_pressed_between_steps_waits_for_the_song_grid() {
        let mut midi = MidiProcessor::default();
        midi.set_clock(48_000.0, 120.0);
        midi.set(
            MidiEffects {
                arpeggiator: Some(Arpeggiator::default()),
                ..Default::default()
            },
            &mut |_| {},
        );
        midi.set_position(Some(4_000));
        midi.note_on(60, 100, &mut |_| {});
        // The next sixteenth is at 6000.
        assert_eq!(midi.frames_to_event(), Some(2_000));
        midi.advance(2_000);
        let first = collect(|mut out| midi.fire(&mut out));
        assert_eq!(first, [on(60)]);
        midi.advance(3_000);
        midi.fire(&mut |_| {});
        // And the one after at 12000, wherever the key came.
        assert_eq!(midi.frames_to_event(), Some(3_000));
    }

    #[test]
    fn the_arpeggiator_steps_through_held_notes_at_its_rate() {
        let mut midi = MidiProcessor::default();
        midi.set_clock(48_000.0, 120.0);
        let mut events = Vec::new();
        midi.set(
            MidiEffects {
                arpeggiator: Some(Arpeggiator::default()),
                ..Default::default()
            },
            &mut |_| {},
        );
        midi.note_on(64, 100, &mut |event| events.push((0, event)));
        midi.note_on(60, 100, &mut |event| events.push((0, event)));
        // A sixteenth at 120 BPM is 6000 frames, half of it sounding.
        let mut time = 0;
        while let Some(wait) = midi.frames_to_event() {
            if time + wait >= 13_000 {
                break;
            }
            midi.advance(wait);
            time += wait;
            midi.fire(&mut |event| events.push((time, event)));
        }
        assert_eq!(
            events,
            [
                (0, on(60)),
                (3_000, off(60)),
                (6_000, on(64)),
                (9_000, off(64)),
                (12_000, on(60)),
            ]
        );
        midi.note_off(60, &mut |_| {});
        midi.note_off(64, &mut |_| {});
        let wait = midi.frames_to_event().unwrap();
        midi.advance(wait);
        let tail = collect(|mut out| midi.fire(&mut out));
        assert_eq!(tail, [off(60)]);
        assert_eq!(midi.frames_to_event(), None);
    }
}
