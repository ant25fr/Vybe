//! An audio effect rack: parallel chains of built-in effects, with macros that
//! move their parameters.

use serde::{Deserialize, Serialize};

use crate::effect::{EffectType, ParamDescriptor};

pub const MAX_CHAINS: usize = 4;
pub const MAX_CHAIN_DEVICES: usize = 4;
pub const MACROS: usize = 8;
pub const MAX_MAPPINGS: usize = 32;

/// Rack parameter indices: the macros, then each chain's volume and switch.
pub const CHAIN_VOLUME: usize = MACROS;
pub const CHAIN_ON: usize = CHAIN_VOLUME + MAX_CHAINS;
pub const PARAM_COUNT: usize = CHAIN_ON + MAX_CHAINS;

/// Parameters of devices inside chains are addressed above the rack's own.
const INNER_BASE: usize = 1_000;
const DEVICE_SPAN: usize = 100;
const CHAIN_SPAN: usize = DEVICE_SPAN * MAX_CHAIN_DEVICES;

/// The index that sets `param` of the device at `device` in `chain`.
pub fn inner_param(chain: usize, device: usize, param: usize) -> usize {
    INNER_BASE + chain * CHAIN_SPAN + device * DEVICE_SPAN + param
}

/// The chain, device and parameter an inner index addresses.
pub fn split_inner_param(index: usize) -> Option<(usize, usize, usize)> {
    let offset = index.checked_sub(INNER_BASE)?;
    let chain = offset / CHAIN_SPAN;
    let rest = offset % CHAIN_SPAN;
    (chain < MAX_CHAINS).then_some((chain, rest / DEVICE_SPAN, rest % DEVICE_SPAN))
}

const fn macro_knob(name: &'static str) -> ParamDescriptor {
    ParamDescriptor {
        name,
        min: 0.0,
        max: 1.0,
        default: 0.0,
        unit: "",
    }
}

const fn chain_volume(name: &'static str) -> ParamDescriptor {
    ParamDescriptor {
        name,
        min: 0.0,
        max: 2.0,
        default: 1.0,
        unit: "",
    }
}

const fn chain_switch(name: &'static str) -> ParamDescriptor {
    ParamDescriptor {
        name,
        min: 0.0,
        max: 1.0,
        default: 1.0,
        unit: "",
    }
}

pub static RACK_PARAMS: &[ParamDescriptor] = &[
    macro_knob("Macro 1"),
    macro_knob("Macro 2"),
    macro_knob("Macro 3"),
    macro_knob("Macro 4"),
    macro_knob("Macro 5"),
    macro_knob("Macro 6"),
    macro_knob("Macro 7"),
    macro_knob("Macro 8"),
    chain_volume("Chain 1 Volume"),
    chain_volume("Chain 2 Volume"),
    chain_volume("Chain 3 Volume"),
    chain_volume("Chain 4 Volume"),
    chain_switch("Chain 1 On"),
    chain_switch("Chain 2 On"),
    chain_switch("Chain 3 On"),
    chain_switch("Chain 4 On"),
];

/// One effect inside a chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RackDevice {
    pub effect_type: EffectType,
    #[serde(default)]
    pub params: Vec<f32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

/// Effects in series; an empty chain passes the dry signal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RackChain {
    #[serde(default)]
    pub devices: Vec<RackDevice>,
}

/// A macro moving one parameter from `min` (macro at zero) to `max`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MacroMapping {
    pub macro_index: u8,
    pub chain: u8,
    pub device: u8,
    pub param: u8,
    pub min: f32,
    pub max: f32,
}

impl MacroMapping {
    pub fn value_at(&self, amount: f32) -> f32 {
        self.min + amount.clamp(0.0, 1.0) * (self.max - self.min)
    }

    pub fn targets(&self, chain: usize, device: usize, param: usize) -> bool {
        (
            usize::from(self.chain),
            usize::from(self.device),
            usize::from(self.param),
        ) == (chain, device, param)
    }
}

/// What a rack holds. Its macros, chain volumes and switches are the effect's
/// parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RackState {
    #[serde(default)]
    pub chains: Vec<RackChain>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<MacroMapping>,
}

impl Default for RackState {
    /// One empty chain, which passes the signal unchanged.
    fn default() -> Self {
        Self {
            chains: vec![RackChain::default()],
            mappings: Vec::new(),
        }
    }
}

impl RackState {
    /// Brought within the rack's limits: no nested racks, no mapping to a
    /// parameter that is not there.
    pub fn sanitized(mut self) -> Self {
        self.chains.truncate(MAX_CHAINS);
        for chain in &mut self.chains {
            chain
                .devices
                .retain(|device| device.effect_type != EffectType::Rack);
            chain.devices.truncate(MAX_CHAIN_DEVICES);
        }
        let chains = &self.chains;
        self.mappings.retain(|mapping| {
            usize::from(mapping.macro_index) < MACROS
                && mapping.min.is_finite()
                && mapping.max.is_finite()
                && chains
                    .get(usize::from(mapping.chain))
                    .is_some_and(|chain| usize::from(mapping.device) < chain.devices.len())
        });
        self.mappings.truncate(MAX_MAPPINGS);
        self
    }

    /// Everything but the device parameter values: what needs the running
    /// rack rebuilt when it differs.
    pub fn same_structure(&self, other: &Self) -> bool {
        self.mappings == other.mappings
            && self.chains.len() == other.chains.len()
            && self.chains.iter().zip(&other.chains).all(|(a, b)| {
                a.devices.len() == b.devices.len()
                    && a.devices
                        .iter()
                        .zip(&b.devices)
                        .all(|(a, b)| a.effect_type == b.effect_type && a.bypass == b.bypass)
            })
    }

    pub fn mapping_of(&self, chain: usize, device: usize, param: usize) -> Option<MacroMapping> {
        self.mappings
            .iter()
            .copied()
            .find(|mapping| mapping.targets(chain, device, param))
    }

    /// Remove `chain`, keeping the mappings of the chains after it.
    pub fn remove_chain(&mut self, chain: usize) {
        if chain >= self.chains.len() {
            return;
        }
        self.chains.remove(chain);
        self.mappings
            .retain(|mapping| usize::from(mapping.chain) != chain);
        for mapping in &mut self.mappings {
            if usize::from(mapping.chain) > chain {
                mapping.chain -= 1;
            }
        }
    }

    /// Remove a device, keeping the mappings of the devices after it.
    pub fn remove_device(&mut self, chain: usize, device: usize) {
        let Some(devices) = self.chains.get_mut(chain).map(|chain| &mut chain.devices) else {
            return;
        };
        if device >= devices.len() {
            return;
        }
        devices.remove(device);
        self.mappings.retain(|mapping| {
            (usize::from(mapping.chain), usize::from(mapping.device)) != (chain, device)
        });
        for mapping in &mut self.mappings {
            if usize::from(mapping.chain) == chain && usize::from(mapping.device) > device {
                mapping.device -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> RackDevice {
        RackDevice {
            effect_type: EffectType::Filter,
            params: Vec::new(),
            bypass: false,
        }
    }

    fn mapping(chain: u8, device: u8) -> MacroMapping {
        MacroMapping {
            macro_index: 0,
            chain,
            device,
            param: 0,
            min: 100.0,
            max: 200.0,
        }
    }

    #[test]
    fn inner_indices_round_trip_and_stay_above_the_rack_s_own() {
        assert!(inner_param(0, 0, 0) >= PARAM_COUNT);
        assert_eq!(split_inner_param(inner_param(3, 2, 41)), Some((3, 2, 41)));
        assert_eq!(split_inner_param(5), None);
        assert_eq!(RACK_PARAMS.len(), PARAM_COUNT);
    }

    #[test]
    fn a_mapping_spans_its_range() {
        assert_eq!(mapping(0, 0).value_at(0.5), 150.0);
        assert_eq!(mapping(0, 0).value_at(3.0), 200.0);
    }

    #[test]
    fn removing_a_chain_or_device_keeps_the_other_mappings_on_target() {
        let mut rack = RackState {
            chains: vec![
                RackChain {
                    devices: vec![device(), device()],
                },
                RackChain {
                    devices: vec![device()],
                },
            ],
            mappings: vec![mapping(0, 0), mapping(0, 1), mapping(1, 0)],
        };
        rack.remove_device(0, 0);
        assert_eq!(rack.mappings, vec![mapping(0, 0), mapping(1, 0)]);
        rack.remove_chain(0);
        assert_eq!(rack.mappings, vec![mapping(0, 0)]);
    }

    #[test]
    fn nested_racks_and_dangling_mappings_are_dropped() {
        let rack = RackState {
            chains: vec![RackChain {
                devices: vec![RackDevice {
                    effect_type: EffectType::Rack,
                    params: Vec::new(),
                    bypass: false,
                }],
            }],
            mappings: vec![mapping(0, 0), mapping(2, 0)],
        }
        .sanitized();
        assert!(rack.chains[0].devices.is_empty());
        assert!(rack.mappings.is_empty());
    }
}
