use super::{new_master_track, ProjectTrack, TrackId};

/// Project-owned tracks and channels shared by every musical timeline.
#[derive(Debug, Clone)]
pub struct ProjectTracksState {
    pub reduced_latency_monitoring: bool,
    /// Hardware controllers bound to parameters, saved with the project.
    pub midi_mappings: Vec<vibez_core::midi_map::MidiMapping>,
    pub tracks: Vec<ProjectTrack>,
    /// The master bus channel (see [`new_master_track`]).
    pub master: ProjectTrack,
    /// Return channels: mixer-only tracks fed by per-track sends.
    pub buses: Vec<ProjectTrack>,
    pub next_track_number: u32,
}

impl Default for ProjectTracksState {
    fn default() -> Self {
        Self {
            reduced_latency_monitoring: false,
            midi_mappings: Vec::new(),
            tracks: Vec::new(),
            master: new_master_track(),
            buses: Vec::new(),
            next_track_number: 0,
        }
    }
}

/// [`ProjectTracksState::send_would_loop`] for views that hold the buses.
pub fn send_would_loop(buses: &[ProjectTrack], from: TrackId, to: TrackId) -> bool {
    let mut pending = vec![to];
    let mut seen = Vec::new();
    while let Some(id) = pending.pop() {
        if id == from {
            return true;
        }
        if !seen.contains(&id) {
            seen.push(id);
            if let Some(bus) = buses.iter().find(|bus| bus.id == id) {
                pending.extend(bus.sends.iter().map(|(target, _)| *target));
            }
        }
    }
    false
}

impl ProjectTracksState {
    pub fn find(&self, id: TrackId) -> Option<&ProjectTrack> {
        if id.is_master() {
            return Some(&self.master);
        }
        self.tracks
            .iter()
            .chain(self.buses.iter())
            .find(|t| t.id == id)
    }

    /// The groups holding `id`, the nearest first.
    pub fn ancestors(&self, id: TrackId) -> Vec<TrackId> {
        let mut found = Vec::new();
        let mut at = id;
        while let Some(group) = self.sender(at).and_then(|channel| channel.output) {
            // A broken file could point in a circle.
            if group == id || found.contains(&group) {
                break;
            }
            if !self.buses.iter().any(|bus| bus.id == group) {
                break;
            }
            found.push(group);
            at = group;
        }
        found
    }

    /// Whether a folded group hides `id` in Arrange.
    pub fn is_folded_away(&self, id: TrackId) -> bool {
        self.ancestors(id)
            .iter()
            .any(|group| self.buses.iter().any(|bus| bus.id == *group && bus.folded))
    }

    /// Moves a track, or a group with all its tracks, past its neighbour
    /// inside the same group. Returns whether anything moved.
    pub fn move_among_siblings(&mut self, id: TrackId, up: bool) -> bool {
        let Some(parent) = self.sender(id).map(|channel| channel.output) else {
            return false;
        };
        // What stands for each track at the moved item's level: itself, or
        // the sibling group holding it; `None` outside the parent.
        let units: Vec<Option<TrackId>> = self
            .tracks
            .iter()
            .map(|track| {
                let mut chain = self.ancestors(track.id);
                chain.reverse();
                chain.push(track.id);
                match parent {
                    Some(parent) => {
                        let at = chain.iter().position(|&step| step == parent)?;
                        chain.get(at + 1).copied()
                    }
                    None => chain.first().copied(),
                }
            })
            .collect();
        let block = |unit: TrackId| {
            let start = units.iter().position(|&u| u == Some(unit))?;
            let len = units[start..]
                .iter()
                .take_while(|&&u| u == Some(unit))
                .count();
            Some(start..start + len)
        };
        let Some(moved) = block(id) else {
            return false;
        };
        let neighbour = if up {
            moved.start.checked_sub(1)
        } else {
            Some(moved.end)
        };
        let Some(other) = neighbour
            .and_then(|at| units.get(at).copied().flatten())
            .and_then(block)
        else {
            return false;
        };
        if up {
            self.tracks[other.start..moved.end].rotate_left(other.len());
        } else {
            self.tracks[moved.start..other.end].rotate_left(moved.len());
        }
        true
    }

    /// Reorders the tracks so each group's members sit together, where its
    /// first member is. Returns whether anything moved.
    pub fn gather_groups(&mut self) -> bool {
        let chains: Vec<Vec<TrackId>> = self
            .tracks
            .iter()
            .map(|track| {
                let mut chain = self.ancestors(track.id);
                chain.reverse();
                chain
            })
            .collect();
        let first_of = |group: TrackId| {
            chains
                .iter()
                .position(|chain| chain.contains(&group))
                .unwrap_or(usize::MAX)
        };
        // Outermost group first: members of one group compare equal until
        // their own place inside it.
        let keys: Vec<Vec<usize>> = chains
            .iter()
            .enumerate()
            .map(|(index, chain)| {
                chain
                    .iter()
                    .map(|&group| first_of(group))
                    .chain(std::iter::once(index))
                    .collect()
            })
            .collect();
        let mut order: Vec<usize> = (0..self.tracks.len()).collect();
        order.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
        if order.iter().enumerate().all(|(at, &from)| at == from) {
            return false;
        }
        let mut taken: Vec<Option<ProjectTrack>> = std::mem::take(&mut self.tracks)
            .into_iter()
            .map(Some)
            .collect();
        self.tracks = order
            .into_iter()
            .filter_map(|index| taken[index].take())
            .collect();
        true
    }

    /// A channel that can send to a bus: a track or a bus.
    pub fn sender(&self, id: TrackId) -> Option<&ProjectTrack> {
        self.tracks
            .iter()
            .chain(self.buses.iter())
            .find(|t| t.id == id)
    }

    pub fn sender_mut(&mut self, id: TrackId) -> Option<&mut ProjectTrack> {
        self.tracks
            .iter_mut()
            .chain(self.buses.iter_mut())
            .find(|t| t.id == id)
    }

    /// Whether a send from `from` to the bus `to` would feed `from` back
    /// into itself, directly or through other buses' sends.
    pub fn send_would_loop(&self, from: TrackId, to: TrackId) -> bool {
        send_would_loop(&self.buses, from, to)
    }

    /// Every channel but the master, in mixer order: tracks, then buses.
    pub fn channel_ids(&self) -> impl Iterator<Item = TrackId> + '_ {
        self.tracks
            .iter()
            .chain(self.buses.iter())
            .map(|track| track.id)
    }

    /// Every channel the engine mixes: tracks, the master, then buses.
    pub fn channels(&self) -> impl Iterator<Item = &ProjectTrack> {
        self.tracks
            .iter()
            .chain(std::iter::once(&self.master))
            .chain(&self.buses)
    }

    pub fn find_mut(&mut self, id: TrackId) -> Option<&mut ProjectTrack> {
        if id.is_master() {
            return Some(&mut self.master);
        }
        self.tracks
            .iter_mut()
            .chain(self.buses.iter_mut())
            .find(|t| t.id == id)
    }
}
