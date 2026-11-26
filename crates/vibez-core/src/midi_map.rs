//! Hardware MIDI controllers bound to project parameters.

use serde::{Deserialize, Serialize};

use crate::automation::AutomationTarget;
use crate::id::TrackId;

/// One controller number driving one parameter of one channel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MidiMapping {
    /// MIDI Control Change number, 0..=127, on any channel.
    pub cc: u8,
    pub track_id: TrackId,
    pub target: AutomationTarget,
}

/// Binds `mapping`, replacing whatever its controller drove and whatever
/// drove its parameter, so each side keeps a single partner.
pub fn bind(mappings: &mut Vec<MidiMapping>, mapping: MidiMapping) {
    mappings.retain(|old| {
        old.cc != mapping.cc && (old.track_id, old.target) != (mapping.track_id, mapping.target)
    });
    mappings.push(mapping);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_controller_and_a_parameter_each_keep_one_partner() {
        let track = TrackId::new();
        let map = |cc, target| MidiMapping {
            cc,
            track_id: track,
            target,
        };
        let mut mappings = Vec::new();
        bind(&mut mappings, map(1, AutomationTarget::TrackGain));
        bind(&mut mappings, map(2, AutomationTarget::TrackPan));
        // CC 1 moves to the pan: the volume is freed and CC 2 dropped.
        bind(&mut mappings, map(1, AutomationTarget::TrackPan));
        assert_eq!(mappings, [map(1, AutomationTarget::TrackPan)]);
    }
}
