//! Tempo automation: the master lane that moves the Arrange tempo.
//!
//! Timeline positions stay counted at the project tempo, so one beat is always
//! the same number of positions. The lane changes how fast the transport moves
//! through them, and audio follows by re-pitching, like Ableton's Re-Pitch.

use crate::automation::{AutomationLane, AutomationTarget};

/// Range of a tempo lane, whose values are normalized 0..1.
pub const MIN_AUTOMATED_BPM: f64 = 40.0;
pub const MAX_AUTOMATED_BPM: f64 = 240.0;

pub fn bpm_from_normalized(value: f32) -> f64 {
    MIN_AUTOMATED_BPM + f64::from(value.clamp(0.0, 1.0)) * (MAX_AUTOMATED_BPM - MIN_AUTOMATED_BPM)
}

pub fn normalized_from_bpm(bpm: f64) -> f32 {
    ((bpm - MIN_AUTOMATED_BPM) / (MAX_AUTOMATED_BPM - MIN_AUTOMATED_BPM)).clamp(0.0, 1.0) as f32
}

/// The tempo lane among the master channel's lanes, if it draws anything.
pub fn tempo_lane(lanes: &[AutomationLane]) -> Option<&AutomationLane> {
    lanes
        .iter()
        .find(|lane| lane.target == AutomationTarget::Tempo && !lane.points.is_empty())
}

/// The tempo at `beat`: the lane's, or `project_bpm` without one.
pub fn bpm_at(lanes: &[AutomationLane], beat: f64, project_bpm: f64) -> f64 {
    tempo_lane(lanes)
        .and_then(|lane| lane.value_at(beat))
        .map_or(project_bpm, bpm_from_normalized)
}

/// Seconds from the song start to `beat`, through the tempo lane.
pub fn seconds_at(lanes: &[AutomationLane], beat: f64, project_bpm: f64) -> f64 {
    let beat = beat.max(0.0);
    let Some(lane) = tempo_lane(lanes) else {
        return beat * 60.0 / project_bpm;
    };
    // Past the last point the tempo holds, so only the drawn span is summed.
    let drawn = lane.points.last().map_or(0.0, |point| point.beat).min(beat);
    let steps = (drawn * 4.0).ceil() as usize;
    let mut seconds = 0.0;
    for step in 0..steps {
        let from = step as f64 * 0.25;
        let to = (from + 0.25).min(drawn);
        seconds += (to - from) * 60.0 / bpm_at(lanes, (from + to) * 0.5, project_bpm);
    }
    seconds + (beat - drawn) * 60.0 / bpm_at(lanes, beat, project_bpm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::AutomationPoint;

    #[test]
    fn a_lane_moves_the_tempo_and_its_absence_leaves_the_project_tempo() {
        let mut lane = AutomationLane::new(AutomationTarget::Tempo);
        assert_eq!(bpm_at(std::slice::from_ref(&lane), 0.0, 128.0), 128.0);
        for (beat, bpm) in [(0.0, 90.0), (8.0, 130.0)] {
            lane.insert_point(AutomationPoint {
                beat,
                value: normalized_from_bpm(bpm),
                curve: 0.0,
            });
        }
        let lanes = [lane];
        assert!((bpm_at(&lanes, 4.0, 128.0) - 110.0).abs() < 1e-3);
        assert!((bpm_at(&lanes, 20.0, 128.0) - 130.0).abs() < 1e-3);
    }

    #[test]
    fn elapsed_time_follows_the_lane() {
        let mut lane = AutomationLane::new(AutomationTarget::Tempo);
        lane.insert_point(AutomationPoint {
            beat: 0.0,
            value: normalized_from_bpm(60.0),
            curve: 0.0,
        });
        assert!((seconds_at(std::slice::from_ref(&lane), 8.0, 120.0) - 8.0).abs() < 1e-3);
        assert_eq!(seconds_at(&[], 8.0, 120.0), 4.0);
    }

    #[test]
    fn the_range_clamps() {
        assert_eq!(bpm_from_normalized(2.0), MAX_AUTOMATED_BPM);
        assert_eq!(normalized_from_bpm(1.0), 0.0);
    }
}
