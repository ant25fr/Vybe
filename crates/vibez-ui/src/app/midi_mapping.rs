//! MIDI learn: touch a control, move a hardware controller, and the two are
//! bound. Bound controllers then edit the parameter like the mouse would.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use vibez_core::automation::AutomationTarget;
use vibez_core::id::TrackId;
use vibez_core::midi_map::{bind, MidiMapping};

use crate::domains::arrangement::ArrangementMsg;
use crate::domains::automation::{target_descriptor, target_label_with_buses};
use crate::domains::devices::DevicesMsg;
use crate::state::UndoGestureId;

use super::*;

/// Moves of one controller closer together than this are one undo step.
const GESTURE_PAUSE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub enum MidiMapMsg {
    ToggleLearn,
    /// Bind a controller to a parameter.
    Bind(MidiMapping),
    /// Forget the mapping of a controller number.
    Remove(u8),
}

#[derive(Default)]
pub(crate) struct MidiLearn {
    pub active: bool,
    /// The parameter last touched while learning, waiting for a controller.
    pub pending: Option<(TrackId, AutomationTarget)>,
    /// A controller is editing, so its own edit is not a touch.
    applying: bool,
    gestures: HashMap<u8, (UndoGestureId, Instant)>,
}

impl App {
    pub(super) fn update_midi_map(&mut self, message: MidiMapMsg) -> Task<Message> {
        match message {
            MidiMapMsg::ToggleLearn => {
                self.midi_learn.active = !self.midi_learn.active;
                self.midi_learn.pending = None;
                self.state.status_text = if !self.midi_learn.active {
                    "MIDI learn off".into()
                } else if self.midi_ports.open.is_empty() {
                    "MIDI learn on, but no MIDI input is open (Settings, Audio)".into()
                } else {
                    "MIDI learn: move a fader or knob, then your controller".into()
                };
            }
            MidiMapMsg::Bind(mapping) => {
                let label = self.midi_mapping_label(mapping.track_id, mapping.target);
                bind(
                    &mut Arc::make_mut(&mut self.state.project_tracks).midi_mappings,
                    mapping,
                );
                self.state.status_text = format!("CC {} now controls {label}", mapping.cc);
            }
            MidiMapMsg::Remove(cc) => {
                Arc::make_mut(&mut self.state.project_tracks)
                    .midi_mappings
                    .retain(|mapping| mapping.cc != cc);
                self.state.status_text = format!("Removed the mapping of CC {cc}");
            }
        }
        Task::none()
    }

    /// "Channel: parameter", for the status bar and the mapping list.
    pub(super) fn midi_mapping_label(&self, track_id: TrackId, target: AutomationTarget) -> String {
        let tracks = &self.state.project_tracks;
        match tracks.find(track_id) {
            Some(track) => format!(
                "{}: {}",
                track.name,
                target_label_with_buses(&target, track, &tracks.buses)
            ),
            None => "(removed channel)".into(),
        }
    }

    /// While learning, remembers which parameter `message` edits.
    pub(super) fn observe_midi_learn(&mut self, message: &Message) {
        if !self.midi_learn.active || self.midi_learn.applying {
            return;
        }
        let Some((track_id, target)) = self.edited_parameter(message) else {
            return;
        };
        if self.midi_learn.pending != Some((track_id, target)) {
            self.midi_learn.pending = Some((track_id, target));
            self.state.status_text = format!(
                "MIDI learn: move a controller for {}",
                self.midi_mapping_label(track_id, target)
            );
        }
    }

    fn edited_parameter(&self, message: &Message) -> Option<(TrackId, AutomationTarget)> {
        match message {
            Message::Arrangement(ArrangementMsg::SetTrackGain(track, _)) => {
                Some((*track, AutomationTarget::TrackGain))
            }
            Message::Arrangement(ArrangementMsg::SetTrackPan(track, _)) => {
                Some((*track, AutomationTarget::TrackPan))
            }
            Message::Arrangement(ArrangementMsg::SetSend {
                track_id, bus_id, ..
            }) => Some((*track_id, AutomationTarget::Send { bus_id: *bus_id })),
            Message::Devices(DevicesMsg::SetEffectParam(track, effect_id, param_index, _)) => {
                Some((
                    *track,
                    AutomationTarget::EffectParam {
                        effect_id: *effect_id,
                        param_index: *param_index,
                    },
                ))
            }
            Message::Devices(DevicesMsg::SetInstrumentParam(track, param_index, _)) => Some((
                *track,
                AutomationTarget::InstrumentParam {
                    param_index: *param_index,
                },
            )),
            _ => None,
        }
    }

    /// The edit that sets `target` to `value` (0..=127), or `None` when the
    /// parameter no longer exists.
    fn controller_edit(
        &self,
        track_id: TrackId,
        target: AutomationTarget,
        value: u8,
    ) -> Option<Message> {
        let track = self.state.project_tracks.find(track_id)?;
        let normal = f32::from(value.min(127)) / 127.0;
        let scaled = || {
            let d = target_descriptor(&target, track)?;
            Some(d.min + normal * (d.max - d.min))
        };
        Some(match target {
            AutomationTarget::TrackGain => {
                Message::Arrangement(ArrangementMsg::SetTrackGain(track_id, normal * 2.0))
            }
            AutomationTarget::TrackPan => {
                // 64 is the controller's centre detent.
                let pan = if value == 64 { 0.5 } else { normal };
                Message::Arrangement(ArrangementMsg::SetTrackPan(track_id, pan))
            }
            AutomationTarget::Send { bus_id } => {
                self.state.project_tracks.find(bus_id)?;
                Message::set_send(track_id, bus_id, normal)
            }
            AutomationTarget::EffectParam {
                effect_id,
                param_index,
            } => Message::set_effect_param(track_id, effect_id, param_index, scaled()?),
            AutomationTarget::InstrumentParam { param_index } => {
                Message::set_instrument_param(track_id, param_index, scaled()?)
            }
            _ => return None,
        })
    }

    /// One controller move from the hardware input.
    pub(super) fn handle_controller(&mut self, cc: u8, value: u8, now: Instant) {
        if self.midi_learn.active {
            if let Some((track_id, target)) = self.midi_learn.pending.take() {
                let _ = self.update_and_refresh_clips(Message::MidiMap(MidiMapMsg::Bind(
                    MidiMapping {
                        cc,
                        track_id,
                        target,
                    },
                )));
                return;
            }
        }
        let Some(mapping) = self
            .state
            .project_tracks
            .midi_mappings
            .iter()
            .find(|mapping| mapping.cc == cc)
            .copied()
        else {
            return;
        };
        let Some(edit) = self.controller_edit(mapping.track_id, mapping.target, value) else {
            return;
        };
        let id = match self.midi_learn.gestures.get(&cc) {
            Some((id, last)) if now.saturating_duration_since(*last) < GESTURE_PAUSE => *id,
            _ => UndoGestureId::new(),
        };
        self.midi_learn.gestures.insert(cc, (id, now));
        self.midi_learn.applying = true;
        let _ = self.update_and_refresh_clips(Message::UndoGesture {
            id,
            edit: Box::new(edit),
        });
        self.midi_learn.applying = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::project::ProjectMsg;
    use crate::state::ProjectTrack;

    fn app_with_track() -> (App, TrackId) {
        let mut app = super::super::test_support::app();
        let track = ProjectTrack::new(TrackId::new(), "Bass".into(), 2);
        let id = track.id;
        Arc::make_mut(&mut app.state.project_tracks)
            .tracks
            .push(track);
        (app, id)
    }

    #[test]
    fn a_touched_fader_is_learned_and_then_follows_the_controller() {
        let (mut app, track) = app_with_track();
        let _ = app.update(Message::MidiMap(MidiMapMsg::ToggleLearn));
        let _ = app.update(Message::Arrangement(ArrangementMsg::SetTrackGain(
            track, 0.7,
        )));
        let start = Instant::now();
        app.handle_controller(21, 10, start);
        let mappings = &app.state.project_tracks.midi_mappings;
        assert_eq!(mappings.len(), 1);
        assert_eq!(
            (mappings[0].cc, mappings[0].track_id, mappings[0].target),
            (21, track, AutomationTarget::TrackGain)
        );
        // The binding move itself changes nothing.
        assert_eq!(app.state.find_track(track).unwrap().gain, 0.7);

        app.handle_controller(21, 127, start);
        assert_eq!(app.state.find_track(track).unwrap().gain, 2.0);
        app.handle_controller(21, 0, start + Duration::from_millis(100));
        assert_eq!(app.state.find_track(track).unwrap().gain, 0.0);

        // The two quick moves undo together.
        let _ = app.update(Message::Project(ProjectMsg::Undo));
        assert_eq!(app.state.find_track(track).unwrap().gain, 0.7);
    }

    #[test]
    fn an_unbound_controller_and_a_removed_mapping_do_nothing() {
        let (mut app, track) = app_with_track();
        app.handle_controller(7, 0, Instant::now());
        let _ = app.update(Message::MidiMap(MidiMapMsg::Bind(MidiMapping {
            cc: 7,
            track_id: track,
            target: AutomationTarget::TrackPan,
        })));
        app.handle_controller(7, 64, Instant::now());
        assert_eq!(app.state.find_track(track).unwrap().pan, 0.5);
        app.handle_controller(7, 0, Instant::now());
        assert_eq!(app.state.find_track(track).unwrap().pan, 0.0);
        let _ = app.update(Message::MidiMap(MidiMapMsg::Remove(7)));
        app.handle_controller(7, 127, Instant::now());
        assert_eq!(app.state.find_track(track).unwrap().pan, 0.0);
    }
}
