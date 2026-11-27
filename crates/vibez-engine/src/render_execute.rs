use super::*;
use crate::engine::{tempo::SlicePlan, AudioEngine, OfflineRoutingSetup};

pub(super) fn render_offline_inner(
    req: &BounceRequest,
    plugins: Option<&mut OfflinePlugins>,
    progress: impl FnMut(u8),
) -> Result<BounceResult, String> {
    render_with_stems(req, plugins, &[], progress).map(|(result, _)| result)
}

/// The mix of `req`, and the post-fader signal of each channel of `stems`
/// over the same range, taken from the one pass.
pub(super) fn render_with_stems(
    req: &BounceRequest,
    mut plugins: Option<&mut OfflinePlugins>,
    stems: &[TrackId],
    mut progress: impl FnMut(u8),
) -> Result<(BounceResult, Vec<(TrackId, DecodedAudio)>), String> {
    if req.sample_rate == 0
        || !req.bpm.is_finite()
        || req.bpm <= 0.0
        || req.range_samples.1 < req.range_samples.0
    {
        return Err("Invalid offline sample rate, tempo or render range".into());
    }
    let (start, end) = req.range_samples;
    let frames =
        usize::try_from(end - start).map_err(|_| "Offline range exceeds addressable storage")?;
    progress(0);
    let dependencies = dependencies::actual_dependencies(req, plugins.as_deref());
    if let Some(plugins) = plugins.as_deref() {
        dependencies::validate_plugins(req, plugins, &dependencies)?;
    }
    let mut reports = Vec::new();
    let mut controls = Vec::new();
    for channel in &dependencies.graph_channels {
        let info = dependencies::all_channels(req).find(|info| info.id == channel.id);
        let instrument_latency = plugins
            .as_deref()
            .and_then(|plugins| plugins.instruments.get(&channel.id))
            .map_or(0, |instrument| instrument.latency_samples());
        reports.push((
            vibez_core::routing::RoutingNode {
                channel: channel.id,
                stage: vibez_core::routing::NodeStage::Source,
            },
            instrument_latency,
        ));
        if let Some(info) = info {
            controls.extend(info.automation.iter().map(|lane| (channel.id, lane.target)));
            for effect in &channel.effects {
                let latency = plugins
                    .as_deref()
                    .and_then(|plugins| plugins.effects.get(&effect.id))
                    .map(|effect| effect.latency_samples())
                    .unwrap_or_else(|| {
                        info.effects
                            .iter()
                            .find(|info| info.id == effect.id)
                            .map_or(0, |info| {
                                vibez_dsp::factory::create_effect_from_info(
                                    info,
                                    req.sample_rate as f32,
                                )
                                .latency_samples()
                            })
                    });
                reports.push((
                    vibez_core::routing::RoutingNode {
                        channel: channel.id,
                        stage: vibez_core::routing::NodeStage::Effect(effect.id),
                    },
                    latency,
                ));
            }
        }
    }
    let mut routing = crate::routing::PreparedRouting::prepare_compensated(
        &dependencies.graph_channels,
        BLOCK_FRAMES,
        &reports,
        &[],
        1,
    )?;
    routing.configure_automation(&controls)?;
    let timing = crate::bounce_timing::BounceTiming::prepare(
        (start, end),
        routing.compensation.output_latency,
    )
    .map_err(str::to_owned)?;
    let mut warnings = Vec::new();
    let mut tracks = Vec::new();
    let mut buses = Vec::new();
    for info in req
        .tracks
        .iter()
        .filter(|info| dependencies.taps.contains_key(&info.id))
    {
        tracks.push(Box::new(prepare::channel(
            req,
            info,
            dependencies.taps[&info.id],
            plugins.as_deref_mut(),
            &mut warnings,
        )));
    }
    for info in req
        .buses
        .iter()
        .filter(|info| dependencies.taps.contains_key(&info.id))
    {
        buses.push(Box::new(prepare::channel(
            req,
            info,
            dependencies.taps[&info.id],
            plugins.as_deref_mut(),
            &mut warnings,
        )));
    }
    let master = req
        .master
        .as_ref()
        .filter(|info| dependencies.taps.contains_key(&info.id))
        .map(|info| {
            prepare::channel(
                req,
                info,
                dependencies.taps[&info.id],
                plugins.as_deref_mut(),
                &mut warnings,
            )
        })
        .unwrap_or_else(|| EngineTrack::new(TrackId::MASTER));
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    let mut scratch = vec![0.0; BLOCK_FRAMES * CHANNELS];
    let mut selected = vec![0.0; BLOCK_FRAMES * CHANNELS];
    let selected_track = dependencies::selected_track(req.mode);
    let latency = routing.compensation.output_latency as usize;
    let tempo_lanes = req
        .master
        .as_ref()
        .map_or(&[][..], |master| master.automation.as_slice());
    let modulated = dependencies::all_channels(req).any(|channel| !channel.modulators.is_empty());
    let mut engine = AudioEngine::for_offline_routing(OfflineRoutingSetup {
        sample_rate: req.sample_rate,
        bpm: req.bpm,
        swing: req.swing,
        tracks,
        buses,
        master,
        routing,
    });
    if !stems.is_empty() {
        let rendered = usize::try_from(timing.render_end - timing.render_start)
            .map_err(|_| "Offline range exceeds addressable storage")?;
        engine.collect_offline_stems(stems, rendered);
    }
    let mut position = timing.render_start;
    let mut failure = None;
    // An automated tempo moves the timeline at its own pace and modulators
    // need short slices, so the range is then found in the output as it grows.
    let tempo_automated = vibez_core::tempo::tempo_lane(tempo_lanes).is_some();
    let sliced = tempo_automated || modulated;
    let (mut carry, mut written_frames) = (0.0, 0usize);
    let (mut first_output, mut end_output) = (None, None);
    loop {
        let done = if tempo_automated {
            end_output.is_some_and(|end: usize| written_frames >= end + latency)
        } else {
            position >= timing.render_end
        };
        if done {
            break;
        }
        let (block, slice, positions) = if tempo_automated {
            let plan = SlicePlan::at(tempo_lanes, position, 64, req.bpm, req.sample_rate, carry);
            carry = plan.carry;
            let output_of = |target: u64| {
                (position + plan.grid_frames >= target).then(|| {
                    let share = target.saturating_sub(position) as f64 / plan.grid_frames as f64;
                    written_frames + (share * plan.frames as f64).round() as usize
                })
            };
            first_output = first_output.or_else(|| output_of(start));
            end_output = end_output.or_else(|| output_of(end));
            (
                plan.frames,
                Some((plan.grid_frames as usize, plan.bpm)),
                plan.grid_frames,
            )
        } else {
            let limit = if sliced { 64 } else { BLOCK_FRAMES as u64 };
            let block = (timing.render_end - position).min(limit) as usize;
            (block, None, block as u64)
        };
        let output = &mut scratch[..block * CHANNELS];
        let selected_output = &mut selected[..block * CHANNELS];
        engine.render_offline_routing_segment(
            position,
            output,
            selected_track.map(|id| (id, selected_output)),
            slice,
        );
        if let Some((track_id, effect_id, reason)) = engine.take_device_processing_error() {
            let channel = dependencies::all_channels(req).find(|channel| channel.id == track_id);
            let channel_name = channel.map_or("Master", |channel| channel.name.as_str());
            let device_name = channel
                .and_then(|channel| match effect_id {
                    Some(id) => channel
                        .effects
                        .iter()
                        .find(|effect| effect.id == id)
                        .and_then(|effect| effect.plugin.as_ref()),
                    None => channel.plugin_instrument.as_ref(),
                })
                .map_or("Device", |device| device.name.as_str());
            failure = Some(format!(
                "{device_name} on channel '{channel_name}' failed: {reason}"
            ));
            break;
        }
        if let Some(reason) = engine.offline_compensation_failure() {
            failure = Some(reason);
            break;
        }
        let selection = if tempo_automated {
            Some(0..block)
        } else {
            timing
                .block_selection(position, block)
                .map(|selection| selection.start..selection.end)
        };
        if let Some(selection) = selection {
            let written = if selected_track.is_some() {
                &selected[..block * CHANNELS]
            } else {
                output
            };
            for frame in written[selection.start * CHANNELS..selection.end * CHANNELS]
                .as_chunks::<CHANNELS>()
                .0
            {
                left.push(frame[0]);
                right.push(frame[1]);
            }
        }
        written_frames += block;
        position += positions;
        progress(if timing.render_end == 0 {
            100
        } else {
            ((position as u128 * 100) / timing.render_end as u128).min(100) as u8
        });
    }
    let (frames, first_stem_frame) = if tempo_automated {
        let first = first_output.unwrap_or(0);
        let frames = end_output.unwrap_or(first).saturating_sub(first);
        for channel in [&mut left, &mut right] {
            channel.drain(..(first + latency).min(channel.len()));
            channel.resize(frames, 0.0);
        }
        (frames, first)
    } else {
        (frames, (start - timing.render_start) as usize)
    };
    let stem_audio = engine
        .take_offline_stems()
        .into_iter()
        .map(|(id, latency, samples)| {
            // Each channel runs its own latency behind, not the mix's.
            let first = first_stem_frame + latency as usize;
            let mut channels = vec![vec![0.0; frames]; CHANNELS];
            for (frame, values) in samples
                .chunks_exact(CHANNELS)
                .skip(first)
                .take(frames)
                .enumerate()
            {
                for (channel, value) in channels.iter_mut().zip(values) {
                    channel[frame] = *value;
                }
            }
            let audio = DecodedAudio {
                channels,
                sample_rate: req.sample_rate,
            };
            (id, audio)
        })
        .collect();
    if let Some(plugins) = plugins {
        let (mut tracks, mut buses, mut master) = engine.take_offline_channels();
        prepare::return_plugins(req, plugins, &mut tracks, &mut buses, &mut master);
    }
    if let Some(reason) = failure {
        return Err(reason);
    }
    progress(100);
    let result = BounceResult {
        audio: DecodedAudio {
            channels: vec![left, right],
            sample_rate: req.sample_rate,
        },
        warnings,
    };
    Ok((result, stem_audio))
}
