//! Ruler labels and grid lines on a timeline whose time signature is not 4/4
//! throughout: bars come from the signature map, beats from each signature.

use vibez_core::meter::MeterMap;

use super::lane_grid::{GridLine, LaneGrid};
use super::ruler_scale::{label_step_beats, RulerMark};
use crate::timeline_geometry::TimelineGeometry;

const MIN_TICK_GAP_PX: f32 = 7.0;
const MIN_LINE_GAP_PX: f32 = 14.0;

/// Bars between labels at a zoom, at least one.
fn label_bars(pixels_per_beat: f32) -> u32 {
    (label_step_beats(pixels_per_beat) / 4.0).max(1.0) as u32
}

/// Each bar overlapping `width` pixels: its index, start, beat length and
/// bar length.
fn visible_bars(
    geometry: TimelineGeometry,
    width: f32,
    meter: MeterMap,
) -> impl Iterator<Item = (u32, f64, f64, f64)> {
    let last = geometry.x_to_beat(width);
    let mut bar = meter.position_at(geometry.x_to_beat(0.0)).bar;
    std::iter::from_fn(move || {
        let start = meter.bar_start(bar);
        if start > last {
            return None;
        }
        let signature = meter.signature_at_bar(bar);
        let item = (bar, start, signature.beat_length(), signature.bar_length());
        bar = bar.checked_add(1)?;
        Some(item)
    })
}

pub(super) fn metered_ruler_marks(
    geometry: TimelineGeometry,
    width: f32,
    meter: MeterMap,
) -> Vec<RulerMark> {
    let ppb = geometry.pixels_per_beat();
    if ppb <= 0.0 {
        return Vec::new();
    }
    let every = label_bars(ppb);
    let label_beats = label_step_beats(ppb) < 4.0;
    let mut marks = Vec::new();
    for (bar, start, unit, length) in visible_bars(geometry, width, meter) {
        let count = (length / unit).round() as u32;
        for beat in 0..count {
            if beat > 0 && geometry.width_for_beats(unit) < MIN_TICK_GAP_PX {
                break;
            }
            let x = geometry.beat_to_x(start + f64::from(beat) * unit).round();
            if !(0.0..=width).contains(&x) {
                continue;
            }
            let number = u64::from(bar) + 1;
            marks.push(if beat == 0 && bar % every == 0 {
                RulerMark::Label {
                    x,
                    text: number.to_string(),
                    bar: true,
                }
            } else if beat > 0 && label_beats {
                RulerMark::Label {
                    x,
                    text: format!("{number}.{}", beat + 1),
                    bar: false,
                }
            } else if beat == 0 {
                RulerMark::Mid { x }
            } else {
                RulerMark::Minor { x }
            });
        }
    }
    marks
}

pub(super) fn metered_lane_grid(
    geometry: TimelineGeometry,
    width: f32,
    step_beats: f64,
    meter: MeterMap,
) -> LaneGrid {
    let ppb = geometry.pixels_per_beat();
    if ppb <= 0.0 || step_beats <= 0.0 {
        return LaneGrid::default();
    }
    let every = label_bars(ppb);
    let mut fine = step_beats;
    while ppb * (fine as f32) < MIN_LINE_GAP_PX {
        fine *= 2.0;
    }
    let mut grid = LaneGrid::default();
    for (bar, start, _, length) in visible_bars(geometry, width, meter) {
        if (bar / every) % 2 == 1 {
            let from = geometry.beat_to_x(start).round().max(0.0);
            let to = geometry
                .beat_to_x(start + length)
                .round()
                .min(width.floor());
            if to > from {
                grid.bands.push((from, to));
            }
        }
        let labelled = bar % every == 0;
        let x = geometry.beat_to_x(start).round();
        let shown = labelled || ppb * length as f32 >= MIN_LINE_GAP_PX;
        if shown && (0.0..width).contains(&x) {
            grid.lines.push((
                x,
                if labelled {
                    GridLine::Label
                } else {
                    GridLine::Bar
                },
            ));
        }
        if fine >= length {
            continue;
        }
        let mut offset = fine;
        while offset < length - 1e-6 {
            let x = geometry.beat_to_x(start + offset).round();
            if (0.0..width).contains(&x) {
                grid.lines.push((x, GridLine::Fine));
            }
            offset += fine;
        }
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibez_core::meter::TimeSignature;

    fn waltz_from_bar_two() -> MeterMap {
        let mut meter = MeterMap::default();
        meter.set_change(1, TimeSignature::new(3, 4).unwrap());
        meter
    }

    #[test]
    fn bar_numbers_follow_the_signatures() {
        // 20 px beats: every bar is labelled.
        let marks = metered_ruler_marks(
            TimelineGeometry::new(20.0, 0.0),
            210.0,
            waltz_from_bar_two(),
        );
        let labels: Vec<_> = marks
            .iter()
            .filter_map(|mark| match mark {
                RulerMark::Label { x, text, .. } => Some((*x, text.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            vec![(0.0, "1"), (80.0, "2"), (140.0, "3"), (200.0, "4")]
        );
    }

    #[test]
    fn grid_lines_restart_at_each_bar() {
        let grid = metered_lane_grid(
            TimelineGeometry::new(20.0, 0.0),
            150.0,
            1.0,
            waltz_from_bar_two(),
        );
        let bars: Vec<_> = grid
            .lines
            .iter()
            .filter(|(_, line)| *line != GridLine::Fine)
            .map(|(x, _)| *x)
            .collect();
        assert_eq!(bars, vec![0.0, 80.0, 140.0]);
    }
}
