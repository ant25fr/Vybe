use super::*;

const RATE: f32 = 48_000.0;

/// The frequency of the settled output, from its rising zero crossings.
fn frequency(instrument: &mut dyn Instrument) -> f32 {
    let mut buffer = vec![0.0; RATE as usize / 2];
    instrument.render(&mut buffer, 1);
    let tail = &buffer[buffer.len() / 2..];
    let rising: Vec<usize> = (1..tail.len())
        .filter(|&i| tail[i - 1] <= 0.0 && tail[i] > 0.0)
        .collect();
    let (first, last) = (rising[0], rising[rising.len() - 1]);
    (rising.len() - 1) as f32 * RATE / (last - first) as f32
}

fn bent_ratio(mut build: impl FnMut() -> Box<dyn Instrument>) -> f32 {
    let mut plain = build();
    plain.note_on(57, 100);
    let mut bent = build();
    bent.set_pitch_bend(PITCH_BEND_RANGE_SEMITONES);
    bent.note_on(57, 100);
    frequency(bent.as_mut()) / frequency(plain.as_mut())
}

#[test]
fn a_full_wheel_raises_the_synth_by_two_semitones() {
    let ratio = bent_ratio(|| {
        let mut synth = synth::SubtractiveSynth::new(RATE);
        // A bare sine, so the crossings count the note.
        synth.set_param(synth_params::param::WAVEFORM, 0.0);
        synth.set_param(synth_params::param::LEVEL_2, 0.0);
        synth.set_param(synth_params::param::SUB, 0.0);
        Box::new(synth)
    });
    let expected = 2.0_f32.powf(2.0 / 12.0);
    assert!((ratio - expected).abs() < 0.01, "{ratio} != {expected}");
}
