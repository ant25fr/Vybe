use super::*;

const RATE: usize = 48_000;
/// One beat at 120 BPM.
const BEAT: usize = RATE / 2;

fn playing(metronome: bool) -> (AudioEngine, rtrb::Producer<EngineCommand>) {
    let (mut engine, mut commands, _events) = AudioEngine::new();
    for command in [
        EngineCommand::SetSampleRate(RATE as u32),
        EngineCommand::SetBpm(120.0),
        EngineCommand::SetMetronome(metronome),
    ] {
        commands.push(command).unwrap();
    }
    engine.process(&mut [], 2);
    (engine, commands)
}

/// Renders `frames` in callback-sized blocks and returns the left channel.
fn render(engine: &mut AudioEngine, frames: usize) -> Vec<f32> {
    let mut left = Vec::with_capacity(frames);
    while left.len() < frames {
        let mut block = vec![0.0; 256 * 2];
        engine.process(&mut block, 2);
        left.extend(block.chunks_exact(2).map(|frame| frame[0]));
    }
    left.truncate(frames);
    left
}

#[test]
fn clicks_land_on_the_beats_with_the_first_of_the_bar_louder() {
    let (mut engine, mut commands) = playing(true);
    commands.push(EngineCommand::Play).unwrap();
    let out = render(&mut engine, BEAT * 5);
    for beat in 0..5 {
        let start = beat * BEAT;
        let expected = if beat % 4 == 0 { 0.32 } else { 0.22 };
        assert!(
            (out[start] - expected).abs() < 1e-4,
            "beat {beat}: {}",
            out[start]
        );
        // Silent between the clicks.
        assert!(out[start + RATE / 20..start + BEAT]
            .iter()
            .all(|s| *s == 0.0));
    }
}

#[test]
fn it_is_silent_when_off_or_stopped() {
    let (mut engine, mut commands) = playing(false);
    commands.push(EngineCommand::Play).unwrap();
    assert!(render(&mut engine, BEAT).iter().all(|s| *s == 0.0));

    let (mut stopped, _commands) = playing(true);
    assert!(render(&mut stopped, BEAT).iter().all(|s| *s == 0.0));
}

#[test]
fn the_click_follows_its_volume() {
    for (volume, expected) in [(0.0, 0.0), (0.5, 0.16), (2.0, 0.64)] {
        let (mut engine, mut commands) = playing(true);
        commands
            .push(EngineCommand::SetMetronomeVolume(volume))
            .unwrap();
        commands.push(EngineCommand::Play).unwrap();
        let out = render(&mut engine, 1);
        assert!((out[0] - expected).abs() < 1e-4, "{volume}: {}", out[0]);
    }
}
