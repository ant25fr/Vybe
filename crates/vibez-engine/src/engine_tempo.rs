//! Rendering in short slices: tempo automation moves the transport faster or
//! slower than the output, and modulators need a finer clock than a callback.

use super::*;

/// Output frames of one slice.
const SLICE_FRAMES: usize = 64;

/// The slice being rendered.
#[derive(Debug, Clone, Copy)]
pub(super) struct TempoSlice {
    /// Timeline positions this slice covers.
    pub grid_frames: usize,
    pub bpm: f64,
}

/// Where the next slice starts and how long it is, in both clocks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SlicePlan {
    pub frames: usize,
    pub grid_frames: u64,
    pub bpm: f64,
    /// Share of a timeline position carried to the next slice.
    pub carry: f64,
}

impl SlicePlan {
    /// Plan up to `remaining` output frames from timeline `position`.
    pub(crate) fn at(
        lanes: &[vibez_core::automation::AutomationLane],
        position: u64,
        remaining: usize,
        project_bpm: f64,
        sample_rate: u32,
        carry: f64,
    ) -> Self {
        // The last slice takes the remainder, so none is shorter than half.
        let frames = if remaining >= SLICE_FRAMES * 3 / 2 {
            SLICE_FRAMES
        } else {
            remaining
        };
        let samples_per_beat = f64::from(sample_rate) * 60.0 / project_bpm;
        let bpm = vibez_core::tempo::bpm_at(lanes, position as f64 / samples_per_beat, project_bpm);
        let exact = frames as f64 * (bpm / project_bpm) + carry;
        let grid_frames = (exact.floor() as u64).max(1);
        Self {
            frames,
            grid_frames,
            bpm,
            carry: exact - grid_frames as f64,
        }
    }

    /// Cut the slice short of `boundary` when it falls inside it.
    pub(super) fn cut_before(&mut self, position: u64, boundary: Option<u64>, project_bpm: f64) {
        if let Some(boundary) = boundary.filter(|&boundary| {
            boundary > position && boundary < position.saturating_add(self.grid_frames)
        }) {
            self.end_on(position, boundary, project_bpm);
        }
    }

    /// Cut the slice to end exactly on timeline position `end`.
    fn end_on(&mut self, position: u64, end: u64, project_bpm: f64) {
        let grid_frames = end - position;
        let ratio = self.bpm / project_bpm;
        self.frames = ((grid_frames as f64 / ratio).round() as usize).clamp(1, self.frames);
        self.grid_frames = grid_frames;
        self.carry = 0.0;
    }
}

impl AudioEngine {
    pub(super) fn arrange_tempo_is_automated(&self) -> bool {
        vibez_core::tempo::tempo_lane(&self.master.playback_source.automation).is_some()
    }

    pub(super) fn has_modulators(&self) -> bool {
        self.tracks
            .iter()
            .chain(self.buses.iter())
            .chain(std::iter::once(&self.master))
            .any(|track| track.modulation.is_active())
    }

    /// The tempo devices hear now: the slice's, the lane's at the Arrange
    /// playhead, or the project's.
    pub(super) fn effective_bpm(&self) -> f64 {
        if let Some(slice) = self.tempo_slice {
            return slice.bpm;
        }
        let project = self.transport.bpm();
        if self.clock_domain != ClockDomain::Arrange {
            return project;
        }
        let samples_per_beat = f64::from(self.sample_rate) * 60.0 / project;
        vibez_core::tempo::bpm_at(
            &self.master.playback_source.automation,
            self.transport.position() as f64 / samples_per_beat,
            project,
        )
    }

    /// Render the playing Arrangement slice by slice. The transport advances
    /// afterwards by the positions recorded here.
    pub(super) fn process_arrange_slices(
        &mut self,
        output: &mut [f32],
        frames: usize,
        channels: usize,
        live_input: Option<LiveInputBlock<'_>>,
        mut capture: Option<&mut TrackOutputCapture<'_>>,
    ) {
        let project_bpm = self.transport.bpm();
        let mut position = self.transport.position();
        let mut advanced = 0u64;
        let mut done = 0;
        while done < frames {
            let mut plan = SlicePlan::at(
                &self.master.playback_source.automation,
                position,
                frames - done,
                project_bpm,
                self.sample_rate,
                self.tempo_carry,
            );
            self.apply_track_mutes_due(position);
            // A queued mute lands on its own sample, so the slice stops there.
            plan.cut_before(position, self.next_track_mute_boundary(), project_bpm);
            let loop_region = self.transport.active_loop_region();
            let wrap = loop_region.filter(|&(_, end)| {
                position < end && position.saturating_add(plan.grid_frames) >= end
            });
            if let Some((_, end)) = wrap {
                plan.end_on(position, end, project_bpm);
            }
            self.tempo_carry = plan.carry;
            let (start, end) = (done * channels, (done + plan.frames) * channels);
            let mut part = capture.as_deref_mut().map(|capture| TrackOutputCapture {
                source_track_raw: capture.source_track_raw,
                samples: capture.samples.get_mut(start..end).unwrap_or(&mut []),
            });
            self.hold_meters = done + plan.frames < frames;
            self.tempo_slice = Some(TempoSlice {
                grid_frames: plan.grid_frames as usize,
                bpm: plan.bpm,
            });
            self.render_routing_graph(
                &mut output[start..end],
                render_paths::MultitrackRenderBlock {
                    pos: position,
                    repeat_pos: position,
                    frames: plan.frames,
                    channels,
                    loop_region,
                    live_input: live_input.map(|input| input.slice(done, plan.frames, channels)),
                },
                part.as_mut(),
                false,
                true,
            );
            self.tempo_slice = None;
            done += plan.frames;
            position = position.saturating_add(plan.grid_frames);
            advanced = advanced.saturating_add(plan.grid_frames);
            if let Some((loop_start, loop_end)) = wrap {
                self.apply_track_mutes_due(loop_end);
                for track in &mut self.tracks {
                    track.flush_notes();
                }
                let anchor = self.playing_note_repeat_anchor();
                self.reanchor_note_repeats(anchor, loop_start);
                position = loop_start;
                self.split_wrap_handled = true;
            }
        }
        self.hold_meters = false;
        self.tempo_advance = Some(advanced);
    }

    /// Stopped transport with modulators: the same slices, going nowhere.
    pub(super) fn render_idle_slices(
        &mut self,
        output: &mut [f32],
        frames: usize,
        channels: usize,
        live_input: Option<LiveInputBlock<'_>>,
        mut capture: Option<&mut TrackOutputCapture<'_>>,
    ) {
        let mut done = 0;
        while done < frames {
            let count = (frames - done).min(SLICE_FRAMES);
            let (start, end) = (done * channels, (done + count) * channels);
            let mut part = capture.as_deref_mut().map(|capture| TrackOutputCapture {
                source_track_raw: capture.source_track_raw,
                samples: capture.samples.get_mut(start..end).unwrap_or(&mut []),
            });
            self.hold_meters = done + count < frames;
            self.render_idle_instruments(
                &mut output[start..end],
                count,
                channels,
                self.performance_position.saturating_add(done as u64),
                live_input.map(|input| input.slice(done, count, channels)),
                part.as_mut(),
            );
            done += count;
        }
        self.hold_meters = false;
    }
}

#[cfg(test)]
#[path = "engine_tempo_tests.rs"]
mod tests;
