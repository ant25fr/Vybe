//! Tests of sliced rendering under an automated tempo.

use super::*;
use crate::mixer::EngineClip;
use std::sync::Arc;
use vibez_core::audio_buffer::DecodedAudio;
use vibez_core::automation::{AutomationLane, AutomationPoint, AutomationTarget};
use vibez_core::id::ClipId;
use vibez_core::tempo::normalized_from_bpm;

fn tempo(points: &[(f64, f64)]) -> AutomationLane {
    let mut lane = AutomationLane::new(AutomationTarget::Tempo);
    for &(beat, bpm) in points {
        lane.insert_point(AutomationPoint {
            beat,
            value: normalized_from_bpm(bpm),
            curve: 0.0,
        });
    }
    lane
}

#[test]
fn a_slice_covers_more_of_the_timeline_when_the_tempo_is_faster() {
    let lanes = [tempo(&[(0.0, 180.0)])];
    let mut carry = 0.0;
    let mut covered = 0;
    for _ in 0..100 {
        let plan = SlicePlan::at(&lanes, 0, 4_096, 120.0, 48_000, carry);
        carry = plan.carry;
        covered += plan.grid_frames;
    }
    // 100 slices of 64 frames at one and a half times the project tempo.
    assert!((covered as i64 - 9_600).abs() <= 1, "{covered}");
}

#[test]
fn the_last_slice_takes_the_remainder() {
    let plan = SlicePlan::at(&[], 0, 80, 120.0, 48_000, 0.0);
    assert_eq!((plan.frames, plan.grid_frames), (80, 80));
    let plan = SlicePlan::at(&[], 0, 200, 120.0, 48_000, 0.0);
    assert_eq!(plan.frames, 64);
}

fn engine_playing_ramp_audio() -> AudioEngine {
    let (mut engine, _commands, _events) = AudioEngine::new();
    engine.sample_rate = 48_000;
    let audio = Arc::new(DecodedAudio {
        channels: vec![(0..48_000).map(|frame| frame as f32 / 48_000.0).collect()],
        sample_rate: 48_000,
    });
    let mut track = EngineTrack::new(TrackId::new());
    track.playback_source.clips.push(EngineClip {
        audition: false,
        id: ClipId::new(),
        audio,
        position: 0,
        source_offset: 0,
        start_marker: 0,
        duration: 48_000,
        loop_enabled: false,
        loop_start: 0,
        loop_end: 0,
        linear_gain: 1.0,
        fades: Default::default(),
        playback_direction: Default::default(),
        warp_markers: Default::default(),
    });
    engine.tracks.push(Box::new(track));
    engine
}

#[test]
fn audio_follows_an_automated_tempo_by_re_pitching() {
    let mut engine = engine_playing_ramp_audio();
    engine.tracks[0].render_repitched(1_000, 128, 64, 2, None);
    let buffer = &engine.tracks[0].mix_buffer;
    // Output frame 10 plays timeline position 1020.
    assert!((buffer[20] - 1_020.0 / 48_000.0).abs() < 1e-5);
    engine.tracks[0].render_repitched(1_000, 32, 64, 2, None);
    let buffer = &engine.tracks[0].mix_buffer;
    assert!((buffer[40] - 1_010.0 / 48_000.0).abs() < 1e-5);
}

#[test]
fn only_a_drawn_tempo_lane_splits_arrange() {
    let mut engine = engine_playing_ramp_audio();
    assert!(!engine.arrange_tempo_is_automated());
    engine
        .master
        .playback_source
        .automation
        .push(AutomationLane::new(AutomationTarget::Tempo));
    assert!(!engine.arrange_tempo_is_automated());
    engine.master.playback_source.automation[0] = tempo(&[(0.0, 60.0)]);
    assert!(engine.arrange_tempo_is_automated());
    assert!((engine.effective_bpm() - 60.0).abs() < 1e-3);
}

fn render_callbacks(engine: &mut AudioEngine, callbacks: usize, frames: usize) -> Vec<f32> {
    let mut heard = Vec::new();
    for _ in 0..callbacks {
        let mut output = vec![0.0; frames * 2];
        engine.process_block(AudioProcessBlock::new(&mut output, 2));
        heard.extend(output.iter().step_by(2));
    }
    heard
}

#[test]
fn the_transport_and_a_bounce_move_at_the_automated_tempo() {
    let (mut engine, mut commands, _events) = AudioEngine::new();
    engine.sample_rate = 48_000;
    commands
        .push(EngineCommand::SetAutomationLane {
            track_id: TrackId::MASTER,
            insertion: crate::clip_insertion::PreparedAutomationLaneInsertion::new(tempo(&[(
                0.0, 240.0,
            )])),
        })
        .ok()
        .unwrap();
    commands.push(EngineCommand::Play).ok().unwrap();
    render_callbacks(&mut engine, 10, 512);
    // Twice the project tempo of 120: twice the positions.
    let expected = 10 * 512 * 2;
    assert!(
        (engine.transport().position() as i64 - expected).abs() <= 2,
        "{}",
        engine.transport().position()
    );
}

#[test]
fn a_bounce_through_a_doubled_tempo_is_half_as_long_and_an_octave_up() {
    use crate::render::{render_offline, BounceMode, BounceRequest};
    use vibez_core::track::{ClipInfo, TrackInfo};
    let track = TrackInfo::new("ramp");
    let mut master = TrackInfo::new("Master");
    master.id = TrackId::MASTER;
    master.automation.push(tempo(&[(0.0, 240.0)]));
    let clip_id = ClipId::new();
    let clip = ClipInfo {
        id: clip_id,
        track_id: track.id,
        name: "ramp".into(),
        position: 0,
        source_offset: 0,
        start_marker: None,
        duration: 4_096,
        source: None,
        file_path: None,
        loop_enabled: false,
        loop_start: 0,
        loop_end: 0,
        gain_db: Default::default(),
        fades: Default::default(),
        playback_direction: Default::default(),
        transient_markers: Default::default(),
        warp_markers: Default::default(),
        transpose: Default::default(),
        original_bpm: None,
        warped: false,
        warped_to_bpm: None,
        take_lane: None,
        color_index: None,
    };
    let audio = Arc::new(DecodedAudio {
        channels: vec![(0..4_096).map(|frame| frame as f32 / 8_192.0).collect()],
        sample_rate: 44_100,
    });
    let result = render_offline(&BounceRequest {
        tracks: vec![track],
        master: Some(master),
        buses: Vec::new(),
        audio_clips: vec![clip],
        note_clips: Vec::new(),
        clip_audio: [(clip_id, audio)].into_iter().collect(),
        sampler_audio: Default::default(),
        drum_pad_audio: Default::default(),
        mode: BounceMode::Master,
        range_samples: (0, 4_096),
        bpm: 120.0,
        sample_rate: 44_100,
        swing: vibez_core::perform::SwingAmount::STRAIGHT,
    });
    let left = &result.audio.channels[0];
    assert!((left.len() as i64 - 2_048).abs() <= 1, "{}", left.len());
    // Frame 800 plays timeline position 1600, frame 400 position 800.
    assert!((left[800] / left[400] - 2.0).abs() < 0.02);
}

/// Records the continuous clock each slice hands the instrument.
struct ClockProbe(Arc<std::sync::Mutex<Vec<u64>>>);

impl vibez_instruments::Instrument for ClockProbe {
    fn set_audio_context(&mut self, context: vibez_core::audio_context::DeviceAudioContext) {
        self.0.lock().unwrap().push(context.continuous_sample);
    }
    fn instrument_kind(&self) -> vibez_core::midi::InstrumentKind {
        vibez_core::midi::InstrumentKind::SubtractiveSynth
    }
    fn param_descriptors(&self) -> &'static [vibez_core::effect::ParamDescriptor] {
        &[]
    }
    fn set_param(&mut self, _: usize, _: f32) -> bool {
        false
    }
    fn get_param(&self, _: usize) -> f32 {
        0.0
    }
    fn note_on(&mut self, _: u8, _: u8) {}
    fn note_off(&mut self, _: u8) {}
    fn render(&mut self, _: &mut [f32], _: usize) {}
    fn reset(&mut self) {}
}

#[test]
fn slices_count_each_output_frame_once() {
    let (mut engine, mut commands, _events) = AudioEngine::new();
    let track = TrackId::new();
    let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
    let model = [
        super::super::graph_test_support::routing_channel(track),
        super::super::graph_test_support::routing_channel(TrackId::MASTER),
    ];
    for command in [
        EngineCommand::add_track(track),
        EngineCommand::SetInstrument {
            track_id: track,
            instrument: Box::new(ClockProbe(Arc::clone(&heard))),
        },
        EngineCommand::SetRouting(crate::routing::PreparedRouting::prepare(&model, 512).unwrap()),
        EngineCommand::SetModulator {
            track_id: track,
            slot: 0,
            settings: Some(Default::default()),
        },
        EngineCommand::Play,
    ] {
        commands.push(command).ok().unwrap();
    }
    render_callbacks(&mut engine, 2, 256);
    assert_eq!(
        *heard.lock().unwrap(),
        [0, 64, 128, 192, 256, 320, 384, 448]
    );
}

#[test]
fn a_slice_ends_on_a_queued_mute() {
    let mut plan = SlicePlan::at(&[], 100, 512, 120.0, 48_000, 0.0);
    plan.cut_before(100, Some(130), 120.0);
    assert_eq!((plan.frames, plan.grid_frames), (30, 30));
    let mut plan = SlicePlan::at(&[], 100, 512, 120.0, 48_000, 0.0);
    plan.cut_before(100, Some(100), 120.0);
    assert_eq!(plan.frames, 64);
}

#[test]
fn a_removed_effect_leaves_no_modulation_behind() {
    use vibez_core::modulation::ModulationLink;
    let mut track = EngineTrack::new(TrackId::new());
    let effect_id = vibez_core::id::EffectId::new();
    track.effects.push(EffectSlot {
        id: effect_id,
        effect: vibez_dsp::factory::create_effect(vibez_core::effect::EffectType::Filter, 48_000.0),
        bypass: false,
        levels: Default::default(),
    });
    track.modulation.set_modulator(0, Some(Default::default()));
    track.modulation.set_link(
        0,
        0,
        Some(ModulationLink {
            target: AutomationTarget::EffectParam {
                effect_id,
                param_index: 0,
            },
            amount: 0.2,
        }),
    );
    let clock = crate::mixer::ModulationClock {
        beat: 0.0,
        seconds: 0.0,
        frames: 64,
        sample_rate: 48_000,
    };
    track.advance_modulation(clock);
    track.apply_modulation(vibez_core::routing::NodeStage::Effect(effect_id));
    track.effects.clear();
    track.modulation.set_modulator(0, None);
    for _ in 0..3 {
        track.advance_modulation(clock);
    }
    assert!(!track.modulation.is_active());
}
