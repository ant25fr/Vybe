//! The native Multiband Dynamics card: its per-band reduction scope's
//! panel state, as the engine measures it.

use std::time::{Duration, Instant};

use vibez_core::effect::DynamicsMeter;
use vibez_engine::metering::MeterValues;

use super::device_levels::ReadingHistory;

/// How far back the multiband scope's readings reach.
pub const MULTIBAND_HISTORY: Duration = Duration::from_secs(12);

pub fn is_native_multiband_dynamics(effect: &crate::state::UiEffect) -> bool {
    effect.effect_type == vibez_core::effect::EffectType::MultibandDynamics
        && effect.plugin_ref.is_none()
        && effect.plugin_name.is_none()
}

/// One reading from the multiband dynamics, in decibels: the loudest input
/// since the last, and each band's peak reduction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MultibandReading {
    pub input_db: f32,
    pub band_reduction_db: [f32; 3],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MultibandPanelState {
    pub input: MeterValues,
    pub output: MeterValues,
    /// Peak reduction per band in the latest reading.
    pub band_reduction_db: [f32; 3],
    pub bands: ReadingHistory<MultibandReading>,
    pub last_meter_at: Option<Instant>,
}

impl MultibandPanelState {
    pub fn accept_dynamics(&mut self, meter: DynamicsMeter, now: Instant) {
        let input = meter.detector_peak[0].max(meter.detector_peak[1]);
        self.band_reduction_db = meter.band_reduction_db;
        self.last_meter_at = Some(now);
        self.bands.push(
            now,
            MultibandReading {
                input_db: vibez_core::level::amplitude_to_db(input),
                band_reduction_db: meter.band_reduction_db,
            },
            MULTIBAND_HISTORY,
        );
    }
}

impl super::device_levels::MeteredPanel for MultibandPanelState {
    fn accept_levels(&mut self, input: MeterValues, output: MeterValues, now: Instant) {
        self.input = input;
        self.output = output;
        self.last_meter_at = Some(now);
    }

    fn reset_stale(&mut self, now: Instant) {
        if self
            .last_meter_at
            .is_some_and(|last| now.saturating_duration_since(last) >= Duration::from_millis(250))
        {
            self.clear_readings();
        }
    }

    fn clear_readings(&mut self) {
        self.input = MeterValues::SILENT;
        self.output = MeterValues::SILENT;
        self.band_reduction_db = [0.0; 3];
        self.bands.clear();
        self.last_meter_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::device_levels::MeteredPanel;

    #[test]
    fn band_readings_keep_the_deepest_reduction_per_band() {
        let now = Instant::now();
        let mut panel = MultibandPanelState::default();
        panel.accept_dynamics(
            DynamicsMeter {
                band_reduction_db: [4.0, 1.5, 0.0],
                detector_peak: [0.5, 0.25],
                ..Default::default()
            },
            now,
        );
        assert_eq!(panel.band_reduction_db, [4.0, 1.5, 0.0]);
        let (_, reading) = panel.bands.iter().next().copied().unwrap();
        assert!((reading.input_db - -6.02).abs() < 0.01, "{reading:?}");
        assert_eq!(reading.band_reduction_db, [4.0, 1.5, 0.0]);
        panel.reset_stale(now + Duration::from_millis(250));
        assert!(panel.bands.is_empty() && panel.band_reduction_db == [0.0; 3]);
    }
}
