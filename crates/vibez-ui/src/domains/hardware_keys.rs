//! Keys and wheel of the hardware MIDI inputs play the configured MIDI
//! target Track.

use std::collections::HashMap;

use vibez_audio_io::midi_input::MidiEvent;
use vibez_core::id::TrackId;
use vibez_engine::commands::EngineCommand;
use vibez_instruments::PITCH_BEND_RANGE_SEMITONES;

use super::EngineHandle;

/// Which open input an event came from.
pub type PortId = u32;

/// The keys held down on the open hardware MIDI inputs, each with the Track
/// it played, so selecting another Track cannot redirect its release.
#[derive(Debug, Default)]
pub struct HardwareKeys {
    held: HashMap<(PortId, u8), TrackId>,
    /// The Track each port's wheel is holding away from rest.
    bent: HashMap<PortId, TrackId>,
}

impl HardwareKeys {
    /// Plays a key-down on `target`, the configured MIDI target Track now,
    /// and a key-up on the Track its key-down played. A key-down with no
    /// target and a key-up with no key-down it played are dropped.
    pub fn play(
        &mut self,
        port: PortId,
        event: MidiEvent,
        target: Option<TrackId>,
        engine: &mut impl EngineHandle,
    ) {
        match event {
            MidiEvent::NoteOn { pitch, velocity } => {
                let Some(track_id) = target else {
                    return;
                };
                if let Some(previous) = self.held.insert((port, pitch), track_id) {
                    engine.send(EngineCommand::ExternalNoteOff {
                        track_id: previous,
                        pitch,
                    });
                }
                engine.send(EngineCommand::ExternalNoteOn {
                    track_id,
                    pitch,
                    velocity,
                });
            }
            MidiEvent::NoteOff { pitch } => {
                if let Some(track_id) = self.held.remove(&(port, pitch)) {
                    engine.send(EngineCommand::ExternalNoteOff { track_id, pitch });
                }
            }
            MidiEvent::PitchBend { value } => {
                // A Track left bent by this wheel goes back to rest first.
                if let Some(previous) = self.bent.get(&port).copied() {
                    if Some(previous) != target {
                        self.rest(port, engine);
                    }
                }
                let Some(track_id) = target else {
                    return;
                };
                // The two directions have 8192 and 8191 steps.
                let full = if value < 0 { 8192.0 } else { 8191.0 };
                engine.send(EngineCommand::ExternalPitchBend {
                    track_id,
                    semitones: f32::from(value) / full * PITCH_BEND_RANGE_SEMITONES,
                });
                if value == 0 {
                    self.bent.remove(&port);
                } else {
                    self.bent.insert(port, track_id);
                }
            }
            MidiEvent::ControlChange { .. } => {
                // Controllers are mapped by the app (MIDI learn).
            }
        }
    }

    fn rest(&mut self, port: PortId, engine: &mut impl EngineHandle) {
        if let Some(track_id) = self.bent.remove(&port) {
            engine.send(EngineCommand::ExternalPitchBend {
                track_id,
                semitones: 0.0,
            });
        }
    }

    /// Releases everything `port` is holding, as it closes: its key-ups will
    /// never arrive.
    pub fn release_port(&mut self, port: PortId, engine: &mut impl EngineHandle) {
        let keys: Vec<(PortId, u8)> = self
            .held
            .keys()
            .filter(|(held, _)| *held == port)
            .copied()
            .collect();
        for key in keys {
            if let Some(track_id) = self.held.remove(&key) {
                engine.send(EngineCommand::ExternalNoteOff {
                    track_id,
                    pitch: key.1,
                });
            }
        }
        self.rest(port, engine);
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::RecordingEngine;
    use super::*;

    const C4: u8 = 60;

    /// The live notes sent, as (track, pitch, on).
    fn notes(engine: &RecordingEngine) -> Vec<(TrackId, u8, bool)> {
        engine
            .0
            .iter()
            .filter_map(|command| match command {
                EngineCommand::ExternalNoteOn {
                    track_id, pitch, ..
                } => Some((*track_id, *pitch, true)),
                EngineCommand::ExternalNoteOff { track_id, pitch } => {
                    Some((*track_id, *pitch, false))
                }
                _ => None,
            })
            .collect()
    }

    fn press(pitch: u8) -> MidiEvent {
        MidiEvent::NoteOn {
            pitch,
            velocity: 100,
        }
    }

    fn release(pitch: u8) -> MidiEvent {
        MidiEvent::NoteOff { pitch }
    }

    #[test]
    fn a_key_released_after_selecting_another_track_releases_where_it_was_pressed() {
        let (a, b) = (TrackId::new(), TrackId::new());
        let mut keys = HardwareKeys::default();
        let mut engine = RecordingEngine::default();

        keys.play(0, press(C4), Some(a), &mut engine);
        keys.play(0, release(C4), Some(b), &mut engine);

        assert_eq!(notes(&engine), [(a, C4, true), (a, C4, false)]);
    }

    #[test]
    fn a_key_released_with_no_target_left_still_releases() {
        let a = TrackId::new();
        let mut keys = HardwareKeys::default();
        let mut engine = RecordingEngine::default();

        keys.play(0, press(C4), Some(a), &mut engine);
        keys.play(0, release(C4), None, &mut engine);

        assert_eq!(notes(&engine), [(a, C4, true), (a, C4, false)]);
    }

    #[test]
    fn a_key_pressed_again_on_another_track_first_releases_the_first() {
        let (a, b) = (TrackId::new(), TrackId::new());
        let mut keys = HardwareKeys::default();
        let mut engine = RecordingEngine::default();

        keys.play(0, press(C4), Some(a), &mut engine);
        keys.play(0, press(C4), Some(b), &mut engine);
        keys.play(0, release(C4), Some(b), &mut engine);

        assert_eq!(
            notes(&engine),
            [(a, C4, true), (a, C4, false), (b, C4, true), (b, C4, false)]
        );
    }

    #[test]
    fn a_key_up_without_a_press_it_played_is_dropped() {
        let b = TrackId::new();
        let mut keys = HardwareKeys::default();
        let mut engine = RecordingEngine::default();

        keys.play(0, press(C4), None, &mut engine);
        keys.play(0, release(C4), Some(b), &mut engine);

        assert_eq!(notes(&engine), []);
    }

    #[test]
    fn the_wheel_bends_the_target_and_rests_the_track_it_leaves() {
        let (first, second) = (TrackId::new(), TrackId::new());
        let mut keys = HardwareKeys::default();
        let mut engine = RecordingEngine::default();
        keys.play(
            0,
            MidiEvent::PitchBend { value: 8191 },
            Some(first),
            &mut engine,
        );
        keys.play(
            0,
            MidiEvent::PitchBend { value: -8192 },
            Some(second),
            &mut engine,
        );
        let bends: Vec<_> = engine
            .0
            .iter()
            .filter_map(|command| match command {
                EngineCommand::ExternalPitchBend {
                    track_id,
                    semitones,
                } => Some((*track_id, *semitones)),
                _ => None,
            })
            .collect();
        assert_eq!(bends, [(first, 2.0), (first, 0.0), (second, -2.0)]);
    }

    #[test]
    fn closing_a_port_releases_only_its_own_keys() {
        let track = TrackId::new();
        let mut keys = HardwareKeys::default();
        let mut engine = RecordingEngine::default();
        keys.play(1, press(C4), Some(track), &mut engine);
        keys.play(2, press(C4 + 1), Some(track), &mut engine);
        keys.release_port(1, &mut engine);
        assert_eq!(
            notes(&engine),
            [(track, C4, true), (track, C4 + 1, true), (track, C4, false)]
        );
    }
}
