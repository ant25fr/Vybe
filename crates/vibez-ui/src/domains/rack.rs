//! Editing an Effect Rack: its chains, the devices inside them and what the
//! macros move.

use std::collections::HashMap;
use std::sync::OnceLock;

use vibez_core::effect::{EffectInfo, EffectType, ParamDescriptor};
use vibez_core::id::{EffectId, TrackId};
use vibez_core::rack::{
    inner_param, MacroMapping, RackChain, RackDevice, RackState, CHAIN_ON, CHAIN_VOLUME, MACROS,
    MAX_CHAINS, MAX_CHAIN_DEVICES, MAX_MAPPINGS, RACK_PARAMS,
};
use vibez_dsp::rack::RackEffect;
use vibez_engine::commands::EngineCommand;

use super::EngineHandle;
use crate::state::{ProjectTracksState, UiEffect};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RackEdit {
    /// Show a chain and one of its devices.
    Select {
        chain: usize,
        device: usize,
    },
    AddChain,
    RemoveChain(usize),
    AddDevice {
        chain: usize,
        effect_type: EffectType,
    },
    RemoveDevice {
        chain: usize,
        device: usize,
    },
    ToggleDevice {
        chain: usize,
        device: usize,
    },
    SetDeviceParam {
        chain: usize,
        device: usize,
        param: usize,
        value: f32,
    },
    /// Give a parameter to a macro, or take it back.
    Map {
        chain: usize,
        device: usize,
        param: usize,
        macro_index: Option<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RackMsg {
    pub track_id: TrackId,
    pub effect_id: EffectId,
    pub edit: RackEdit,
}

/// The parameters of a built-in effect a rack can hold.
pub fn descriptors(effect_type: EffectType) -> &'static [ParamDescriptor] {
    // The card asks on every frame, and building an effect allocates.
    static TABLES: OnceLock<Vec<(EffectType, &'static [ParamDescriptor])>> = OnceLock::new();
    TABLES
        .get_or_init(|| {
            EffectType::all()
                .iter()
                .map(|&kind| {
                    let device = vibez_dsp::factory::create_effect(kind, 48_000.0);
                    (kind, device.param_descriptors())
                })
                .collect()
        })
        .iter()
        .find(|(kind, _)| *kind == effect_type)
        .map_or(&[], |(_, table)| table)
}

/// The rack of a saved effect, within the limits the card and engine expect.
pub fn loaded_rack(info: &EffectInfo) -> Option<RackState> {
    (info.effect_type == EffectType::Rack)
        .then(|| info.rack.clone().unwrap_or_default().sanitized())
}

/// A device's value for `param`: its macro's when mapped, else its own.
pub fn device_value(
    rack: &RackState,
    params: &[f32],
    chain: usize,
    device: usize,
    param: usize,
    descriptor: &ParamDescriptor,
) -> f32 {
    if let Some(mapping) = rack.mapping_of(chain, device, param) {
        let amount = params
            .get(usize::from(mapping.macro_index))
            .copied()
            .unwrap_or(0.0);
        return mapping.value_at(amount);
    }
    rack.chains
        .get(chain)
        .and_then(|chain| chain.devices.get(device))
        .and_then(|device| device.params.get(param))
        .copied()
        .unwrap_or(descriptor.default)
}

impl RackMsg {
    /// The effect after this message, when it changes it.
    pub fn edited(&self, tracks: &ProjectTracksState) -> Option<UiEffect> {
        let effect = tracks
            .find(self.track_id)?
            .effects
            .iter()
            .find(|effect| effect.id == self.effect_id)?;
        let mut next = effect.clone();
        let rack = next.rack.as_mut()?;
        match self.edit {
            RackEdit::Select { .. } => return None,
            RackEdit::AddChain => {
                if rack.chains.len() >= MAX_CHAINS {
                    return None;
                }
                rack.chains.push(RackChain::default());
                let index = rack.chains.len() - 1;
                set(&mut next.params, CHAIN_VOLUME + index, 1.0);
                set(&mut next.params, CHAIN_ON + index, 1.0);
            }
            RackEdit::RemoveChain(chain) => {
                if chain >= rack.chains.len() || rack.chains.len() == 1 {
                    return None;
                }
                rack.remove_chain(chain);
                // The later chains keep their volume and switch.
                for start in [CHAIN_VOLUME, CHAIN_ON] {
                    for index in chain..MAX_CHAINS {
                        let value = if index + 1 < MAX_CHAINS {
                            next.params.get(start + index + 1).copied().unwrap_or(1.0)
                        } else {
                            1.0
                        };
                        set(&mut next.params, start + index, value);
                    }
                }
            }
            RackEdit::AddDevice { chain, effect_type } => {
                let devices = &mut rack.chains.get_mut(chain)?.devices;
                if devices.len() >= MAX_CHAIN_DEVICES || effect_type == EffectType::Rack {
                    return None;
                }
                devices.push(RackDevice {
                    effect_type,
                    params: descriptors(effect_type)
                        .iter()
                        .map(|descriptor| descriptor.default)
                        .collect(),
                    bypass: false,
                });
            }
            RackEdit::RemoveDevice { chain, device } => rack.remove_device(chain, device),
            RackEdit::ToggleDevice { chain, device } => {
                let device = rack.chains.get_mut(chain)?.devices.get_mut(device)?;
                device.bypass = !device.bypass;
            }
            RackEdit::SetDeviceParam {
                chain,
                device,
                param,
                value,
            } => {
                // A mapped parameter belongs to its macro.
                if !value.is_finite() || rack.mapping_of(chain, device, param).is_some() {
                    return None;
                }
                let slot = rack.chains.get_mut(chain)?.devices.get_mut(device)?;
                let descriptor = descriptors(slot.effect_type).get(param)?;
                if slot.params.len() <= param {
                    slot.params = descriptors(slot.effect_type)
                        .iter()
                        .enumerate()
                        .map(|(index, descriptor)| {
                            slot.params
                                .get(index)
                                .copied()
                                .unwrap_or(descriptor.default)
                        })
                        .collect();
                }
                slot.params[param] = value.clamp(descriptor.min, descriptor.max);
            }
            RackEdit::Map {
                chain,
                device,
                param,
                macro_index,
            } => {
                let effect_type = rack.chains.get(chain)?.devices.get(device)?.effect_type;
                let descriptor = descriptors(effect_type).get(param)?;
                let current = device_value(rack, &next.params, chain, device, param, descriptor);
                rack.mappings
                    .retain(|mapping| !mapping.targets(chain, device, param));
                match macro_index {
                    // The parameter keeps the value the macro gave it.
                    None => {
                        let slot = &mut rack.chains[chain].devices[device];
                        if let Some(value) = slot.params.get_mut(param) {
                            *value = current;
                        }
                    }
                    Some(index) => {
                        if index >= MACROS || rack.mappings.len() >= MAX_MAPPINGS {
                            return None;
                        }
                        let unused = !rack
                            .mappings
                            .iter()
                            .any(|mapping| usize::from(mapping.macro_index) == index);
                        rack.mappings.push(MacroMapping {
                            macro_index: index as u8,
                            chain: chain as u8,
                            device: device as u8,
                            param: param as u8,
                            min: descriptor.min,
                            max: descriptor.max,
                        });
                        // A free macro starts where the parameter is, so
                        // mapping it does not change the sound.
                        let span = descriptor.max - descriptor.min;
                        if unused && span.abs() > f32::EPSILON {
                            set(
                                &mut next.params,
                                index,
                                ((current - descriptor.min) / span).clamp(0.0, 1.0),
                            );
                        }
                    }
                }
            }
        }
        let changed = next.rack != effect.rack || next.params != effect.params;
        changed.then_some(next)
    }
}

fn set(params: &mut Vec<f32>, index: usize, value: f32) {
    while params.len() <= index {
        let default = RACK_PARAMS.get(params.len()).map_or(0.0, |d| d.default);
        params.push(default);
    }
    params[index] = value;
}

/// The command putting `effect`'s rack into a chain at `position`.
pub fn insert_command(
    track_id: TrackId,
    effect: &UiEffect,
    rack: &RackState,
    sample_rate: u32,
    position: Option<usize>,
) -> (EngineCommand, u32) {
    let device = RackEffect::new(rack, sample_rate as f32, &effect.params);
    let latency = vibez_dsp::effect::AudioEffect::latency_samples(&device);
    (
        EngineCommand::InsertEffect {
            track_id,
            effect_id: effect.id,
            effect: Box::new(device),
            position,
        },
        latency,
    )
}

/// Which chain and device each rack's card shows. Not saved with the project.
#[derive(Debug, Default)]
pub struct RackUiState {
    selected: HashMap<EffectId, (usize, usize)>,
}

impl RackUiState {
    /// The chain and device shown, kept within what `rack` holds.
    pub fn selected(&self, effect_id: EffectId, rack: &RackState) -> (usize, usize) {
        let (chain, device) = self.selected.get(&effect_id).copied().unwrap_or((0, 0));
        let chain = chain.min(rack.chains.len().saturating_sub(1));
        let devices = rack
            .chains
            .get(chain)
            .map_or(0, |chain| chain.devices.len());
        (chain, device.min(devices.saturating_sub(1)))
    }

    /// Apply `msg`; returns whether the rack was rebuilt, which needs the
    /// routing prepared again.
    pub fn update(
        &mut self,
        msg: RackMsg,
        engine: &mut impl EngineHandle,
        tracks: &mut std::sync::Arc<ProjectTracksState>,
        sample_rate: u32,
    ) -> bool {
        if let RackEdit::Select { chain, device } = msg.edit {
            self.selected.insert(msg.effect_id, (chain, device));
            return false;
        }
        let Some(mut next) = msg.edited(tracks) else {
            return false;
        };
        let Some(rack) = next.rack.clone() else {
            return false;
        };
        let Some(track) = std::sync::Arc::make_mut(tracks).find_mut(msg.track_id) else {
            return false;
        };
        let Some(position) = track
            .effects
            .iter()
            .position(|effect| effect.id == msg.effect_id)
        else {
            return false;
        };
        // A knob turn reaches the running rack; anything else rebuilds it.
        if let RackEdit::SetDeviceParam {
            chain,
            device,
            param,
            ..
        } = msg.edit
        {
            if let Some(value) = rack.chains[chain].devices[device].params.get(param) {
                engine.send(EngineCommand::SetEffectParam {
                    track_id: msg.track_id,
                    effect_id: msg.effect_id,
                    param_index: inner_param(chain, device, param),
                    value: *value,
                });
            }
            track.effects[position] = next;
            return false;
        }
        match msg.edit {
            RackEdit::AddChain => {
                self.selected
                    .insert(msg.effect_id, (rack.chains.len() - 1, 0));
            }
            RackEdit::AddDevice { chain, .. } => {
                let device = rack.chains[chain].devices.len() - 1;
                self.selected.insert(msg.effect_id, (chain, device));
            }
            _ => {}
        }
        let (insert, latency) =
            insert_command(msg.track_id, &next, &rack, sample_rate, Some(position));
        engine.send(EngineCommand::RemoveEffect(msg.track_id, msg.effect_id));
        engine.send(insert);
        engine.send(EngineCommand::SetEffectBypass {
            track_id: msg.track_id,
            effect_id: msg.effect_id,
            bypass: next.bypass,
        });
        next.latency_samples = Some(latency);
        track.effects[position] = next;
        true
    }
}

#[cfg(test)]
#[path = "rack_domain_tests.rs"]
mod tests;
