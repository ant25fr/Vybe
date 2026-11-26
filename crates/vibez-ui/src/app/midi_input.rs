//! MIDI delivery and timing eligibility share the existing configured destination.

use vibez_audio_io::midi_input::MidiEvent;

use super::*;

pub(super) fn configured_target(state: &crate::state::AppState) -> Option<vibez_core::id::TrackId> {
    state.arrangement.selected_track.filter(|id| {
        state
            .find_track(*id)
            .is_some_and(|track| track.is_playable_midi_target())
    })
}

impl App {
    /// Drain pending MIDI events from the open input ports and play them on
    /// the configured MIDI target. Called on every UI tick.
    pub(super) fn poll_midi_input(&mut self) {
        let mut events = Vec::new();
        for port in &self.midi_ports.open {
            while let Ok(event) = port.handle.rx.try_recv() {
                events.push((port.id, event));
            }
        }
        if events.is_empty() {
            return;
        }
        let target = configured_target(&self.state);
        // Controllers send a stream: only each one's latest value matters.
        let mut controllers: Vec<(u8, u8)> = Vec::new();
        for (port, event) in events {
            if let MidiEvent::ControlChange { cc, value } = event {
                match controllers.iter_mut().find(|(known, _)| *known == cc) {
                    Some(entry) => entry.1 = value,
                    None => controllers.push((cc, value)),
                }
            } else {
                self.hardware_keys
                    .play(port, event, target, &mut self.cmd_tx);
            }
        }
        let now = std::time::Instant::now();
        for (cc, value) in controllers {
            self.handle_controller(cc, value, now);
        }
    }
}
