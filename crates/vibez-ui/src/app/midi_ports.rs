//! The open hardware MIDI inputs: any number at once, remembered by name so
//! an unplugged device is reopened the next time it is there.

use vibez_audio_io::midi_input::{list_midi_input_ports, open_midi_input, MidiInputHandle};

use crate::domains::hardware_keys::PortId;

use super::*;

pub(crate) struct MidiPort {
    pub id: PortId,
    pub handle: MidiInputHandle,
}

#[derive(Default)]
pub(crate) struct MidiPorts {
    pub open: Vec<MidiPort>,
    /// Names of the inputs the user enabled, open or not.
    pub wanted: Vec<String>,
    next_id: PortId,
}

impl MidiPorts {
    /// Opens the saved inputs that are present. With nothing saved yet
    /// (`saved` empty and never chosen), opens `preferred`, else the first.
    pub fn restore(saved: Option<&[String]>, preferred: Option<&str>) -> Self {
        let mut ports = Self::default();
        let wanted: Vec<String> = match saved {
            Some(saved) => saved.to_vec(),
            None => {
                let present = list_midi_input_ports().unwrap_or_default();
                preferred
                    .map(str::to_string)
                    .filter(|name| present.contains(name))
                    .or_else(|| present.into_iter().next())
                    .into_iter()
                    .collect()
            }
        };
        for name in wanted {
            // A missing device stays wanted.
            let _ = ports.open(&name);
            if !ports.wanted.contains(&name) {
                ports.wanted.push(name);
            }
        }
        ports
    }

    pub fn is_open(&self, name: &str) -> bool {
        self.open.iter().any(|port| port.handle.port_name == name)
    }

    pub fn open(&mut self, name: &str) -> Result<(), String> {
        if self.is_open(name) {
            return Ok(());
        }
        let handle = open_midi_input(name)?;
        self.next_id += 1;
        if !self.wanted.contains(&handle.port_name) {
            self.wanted.push(handle.port_name.clone());
        }
        self.open.push(MidiPort {
            id: self.next_id,
            handle,
        });
        Ok(())
    }

    /// Closes and forgets `name`, returning its id if it was open.
    pub fn close(&mut self, name: &str) -> Option<PortId> {
        self.wanted.retain(|wanted| wanted != name);
        let index = self
            .open
            .iter()
            .position(|port| port.handle.port_name == name)?;
        Some(self.open.remove(index).id)
    }
}

impl App {
    pub(super) fn on_close_midi_port(&mut self, name: &str) {
        if let Some(id) = self.midi_ports.close(name) {
            self.hardware_keys.release_port(id, &mut self.cmd_tx);
        }
        self.state.status_text = format!("MIDI input off: {name}");
        self.persist_ui_settings();
    }

    pub(super) fn on_close_all_midi_ports(&mut self) {
        let open: Vec<String> = self
            .midi_ports
            .open
            .iter()
            .map(|port| port.handle.port_name.clone())
            .collect();
        for name in open {
            if let Some(id) = self.midi_ports.close(&name) {
                self.hardware_keys.release_port(id, &mut self.cmd_tx);
            }
        }
        self.midi_ports.wanted.clear();
        self.state.status_text = "MIDI inputs off".into();
        self.persist_ui_settings();
    }
}
