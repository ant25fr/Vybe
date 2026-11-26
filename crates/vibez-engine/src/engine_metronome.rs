//! The metronome: a click on every counted beat of the running transport,
//! accented on the first of each bar. It is mixed after the project graph, so
//! it is heard but never bounced, captured or metered with the project.

use super::*;
use vibez_core::meter::MeterMap;

/// Length of one click.
const CLICK_SECONDS: f32 = 0.03;

impl AudioEngine {
    /// Adds the clicks of this block to `output`, which starts at the
    /// transport position the block was rendered from and covers `advance`
    /// timeline positions.
    pub(super) fn mix_metronome(
        &self,
        output: &mut [f32],
        frames: usize,
        channels: usize,
        advance: u64,
    ) {
        if !self.metronome
            || !self.transport.is_playing()
            || channels == 0
            || frames == 0
            || self.sample_rate == 0
            || self.count_in_is_clicking()
        {
            return;
        }
        let bpm = self.transport.bpm();
        if bpm <= 0.0 {
            return;
        }
        let beat_samples = f64::from(self.sample_rate) * 60.0 / bpm;
        // Positions the transport covers per output frame: 1 unless the tempo
        // is automated.
        let rate = advance as f64 / frames as f64;
        let click_positions = f64::from((self.sample_rate as f32 * CLICK_SECONDS).max(1.0)) * rate;
        let performing = self.clock_domain == ClockDomain::Perform;
        // Perform counts in 4/4.
        let meter = if performing {
            MeterMap::default()
        } else {
            self.meter
        };
        let block_start = if performing {
            self.performance_position
        } else {
            self.transport.position()
        };
        // The project is heard this much after the transport renders it.
        let latency = u64::from(self.mix_latency());
        let (loop_start, loop_end) = (self.transport.loop_start(), self.transport.loop_end());
        let looping = !performing
            && self.transport.loop_enabled()
            && loop_end > loop_start
            && block_start < loop_end;
        for frame in 0..frames {
            let elapsed = if advance == frames as u64 {
                frame as u64
            } else {
                (frame as f64 * rate) as u64
            };
            let mut position = block_start.saturating_add(elapsed);
            if looping && position >= loop_end {
                position = loop_start + (position - loop_end) % (loop_end - loop_start);
            }
            let Some(heard) = position.checked_sub(latency) else {
                continue;
            };
            let sample_of = |beat: f64| (beat * beat_samples).round() as u64;
            // The counted beat at or before this position, on the whole-sample
            // grid the beats round to: a position just before a beat that
            // rounds onto it belongs to it.
            let beat = heard as f64 / beat_samples;
            let ahead = meter.position_at((heard as f64 + 0.5) / beat_samples);
            let unit = ahead.signature.beat_length();
            let mut count = (((heard as f64 + 0.5) / beat_samples - ahead.start_beat) / unit)
                .floor()
                .max(0.0);
            let mut bar = ahead;
            if sample_of(bar.start_beat + count * unit) > heard {
                bar = meter.position_at((beat - 1e-5).max(0.0));
                let unit = bar.signature.beat_length();
                count = ((beat - bar.start_beat) / unit).floor().max(0.0);
            }
            let start = sample_of(bar.start_beat + count * bar.signature.beat_length());
            let Some(offset) = heard.checked_sub(start) else {
                continue;
            };
            let click_length = click_positions.min(unit * beat_samples);
            if offset as f64 >= click_length {
                continue;
            }
            let accent = count == 0.0;
            let frequency = if accent { 1_760.0 } else { 1_320.0 };
            let amplitude = if accent { 0.32 } else { 0.22 };
            let time = (offset as f64 / rate.max(f64::EPSILON)) as f32 / self.sample_rate as f32;
            let envelope = 1.0 - (offset as f64 / click_length) as f32;
            let click = (std::f32::consts::TAU * frequency * time).cos()
                * envelope.powi(4)
                * amplitude
                * self.metronome_volume;
            for sample in &mut output[frame * channels..(frame + 1) * channels] {
                *sample += click;
            }
        }
    }

    /// A recording count-in plays its own clicks; the metronome stays out
    /// of its way until the take starts.
    fn count_in_is_clicking(&self) -> bool {
        let before = |boundary: u64| self.performance_position < boundary;
        self.pending_section_record.as_ref().is_some_and(|pending| {
            pending.prepared.is_some() && before(pending.effective_at_samples)
        }) || self
            .clip_count_in_timing()
            .is_some_and(|timing| before(timing.boundary))
    }
}
