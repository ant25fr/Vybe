//! A channel's modulators: adding them, setting them and choosing what they
//! move.

use std::collections::HashMap;
use std::time::Instant;

use vibez_core::automation::AutomationTarget;
use vibez_core::id::TrackId;
use vibez_core::modulation::{
    can_modulate, ModulationLink, Modulator, ModulatorKind, ModulatorSettings, MAX_ENVELOPE_MS,
    MAX_MODULATION_LINKS, MAX_MODULATORS, MAX_RATE_HZ, MIN_RATE_HZ,
};
use vibez_engine::commands::EngineCommand;

use super::EngineHandle;
use crate::state::{ProjectTrack, ProjectTracksState};

/// How far a newly linked parameter moves, as a share of its range.
pub const NEW_LINK_AMOUNT: f32 = 0.25;

/// A continuous control of the modulator card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModField {
    Rate,
    Phase,
    Smooth,
    Attack,
    Decay,
    Sustain,
    Release,
    /// The amount of one link.
    Amount(usize),
}

impl ModField {
    /// The knob's travel and rest value.
    pub fn range(self) -> (f32, f32, f32) {
        let defaults = ModulatorSettings::default();
        match self {
            Self::Rate => (MIN_RATE_HZ, MAX_RATE_HZ, defaults.rate_hz),
            Self::Phase => (0.0, 1.0, defaults.phase),
            Self::Smooth => (0.0, 1.0, defaults.smooth),
            Self::Attack => (0.0, MAX_ENVELOPE_MS, defaults.attack_ms),
            Self::Decay => (0.0, MAX_ENVELOPE_MS, defaults.decay_ms),
            Self::Sustain => (0.0, 1.0, defaults.sustain),
            Self::Release => (0.0, MAX_ENVELOPE_MS, defaults.release_ms),
            Self::Amount(_) => (-1.0, 1.0, 0.0),
        }
    }

    /// Whether the knob travels logarithmically, as rates and times need.
    pub fn is_curved(self) -> bool {
        matches!(
            self,
            Self::Rate | Self::Attack | Self::Decay | Self::Release
        )
    }

    /// The knob position, 0..1, showing `value` on a curved control.
    pub fn position_of(self, value: f32) -> f32 {
        let (min, max, _) = self.range();
        ((value - min) / (max - min)).clamp(0.0, 1.0).cbrt()
    }

    /// The value a curved control's knob position stands for.
    pub fn value_at(self, position: f32) -> f32 {
        let (min, max, _) = self.range();
        min + position.clamp(0.0, 1.0).powi(3) * (max - min)
    }

    pub fn read(self, modulator: &Modulator) -> f32 {
        let settings = &modulator.settings;
        match self {
            Self::Rate => settings.rate_hz,
            Self::Phase => settings.phase,
            Self::Smooth => settings.smooth,
            Self::Attack => settings.attack_ms,
            Self::Decay => settings.decay_ms,
            Self::Sustain => settings.sustain,
            Self::Release => settings.release_ms,
            Self::Amount(link) => modulator.links.get(link).map_or(0.0, |link| link.amount),
        }
    }

    fn write(self, modulator: &mut Modulator, value: f32) {
        let settings = &mut modulator.settings;
        match self {
            Self::Rate => settings.rate_hz = value,
            Self::Phase => settings.phase = value,
            Self::Smooth => settings.smooth = value,
            Self::Attack => settings.attack_ms = value,
            Self::Decay => settings.decay_ms = value,
            Self::Sustain => settings.sustain = value,
            Self::Release => settings.release_ms = value,
            Self::Amount(link) => {
                if let Some(link) = modulator.links.get_mut(link) {
                    link.amount = value.clamp(-1.0, 1.0);
                }
            }
        }
        modulator.settings = modulator.settings.clamped();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModulationMsg {
    /// Show another of the channel's modulators.
    Select(TrackId, usize),
    Add(TrackId, ModulatorKind),
    Remove(TrackId, usize),
    SetSettings(TrackId, usize, ModulatorSettings),
    /// A knob's value; curved controls send the knob position.
    SetField(TrackId, usize, ModField, f32),
    AddLink(TrackId, usize, AutomationTarget),
    RemoveLink(TrackId, usize, usize),
}

impl ModulationMsg {
    pub fn track_id(&self) -> TrackId {
        match *self {
            Self::Select(id, _)
            | Self::Add(id, _)
            | Self::Remove(id, _)
            | Self::SetSettings(id, ..)
            | Self::SetField(id, ..)
            | Self::AddLink(id, ..)
            | Self::RemoveLink(id, ..) => id,
        }
    }

    /// The channel's modulators after this message, when it changes them.
    pub fn edit(&self, tracks: &ProjectTracksState) -> Option<Vec<Modulator>> {
        let current = &tracks.find(self.track_id())?.modulators;
        let mut next = current.clone();
        match *self {
            Self::Select(..) => return None,
            Self::Add(_, kind) => {
                if next.len() >= MAX_MODULATORS {
                    return None;
                }
                next.push(Modulator {
                    settings: ModulatorSettings::of(kind),
                    links: Vec::new(),
                });
            }
            Self::Remove(_, index) => {
                if index >= next.len() {
                    return None;
                }
                next.remove(index);
            }
            Self::SetSettings(_, index, settings) => {
                next.get_mut(index)?.settings = settings.clamped();
            }
            Self::SetField(_, index, field, value) => {
                if !value.is_finite() {
                    return None;
                }
                let value = if field.is_curved() {
                    field.value_at(value)
                } else {
                    value
                };
                field.write(next.get_mut(index)?, value);
            }
            Self::AddLink(_, index, target) => {
                let modulator = next.get_mut(index)?;
                if !can_modulate(target)
                    || modulator.links.len() >= MAX_MODULATION_LINKS
                    || modulator.links.iter().any(|link| link.target == target)
                {
                    return None;
                }
                modulator.links.push(ModulationLink {
                    target,
                    amount: NEW_LINK_AMOUNT,
                });
            }
            Self::RemoveLink(_, index, link) => {
                let modulator = next.get_mut(index)?;
                if link >= modulator.links.len() {
                    return None;
                }
                modulator.links.remove(link);
            }
        }
        (next != *current).then_some(next)
    }
}

/// Which modulator each channel's card shows, and the clock its plot runs on
/// while the transport is stopped. Not saved with the project.
#[derive(Debug)]
pub struct ModulationState {
    selected: HashMap<TrackId, usize>,
    started: Instant,
}

impl Default for ModulationState {
    fn default() -> Self {
        Self {
            selected: HashMap::new(),
            started: Instant::now(),
        }
    }
}

impl ModulationState {
    pub fn seconds_running(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// The modulator shown for a channel with `count` of them.
    pub fn selected(&self, track_id: TrackId, count: usize) -> usize {
        self.selected
            .get(&track_id)
            .copied()
            .unwrap_or(0)
            .min(count.saturating_sub(1))
    }

    pub fn update(
        &mut self,
        msg: ModulationMsg,
        engine: &mut impl EngineHandle,
        tracks: &mut std::sync::Arc<ProjectTracksState>,
    ) {
        let track_id = msg.track_id();
        if let ModulationMsg::Select(_, index) = msg {
            self.selected.insert(track_id, index);
            return;
        }
        let Some(next) = msg.edit(tracks) else {
            return;
        };
        if matches!(msg, ModulationMsg::Add(..)) {
            self.selected.insert(track_id, next.len() - 1);
        }
        if let Some(track) = std::sync::Arc::make_mut(tracks).find_mut(track_id) {
            sync_modulators(engine, track_id, &track.modulators, &next);
            track.modulators = next;
        }
    }
}

/// Send what differs between two lists of a channel's modulators.
pub fn sync_modulators(
    engine: &mut impl EngineHandle,
    track_id: TrackId,
    current: &[Modulator],
    next: &[Modulator],
) {
    for slot in 0..MAX_MODULATORS {
        let (old, new) = (current.get(slot), next.get(slot));
        let settings = new.map(|modulator| modulator.settings);
        if old.map(|modulator| modulator.settings) != settings {
            engine.send(EngineCommand::SetModulator {
                track_id,
                slot: slot as u8,
                settings,
            });
        }
        // Removing a modulator drops its links with it.
        let Some(new) = new else {
            continue;
        };
        for link in 0..MAX_MODULATION_LINKS {
            let value = new.links.get(link).copied();
            if old.and_then(|old| old.links.get(link).copied()) != value {
                engine.send(EngineCommand::SetModulationLink {
                    track_id,
                    slot: slot as u8,
                    link: link as u8,
                    value,
                });
            }
        }
    }
}

/// A replaced instrument's parameters are other parameters: unlink them.
pub fn drop_instrument_links(engine: &mut impl EngineHandle, track: &mut ProjectTrack) {
    let mut next = track.modulators.clone();
    for modulator in &mut next {
        modulator
            .links
            .retain(|link| !matches!(link.target, AutomationTarget::InstrumentParam { .. }));
    }
    sync_modulators(engine, track.id, &track.modulators, &next);
    track.modulators = next;
}

#[cfg(test)]
#[path = "modulation_domain_tests.rs"]
mod tests;
