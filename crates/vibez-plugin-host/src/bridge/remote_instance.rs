//! [`PluginInstance`] implementation that forwards every call to a plugin
//! hosted in a `vibez-plugin-bridge` subprocess (see [`super::child`]).
//!
//! The engine and the device wrappers cannot tell a remote instance apart
//! from an in-process one. If the plugin crashes or hangs inside the bridge,
//! the audio call fails, the block is silenced and the device reports a
//! local processing failure — the DAW itself keeps running.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vibez_core::audio_context::DeviceAudioContext;
use vibez_core::effect::ParamDescriptor;
use vibez_core::note_output::{BlockNote, NoteOutput};
use vibez_core::routing::{ExternalInputBlock, ExternalInputDescriptor};

use super::child::{AudioOutcome, BridgeChild, BridgeError};
use super::protocol::*;
use crate::instance::PluginInstance;

const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
const STATE_TIMEOUT: Duration = Duration::from_secs(30);
const MIN_AUDIO_TIMEOUT: Duration = Duration::from_millis(150);

const ERR_CRASHED: &'static str = "Plugin crashed or hung in its sandbox process — device bypassed";

pub struct RemotePluginInstance {
    child: Arc<BridgeChild>,
    name: String,
    descriptors: Vec<ParamDescriptor>,
    external_inputs: Vec<ExternalInputDescriptor>,
    is_instrument: bool,
    latency: AtomicU32,
    /// Frames per round trip (set by `prepare`); process calls larger than
    /// this are chunked.
    max_block: AtomicU32,
    /// f64 bits of the sample rate, for audio-timeout computation.
    sample_rate_bits: AtomicU64,
    error: Mutex<Option<&'static str>>,
    /// Written by `process_audio` (&mut self), read by `note_output` (&self)
    /// on the same engine thread.
    note_output: UnsafeCell<Option<NoteOutput>>,
}

// UnsafeCell makes this !Sync; the engine only ever drives one instance
// from one audio thread, which is exactly the in-process contract too.
unsafe impl Send for RemotePluginInstance {}

impl RemotePluginInstance {
    /// Build from a successful `Load` exchange. `child` is kept alive by
    /// this instance and by the UI-side handle.
    pub fn new(child: Arc<BridgeChild>, loaded: &LoadedInfo) -> Self {
        let descriptors = loaded
            .descriptors
            .iter()
            .map(|d| ParamDescriptor {
                name: Box::leak(d.name.clone().into_boxed_str()),
                min: d.min,
                max: d.max,
                default: d.default,
                unit: Box::leak(d.unit.clone().into_boxed_str()),
            })
            .collect();
        child.set_has_gui(loaded.has_gui);
        Self {
            child,
            name: loaded.name.clone(),
            descriptors,
            external_inputs: loaded.external_inputs.clone(),
            is_instrument: loaded.is_instrument,
            latency: AtomicU32::new(loaded.latency),
            // Never chunk below one full engine block (MAX_BLOCK_FRAMES = 4096),
            // so one process call is a single round trip.
            max_block: AtomicU32::new(loaded.max_buffer_size.max(4096)),
            sample_rate_bits: AtomicU64::new(loaded.sample_rate.to_bits()),
            error: Mutex::new(None),
            note_output: UnsafeCell::new(None),
        }
    }

    pub fn child(&self) -> &Arc<BridgeChild> {
        &self.child
    }

    fn sample_rate(&self) -> f64 {
        f64::from_bits(self.sample_rate_bits.load(Ordering::Relaxed))
    }

    fn audio_timeout(&self, frames: usize) -> Duration {
        let sr = self.sample_rate();
        let block_ms = if sr > 0.0 {
            frames as f64 * 4000.0 / sr
        } else {
            1000.0
        };
        MIN_AUDIO_TIMEOUT + Duration::from_millis(block_ms as u64)
    }

    fn request(
        &self,
        req: BridgeRequest,
        timeout: Duration,
    ) -> Result<BridgeResponse, BridgeError> {
        let result = self.child.request(req, timeout);
        if result.is_err() {
            *self.error.lock().unwrap() = Some(ERR_CRASHED);
        }
        result
    }

    fn note_from_wire(&self, notes_in: &[BlockNoteWire], dropped: u64) {
        let notes = notes_in
            .iter()
            .map(|n| BlockNote {
                frame: n.frame,
                pitch: n.pitch,
                velocity: n.velocity,
                on: n.on,
            })
            .collect();
        // Safety: only the engine thread touches this cell (process_audio
        // takes &mut self; note_output reads it on the same thread).
        unsafe {
            *self.note_output.get() = Some(NoteOutput::from_parts(notes, dropped));
        }
    }
}

/// Metadata returned by a successful `Load` exchange.
pub struct LoadedInfo {
    pub name: String,
    pub is_instrument: bool,
    pub has_gui: bool,
    pub latency: u32,
    pub descriptors: Vec<ParamDescriptorWire>,
    pub external_inputs: Vec<ExternalInputDescriptor>,
    pub sample_rate: f64,
    pub max_buffer_size: u32,
}

impl PluginInstance for RemotePluginInstance {
    fn external_inputs(&self) -> &[ExternalInputDescriptor] {
        &self.external_inputs
    }

    fn process_with_inputs(
        &mut self,
        buffer: &mut [f32],
        channels: usize,
        inputs: &[ExternalInputBlock<'_>],
    ) {
        if self.child.is_crashed() {
            buffer.fill(0.0);
            *self.error.lock().unwrap() = Some(ERR_CRASHED);
            return;
        }
        let channels = channels.max(1);
        let frames = buffer.len() / channels;
        let max_block = (self.max_block.load(Ordering::Relaxed) as usize).max(1);
        let timeout = self.audio_timeout(frames);

        let mut offset = 0usize;
        while offset < frames {
            let chunk_frames = (frames - offset).min(max_block);
            let chunk = &mut buffer[offset * channels..(offset + chunk_frames) * channels];
            let input_wire: Vec<(usize, Vec<f32>)> = inputs
                .iter()
                .map(|b| {
                    let start = offset * b.channels;
                    let end = (offset + chunk_frames) * b.channels;
                    let slice = b
                        .samples
                        .get(start..end.min(b.samples.len()))
                        .unwrap_or(&[]);
                    let mut v = slice.to_vec();
                    v.resize(chunk_frames * b.channels, 0.0);
                    (b.channels, v)
                })
                .collect();
            match self
                .child
                .process_audio(chunk, channels, &input_wire, timeout)
            {
                Ok(AudioOutcome {
                    notes,
                    dropped,
                    latency,
                }) => {
                    if latency > 0 {
                        self.latency.store(latency, Ordering::Relaxed);
                    }
                    self.note_from_wire(&notes, dropped);
                }
                Err(_) => {
                    chunk.fill(0.0);
                    *self.error.lock().unwrap() = Some(ERR_CRASHED);
                    return;
                }
            }
            offset += chunk_frames;
        }
    }

    fn set_audio_context(&mut self, context: DeviceAudioContext) {
        let _ = self.child.send_audio_context(context.into());
    }

    fn processing_failure_is_local(&self) -> bool {
        // A crashed sandboxed plugin only silences its own device.
        true
    }

    fn processing_recovery_requested(&self) -> bool {
        false
    }

    fn reconfiguration_requested(&self) -> bool {
        false
    }

    fn processing_configuration_valid(&self) -> bool {
        !self.child.is_crashed()
    }

    fn latency_samples(&self) -> u32 {
        self.latency.load(Ordering::Relaxed)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn param_count(&self) -> usize {
        self.descriptors.len()
    }

    fn param_descriptors_vec(&self) -> Vec<ParamDescriptor> {
        self.descriptors.clone()
    }

    fn set_param(&mut self, index: usize, value: f32) -> bool {
        matches!(
            self.request(BridgeRequest::SetParam { index, value }, CONTROL_TIMEOUT),
            Ok(BridgeResponse::Bool(true)) | Ok(BridgeResponse::Unit)
        )
    }

    fn get_param(&self, index: usize) -> f32 {
        match self.request(BridgeRequest::GetParam { index }, CONTROL_TIMEOUT) {
            Ok(BridgeResponse::Param(v)) => v,
            _ => 0.0,
        }
    }

    fn process_audio(&mut self, buffer: &mut [f32], channels: usize) {
        self.process_with_inputs(buffer, channels, &[]);
    }

    fn note_on(&mut self, pitch: u8, velocity: u8) {
        let _ = self.child.send_note(NoteEvent {
            on: true,
            pitch,
            velocity,
            frame: 0,
        });
    }

    fn note_off(&mut self, pitch: u8) {
        let _ = self.child.send_note(NoteEvent {
            on: false,
            pitch,
            velocity: 0,
            frame: 0,
        });
    }

    fn note_on_at(&mut self, pitch: u8, velocity: u8, frame_offset: u32) {
        let _ = self.child.send_note(NoteEvent {
            on: true,
            pitch,
            velocity,
            frame: frame_offset,
        });
    }

    fn note_off_at(&mut self, pitch: u8, frame_offset: u32) {
        let _ = self.child.send_note(NoteEvent {
            on: false,
            pitch,
            velocity: 0,
            frame: frame_offset,
        });
    }

    fn reset(&mut self) {
        let _ = self.request(BridgeRequest::Reset, CONTROL_TIMEOUT);
    }

    fn note_output(&self) -> Option<&NoteOutput> {
        // Safety: written only from process_audio (&mut self) and read here
        // on the same engine thread.
        unsafe { (*self.note_output.get()).as_ref() }
    }

    fn save_state(&mut self) -> Option<Vec<u8>> {
        match self.request(BridgeRequest::SaveState, STATE_TIMEOUT) {
            Ok(BridgeResponse::State(data)) => data,
            _ => None,
        }
    }

    fn load_state(&mut self, data: &[u8]) -> bool {
        matches!(
            self.request(
                BridgeRequest::LoadState {
                    data: data.to_vec()
                },
                STATE_TIMEOUT
            ),
            Ok(BridgeResponse::Bool(true))
        )
    }

    fn is_instrument(&self) -> bool {
        self.is_instrument
    }

    fn prepare(&mut self, sample_rate: f64, max_buffer_size: u32) {
        self.sample_rate_bits
            .store(sample_rate.to_bits(), Ordering::Relaxed);
        // One round trip per engine block (<= 4096 frames): never chunk
        // smaller than that.
        let max_block = max_buffer_size.max(4096);
        self.max_block.store(max_block, Ordering::Relaxed);
        let _ = self.request(
            BridgeRequest::Prepare {
                sample_rate,
                max_buffer_size: max_block,
            },
            CONTROL_TIMEOUT,
        );
    }

    fn activate(&mut self) -> bool {
        matches!(
            self.request(BridgeRequest::Activate, CONTROL_TIMEOUT),
            Ok(BridgeResponse::Bool(true)) | Ok(BridgeResponse::Unit)
        )
    }

    fn take_processing_error(&mut self) -> Option<&'static str> {
        self.error.lock().unwrap().take()
    }

    fn stop_processing(&mut self) {
        let _ = self.request(BridgeRequest::StopProcessing, CONTROL_TIMEOUT);
    }

    fn deactivate(&mut self) {
        let _ = self.request(BridgeRequest::Deactivate, CONTROL_TIMEOUT);
    }
}
