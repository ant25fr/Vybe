//! The Stereo Widener's live scope: the side signal of its own output,
//! analysed like the EQ's spectrum.

use rtrb::Consumer;
use vibez_core::effect::EffectType;
use vibez_core::id::{EffectId, TrackId};
use vibez_engine::effect_spectrum::{
    EffectSpectrumPosition, EffectSpectrumSample, EffectSpectrumTarget,
};

use crate::spectrum::SpectrumState;
use crate::state::DetailPanelTab;

use super::*;

#[derive(Default)]
pub(super) struct WidenerDevices {
    pub spectrum_rx: Option<Consumer<EffectSpectrumSample>>,
    pub mid_rx: Option<Consumer<EffectSpectrumSample>>,
    pub target: Option<EffectSpectrumTarget>,
    /// The widener last adjusted, which the analyser follows.
    pub focus: Option<EffectId>,
    pub generation: u64,
    /// The output's side signal.
    pub analyser: SpectrumState,
    /// The output's mono sum.
    pub mid_analyser: SpectrumState,
}

impl WidenerDevices {
    /// The (side, mid) analysers for this widener's card, when it is the
    /// one listened to.
    pub fn spectrum_for(&self, effect_id: EffectId) -> Option<(&SpectrumState, &SpectrumState)> {
        (self.target.map(|target| target.effect_id) == Some(effect_id))
            .then_some((&self.analyser, &self.mid_analyser))
    }
}

impl App {
    /// The selected track's last adjusted Stereo Widener, else its first,
    /// while the Devices tab is showing.
    fn wanted_widener_tap(&self) -> Option<(TrackId, EffectId)> {
        if self.state.view.detail_panel_tab != DetailPanelTab::Devices {
            return None;
        }
        let track_id = self.state.active_timeline_editor().selected_track?;
        let track = self.state.find_track(track_id)?;
        let mut wideners = track.effects.iter().filter(|effect| {
            effect.effect_type == EffectType::StereoWidener
                && effect.plugin_ref.is_none()
                && effect.plugin_name.is_none()
        });
        let effect = wideners
            .clone()
            .find(|effect| Some(effect.id) == self.widener_devices.focus)
            .or_else(|| wideners.next())?;
        Some((track_id, effect.id))
    }

    pub(super) fn poll_widener_devices(&mut self) {
        let wanted = self.wanted_widener_tap();
        let current = self
            .widener_devices
            .target
            .map(|target| (target.track_id, target.effect_id));
        if wanted != current {
            let devices = &mut self.widener_devices;
            devices.generation = devices.generation.wrapping_add(1);
            let generation = devices.generation;
            devices.target = wanted.map(|(track_id, effect_id)| EffectSpectrumTarget {
                track_id,
                effect_id,
                position: EffectSpectrumPosition::Output,
                generation,
            });
            devices.analyser.reset();
            devices.mid_analyser.reset();
            let target = devices.target;
            self.send_command(EngineCommand::SetSideSpectrumTap(target));
        }
        let devices = &mut self.widener_devices;
        let generation = devices.generation;
        for (rx, analyser) in [
            (devices.spectrum_rx.as_mut(), &mut devices.analyser),
            (devices.mid_rx.as_mut(), &mut devices.mid_analyser),
        ] {
            let Some(rx) = rx else {
                continue;
            };
            let mut chunk = [0.0f32; 512];
            loop {
                let mut filled = 0;
                while filled < chunk.len() {
                    let Ok(sample) = rx.pop() else {
                        break;
                    };
                    if sample.generation == generation {
                        chunk[filled] = sample.value;
                        filled += 1;
                    }
                }
                analyser.ingest(&chunk[..filled]);
                if filled < chunk.len() {
                    break;
                }
            }
        }
        if devices.target.is_some() {
            let rate = self.state.transport.sample_rate as f32;
            devices.analyser.analyse(rate);
            devices.mid_analyser.analyse(rate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::devices::DevicesMsg;
    use crate::state::ProjectTrack;

    #[test]
    fn the_scope_listens_to_the_selected_tracks_widener() {
        let mut app = super::super::test_support::app();
        let track = ProjectTrack::new(TrackId::new(), "Bus".into(), 2);
        let id = track.id;
        Arc::make_mut(&mut app.state.project_tracks)
            .tracks
            .push(track);
        let _ = app.update(Message::Devices(DevicesMsg::AddEffect(
            id,
            EffectType::StereoWidener,
        )));
        let effect = app.state.find_track(id).unwrap().effects[0].id;
        app.state.arrangement.selected_track = Some(id);
        app.state.view.detail_panel_tab = DetailPanelTab::Devices;
        app.poll_widener_devices();
        let target = app.widener_devices.target.expect("a tap for the widener");
        assert_eq!((target.track_id, target.effect_id), (id, effect));
        assert!(app.widener_devices.spectrum_for(effect).is_some());

        // Adjusting a second widener moves the analyser to it.
        let _ = app.update(Message::Devices(DevicesMsg::AddEffect(
            id,
            EffectType::StereoWidener,
        )));
        let second = app.state.find_track(id).unwrap().effects[1].id;
        let _ = app.update(Message::set_effect_param(id, second, 0, 1.5));
        app.poll_widener_devices();
        assert!(app.widener_devices.spectrum_for(second).is_some());
        assert!(app.widener_devices.spectrum_for(effect).is_none());
    }
}
