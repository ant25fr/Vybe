//! A channel's running modulators and the parameter values they move, in
//! fixed storage the callback never grows.

use super::*;
use vibez_core::automation::AutomationTarget;
use vibez_core::modulation::{
    ModulationLink, ModulatorKind, ModulatorSettings, MAX_MODULATION_LINKS, MAX_MODULATORS,
};
use vibez_core::routing::NodeStage;

/// Parameters one channel can have modulated at once.
const MAX_TARGETS: usize = MAX_MODULATORS * MAX_MODULATION_LINKS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvelopeStage {
    Idle,
    Attack,
    Decay,
    Release,
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    settings: Option<ModulatorSettings>,
    links: [Option<ModulationLink>; MAX_MODULATION_LINKS],
    value: f32,
    level: f32,
    stage: EnvelopeStage,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            settings: None,
            links: [None; MAX_MODULATION_LINKS],
            value: 0.0,
            level: 0.0,
            stage: EnvelopeStage::Idle,
        }
    }
}

/// The value a modulated device parameter returns to.
#[derive(Debug, Clone, Copy)]
struct ParameterState {
    target: AutomationTarget,
    base: f32,
    written: f32,
}

/// When one block of modulation happens.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModulationClock {
    pub beat: f64,
    pub seconds: f64,
    pub frames: usize,
    pub sample_rate: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ModulationState {
    slots: [Slot; MAX_MODULATORS],
    parameters: [Option<ParameterState>; MAX_TARGETS],
    held: u128,
    /// Gain and pan offsets at the end of the previous block and of this one.
    mix_from: (f32, f32),
    mix_to: (f32, f32),
}

impl Default for ModulationState {
    fn default() -> Self {
        Self {
            slots: [Slot::default(); MAX_MODULATORS],
            parameters: [None; MAX_TARGETS],
            held: 0,
            mix_from: (0.0, 0.0),
            mix_to: (0.0, 0.0),
        }
    }
}

impl ModulationState {
    pub(crate) fn set_modulator(&mut self, index: usize, settings: Option<ModulatorSettings>) {
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        match settings {
            Some(settings) => {
                if slot.settings.map(|old| old.kind) != Some(settings.kind) {
                    slot.level = 0.0;
                    slot.stage = EnvelopeStage::Idle;
                    slot.value = 0.0;
                }
                slot.settings = Some(settings.clamped());
            }
            None => *slot = Slot::default(),
        }
    }

    pub(crate) fn set_link(&mut self, index: usize, link: usize, value: Option<ModulationLink>) {
        if let Some(slot) = self
            .slots
            .get_mut(index)
            .and_then(|slot| slot.links.get_mut(link))
        {
            *slot = value.filter(|link| link.amount.is_finite());
        }
    }

    /// Whether any modulator or not yet restored parameter needs work.
    pub(crate) fn is_active(&self) -> bool {
        self.slots.iter().any(|slot| slot.settings.is_some())
            || self.parameters.iter().any(Option::is_some)
            || self.mix_to != (0.0, 0.0)
            || self.mix_from != (0.0, 0.0)
    }

    /// Move every modulator to this block. `gate` holds the sounding pitches
    /// and `seed` tells channels' random sources apart.
    pub(crate) fn advance(&mut self, clock: ModulationClock, gate: u128, seed: u64) {
        let struck = gate & !self.held != 0;
        self.held = gate;
        let elapsed_ms = if clock.sample_rate == 0 {
            0.0
        } else {
            clock.frames as f32 * 1_000.0 / clock.sample_rate as f32
        };
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let Some(settings) = slot.settings else {
                continue;
            };
            slot.value = if settings.kind == ModulatorKind::Envelope {
                slot.advance_envelope(settings, struck, gate != 0, elapsed_ms)
            } else {
                let cycles = settings.cycles(clock.beat, clock.seconds);
                settings.cycle_value(cycles, seed.wrapping_mul(31).wrapping_add(index as u64))
            };
            if !settings.enabled {
                slot.value = 0.0;
            }
        }
        self.mix_from = self.mix_to;
        self.mix_to = (
            self.offset(AutomationTarget::TrackGain),
            self.offset(AutomationTarget::TrackPan),
        );
    }

    /// The summed movement of `target`, in shares of its range.
    fn offset(&self, target: AutomationTarget) -> f32 {
        self.slots
            .iter()
            .filter(|slot| slot.settings.is_some())
            .flat_map(|slot| {
                slot.links
                    .iter()
                    .flatten()
                    .filter(move |link| link.target == target)
                    .map(move |link| link.amount * slot.value)
            })
            .sum()
    }

    fn is_linked(&self, target: AutomationTarget) -> bool {
        self.slots.iter().any(|slot| {
            slot.settings.is_some()
                && slot
                    .links
                    .iter()
                    .flatten()
                    .any(|link| link.target == target)
        })
    }

    /// Gain and pan with modulation at `frame` of a block of `frames`, ramped
    /// so a stepped source does not click.
    pub(crate) fn mixed(&self, gain: f32, pan: f32, frame: usize, frames: usize) -> (f32, f32) {
        let progress = (frame + 1) as f32 / frames.max(1) as f32;
        let ramp = |from: f32, to: f32| from + (to - from) * progress;
        let gain_offset = ramp(self.mix_from.0, self.mix_to.0);
        let pan_offset = ramp(self.mix_from.1, self.mix_to.1);
        // Gain's native range is 0..2; pan is already 0..1.
        (
            if gain_offset == 0.0 {
                gain
            } else {
                (gain + gain_offset * 2.0).clamp(0.0, 2.0)
            },
            if pan_offset == 0.0 {
                pan
            } else {
                (pan + pan_offset).clamp(0.0, 1.0)
            },
        )
    }

    /// Take up the device parameters linked at `stage` that have no state yet.
    fn adopt_targets(&mut self, stage: NodeStage) {
        for slot in 0..MAX_MODULATORS {
            if self.slots[slot].settings.is_none() {
                continue;
            }
            for link in 0..MAX_MODULATION_LINKS {
                let Some(target) = self.slots[slot].links[link].map(|link| link.target) else {
                    continue;
                };
                if !at_stage(target, stage)
                    || self
                        .parameters
                        .iter()
                        .flatten()
                        .any(|state| state.target == target)
                {
                    continue;
                }
                if let Some(free) = self.parameters.iter_mut().find(|state| state.is_none()) {
                    *free = Some(ParameterState {
                        target,
                        base: f32::NAN,
                        written: f32::NAN,
                    });
                }
            }
        }
    }
}

impl Slot {
    fn advance_envelope(
        &mut self,
        settings: ModulatorSettings,
        struck: bool,
        gated: bool,
        elapsed_ms: f32,
    ) -> f32 {
        if struck {
            self.stage = EnvelopeStage::Attack;
        } else if !gated && self.stage != EnvelopeStage::Idle {
            self.stage = EnvelopeStage::Release;
        }
        // Each time is how long a full-scale move takes.
        let step = |time_ms: f32| {
            if time_ms <= 0.0 {
                1.0
            } else {
                elapsed_ms / time_ms
            }
        };
        match self.stage {
            EnvelopeStage::Idle => {}
            EnvelopeStage::Attack => {
                self.level += step(settings.attack_ms);
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = EnvelopeStage::Decay;
                }
            }
            EnvelopeStage::Decay => {
                self.level = (self.level - step(settings.decay_ms)).max(settings.sustain);
            }
            EnvelopeStage::Release => {
                self.level -= step(settings.release_ms);
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = EnvelopeStage::Idle;
                }
            }
        }
        self.level
    }
}

fn at_stage(target: AutomationTarget, stage: NodeStage) -> bool {
    match target {
        AutomationTarget::EffectParam { effect_id, .. } => stage == NodeStage::Effect(effect_id),
        AutomationTarget::InstrumentParam { .. } => stage == NodeStage::Source,
        _ => false,
    }
}

impl EngineTrack {
    pub(crate) fn advance_modulation(&mut self, clock: ModulationClock) {
        if self.modulation.is_active() {
            // A removed device takes its values with it; nothing is restored.
            let (effects, instrument) = (&self.effects, self.instrument.is_some());
            for state in &mut self.modulation.parameters {
                let present = state.is_none_or(|state| match state.target {
                    AutomationTarget::EffectParam { effect_id, .. } => {
                        effects.iter().any(|slot| slot.id == effect_id)
                    }
                    _ => instrument,
                });
                if !present {
                    *state = None;
                }
            }
            let gate = self.active_notes | self.live_held;
            self.modulation.advance(clock, gate, self.id.raw());
        }
    }

    /// Write the modulated device parameters of `stage`, after automation. A
    /// value someone else wrote since the last block becomes the new base.
    pub(crate) fn apply_modulation(&mut self, stage: NodeStage) {
        if !self.modulation.is_active() {
            return;
        }
        self.modulation.adopt_targets(stage);
        for index in 0..MAX_TARGETS {
            let Some(mut state) = self.modulation.parameters[index] else {
                continue;
            };
            if !at_stage(state.target, stage) {
                continue;
            }
            let linked = self.modulation.is_linked(state.target);
            let offset = self.modulation.offset(state.target);
            let apply = |current: f32, low: f32, high: f32, state: &mut ParameterState| {
                // NaN marks a parameter not yet read.
                if current != state.written {
                    state.base = current;
                }
                if linked {
                    (state.base + offset * (high - low)).clamp(low, high)
                } else {
                    state.base
                }
            };
            let found = match state.target {
                AutomationTarget::EffectParam {
                    effect_id,
                    param_index,
                } => self
                    .effects
                    .iter_mut()
                    .find(|slot| slot.id == effect_id)
                    .and_then(|slot| {
                        let range = slot.effect.param_descriptors().get(param_index)?;
                        let (low, high) = (range.min, range.max);
                        let current = slot.effect.get_param(param_index);
                        let value = apply(current, low, high, &mut state);
                        if value != current {
                            slot.effect.set_automated_param(param_index, value);
                        }
                        state.written = slot.effect.get_param(param_index);
                        Some(())
                    }),
                AutomationTarget::InstrumentParam { param_index } => {
                    self.instrument.as_mut().and_then(|instrument| {
                        let range = instrument.param_descriptors().get(param_index)?;
                        let (low, high) = (range.min, range.max);
                        let current = instrument.get_param(param_index);
                        let value = apply(current, low, high, &mut state);
                        if value != current {
                            instrument.set_param(param_index, value);
                        }
                        state.written = instrument.get_param(param_index);
                        Some(())
                    })
                }
                _ => None,
            };
            // A parameter no longer linked, or gone with its device, is
            // forgotten once restored.
            self.modulation.parameters[index] = (linked && found.is_some()).then_some(state);
        }
    }
}

#[cfg(test)]
#[path = "modulation_tests.rs"]
mod tests;
