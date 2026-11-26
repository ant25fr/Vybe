//! A built-in instrument behind its track's MIDI effects: keys go through
//! them, and the arpeggiator's notes are timed inside each render.

use std::sync::Arc;

use vibez_core::audio_buffer::DecodedAudio;
use vibez_core::effect::ParamDescriptor;
use vibez_core::midi::InstrumentKind;
use vibez_core::midi_effects::{MidiEffects, MidiOut, MidiProcessor};

use crate::{Instrument, InstrumentMediaOwners, PreparedDrumPadState};

pub(crate) struct MidiFxInstrument<T: Instrument> {
    inner: T,
    midi: MidiProcessor,
}

impl<T: Instrument> MidiFxInstrument<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            midi: MidiProcessor::default(),
        }
    }
}

fn play(instrument: &mut impl Instrument) -> impl FnMut(MidiOut) + '_ {
    move |event| match event {
        MidiOut::On { pitch, velocity } => instrument.note_on(pitch, velocity),
        MidiOut::Off { pitch } => instrument.note_off(pitch),
    }
}

impl<T: Instrument> Instrument for MidiFxInstrument<T> {
    fn set_audio_context(&mut self, context: vibez_core::audio_context::DeviceAudioContext) {
        self.midi.set_clock(context.sample_rate as f32, context.bpm);
        self.midi
            .set_position(context.playing.then_some(context.musical_sample));
        self.inner.set_audio_context(context);
    }
    fn processing_failure_is_local(&self) -> bool {
        self.inner.processing_failure_is_local()
    }
    fn processing_recovery_requested(&self) -> bool {
        self.inner.processing_recovery_requested()
    }
    fn reconfiguration_requested(&self) -> bool {
        self.inner.reconfiguration_requested()
    }
    fn stop_for_reconfiguration(&mut self) {
        self.inner.stop_for_reconfiguration();
    }
    fn reconfigure_on_main_thread(&mut self) -> Result<(), String> {
        self.inner.reconfigure_on_main_thread()
    }
    fn activation_sample_rate(&self) -> Option<u32> {
        self.inner.activation_sample_rate()
    }
    fn processing_configuration_valid(&self) -> bool {
        self.inner.processing_configuration_valid()
    }
    fn latency_samples(&self) -> u32 {
        self.inner.latency_samples()
    }
    fn instrument_kind(&self) -> InstrumentKind {
        self.inner.instrument_kind()
    }
    fn param_descriptors(&self) -> &'static [ParamDescriptor] {
        self.inner.param_descriptors()
    }
    fn set_param(&mut self, index: usize, value: f32) -> bool {
        self.inner.set_param(index, value)
    }
    fn get_param(&self, index: usize) -> f32 {
        self.inner.get_param(index)
    }
    fn set_midi_effects(&mut self, effects: MidiEffects) {
        self.midi.set(effects, &mut play(&mut self.inner));
    }
    fn note_on(&mut self, pitch: u8, velocity: u8) {
        self.midi
            .note_on(pitch, velocity, &mut play(&mut self.inner));
    }
    fn note_off(&mut self, pitch: u8) {
        self.midi.note_off(pitch, &mut play(&mut self.inner));
    }
    fn note_on_at(&mut self, pitch: u8, velocity: u8, frame_offset: u32) {
        let inner = &mut self.inner;
        self.midi
            .note_on(pitch, velocity, &mut |event| match event {
                MidiOut::On { pitch, velocity } => inner.note_on_at(pitch, velocity, frame_offset),
                MidiOut::Off { pitch } => inner.note_off_at(pitch, frame_offset),
            });
    }
    fn note_off_at(&mut self, pitch: u8, frame_offset: u32) {
        let inner = &mut self.inner;
        self.midi.note_off(pitch, &mut |event| match event {
            MidiOut::On { pitch, velocity } => inner.note_on_at(pitch, velocity, frame_offset),
            MidiOut::Off { pitch } => inner.note_off_at(pitch, frame_offset),
        });
    }
    fn set_pitch_bend(&mut self, semitones: f32) {
        self.inner.set_pitch_bend(semitones);
    }
    fn render(&mut self, buffer: &mut [f32], channels: usize) {
        if !self.midi.arpeggiating() || channels == 0 {
            return self.inner.render(buffer, channels);
        }
        // Render up to each arpeggiator event, then play it.
        let frames = buffer.len() / channels;
        let mut done = 0;
        while done < frames {
            if self.midi.frames_to_event() == Some(0) {
                self.midi.fire(&mut play(&mut self.inner));
                continue;
            }
            let left = frames - done;
            let step = self
                .midi
                .frames_to_event()
                .map_or(left, |wait| (wait as usize).min(left));
            self.inner.render(
                &mut buffer[done * channels..(done + step) * channels],
                channels,
            );
            self.midi.advance(step as u32);
            done += step;
        }
    }
    fn note_output(&self) -> Option<&vibez_core::note_output::NoteOutput> {
        self.inner.note_output()
    }
    fn reset(&mut self) {
        self.midi.clear();
        self.inner.reset();
    }
    fn take_processing_error(&mut self) -> Option<&'static str> {
        self.inner.take_processing_error()
    }
    fn stop_processing(&mut self) {
        self.inner.stop_processing();
    }
    fn finish_offline_processing(&mut self) {
        self.inner.finish_offline_processing();
    }
    fn supports_batch_render(&self) -> bool {
        // The arpeggiator needs its keys as they come.
        !self.midi.arpeggiating() && self.inner.supports_batch_render()
    }
    fn load_sample(&mut self, sample: Arc<DecodedAudio>, name: String) -> InstrumentMediaOwners {
        self.inner.load_sample(sample, name)
    }
    fn load_drum_pad_sample(
        &mut self,
        pad_index: usize,
        sample: Arc<DecodedAudio>,
        name: String,
    ) -> InstrumentMediaOwners {
        self.inner.load_drum_pad_sample(pad_index, sample, name)
    }
    fn clear_drum_pad(&mut self, pad_index: usize) -> InstrumentMediaOwners {
        self.inner.clear_drum_pad(pad_index)
    }
    fn set_drum_pad_state(
        &mut self,
        pad_index: usize,
        state: PreparedDrumPadState,
    ) -> InstrumentMediaOwners {
        self.inner.set_drum_pad_state(pad_index, state)
    }
    fn set_drum_pad_param(&mut self, pad_index: usize, index: usize, value: f32) {
        self.inner.set_drum_pad_param(pad_index, index, value);
    }
    fn sample_name(&self) -> Option<&str> {
        self.inner.sample_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::SubtractiveSynth;
    use vibez_core::midi_effects::Arpeggiator;

    #[test]
    fn an_arpeggiated_key_sounds_in_steps() {
        let mut instrument = MidiFxInstrument::new(SubtractiveSynth::new(48_000.0));
        let mut arpeggiator = Arpeggiator::default();
        arpeggiator.gate = 10;
        instrument.set_midi_effects(MidiEffects {
            arpeggiator: Some(arpeggiator),
            ..Default::default()
        });
        instrument.set_param(crate::synth_params::param::RELEASE, 0.0);
        instrument.note_on(60, 100);
        let mut buffer = vec![0.0; 12_000];
        instrument.render(&mut buffer, 1);
        let loud =
            |range: std::ops::Range<usize>| buffer[range].iter().any(|sample| sample.abs() > 0.01);
        // Steps of 6000 frames, each sounding its first tenth.
        assert!(loud(0..600));
        assert!(!loud(3_000..5_900));
        assert!(loud(6_000..6_600));
    }
}
