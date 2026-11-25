//! Wire protocol between the DAW and a `vibez-plugin-bridge` subprocess.
//!
//! One framed stream in each direction over the child's stdin/stdout:
//!
//! ```text
//! [u32 LE len][u8 kind][3 pad][u64 seq][payload]   len = 12 + payload.len()
//! ```
//!
//! The header is 16 bytes, so the payload starts at a 4-byte-aligned offset
//! inside the receiving buffer (frames are stored in a `Vec<u64>`, whose
//! base is 8-aligned). That lets the audio payloads be viewed as `f32`
//! slices zero-copy (all supported targets are little-endian).
//!
//! - `KIND_CONTROL` (1): payload is a JSON [`ControlFrame`]. Used for
//!   load/params/state/GUI/lifecycle. Requests and responses are matched
//!   by `seq`; the child may also emit unsolicited [`BridgeEvent`]s.
//! - `KIND_AUDIO_REQ` (2): parent -> child, binary audio request.
//! - `KIND_AUDIO_RESP` (3): child -> parent, binary audio response.
//! - `KIND_NOTE` (4): parent -> child, binary [`NoteEvent`] (fire-and-forget,
//!   ordered with audio frames on the same stream, so sample-accurate
//!   scheduling inside the next block is preserved).
//! - `KIND_CONTEXT` (5): parent -> child, binary audio-context update
//!   (fire-and-forget).

use std::io::{self, Read};

use serde::{Deserialize, Serialize};

pub const KIND_CONTROL: u8 = 1;
pub const KIND_AUDIO_REQ: u8 = 2;
pub const KIND_AUDIO_RESP: u8 = 3;
pub const KIND_NOTE: u8 = 4;
pub const KIND_CONTEXT: u8 = 5;

const MAX_FRAME: u32 = 64 * 1024 * 1024;
/// Bytes after the 4-byte length field: kind + 3 pad + seq.
const HEADER_REST: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlFrame {
    Request {
        seq: u64,
        req: BridgeRequest,
    },
    Response {
        seq: u64,
        result: Result<BridgeResponse, String>,
    },
    Event(BridgeEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeRequest {
    /// Load + initialize one plugin. Runs entirely inside the bridge
    /// process, so a crashing factory kills only the bridge.
    Load {
        format: String,
        path: String,
        uid: String,
        is_instrument_hint: bool,
        sample_rate: f64,
        max_buffer_size: u32,
        state: Option<Vec<u8>>,
    },
    Prepare {
        sample_rate: f64,
        max_buffer_size: u32,
    },
    Activate,
    Deactivate,
    StopProcessing,
    Reset,
    SetParam {
        index: usize,
        value: f32,
    },
    GetParam {
        index: usize,
    },
    SaveState,
    LoadState {
        data: Vec<u8>,
    },
    NoteOutput,
    OpenGui {
        width: u32,
        height: u32,
    },
    CloseGui,
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeResponse {
    Loaded {
        name: String,
        is_instrument: bool,
        has_gui: bool,
        latency: u32,
        descriptors: Vec<ParamDescriptorWire>,
        external_inputs: Vec<vibez_core::routing::ExternalInputDescriptor>,
    },
    Unit,
    Param(f32),
    Bool(bool),
    State(Option<Vec<u8>>),
    NoteOutput {
        notes: Vec<BlockNoteWire>,
        dropped: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeEvent {
    /// The user closed the bridge's plugin window.
    GuiClosed,
}

/// Serializable copy of `vibez_core::effect::ParamDescriptor` (which borrows
/// `&'static str`, so it is not serde-able directly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDescriptorWire {
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub unit: String,
}

impl From<&vibez_core::effect::ParamDescriptor> for ParamDescriptorWire {
    fn from(d: &vibez_core::effect::ParamDescriptor) -> Self {
        Self {
            name: d.name.to_string(),
            min: d.min,
            max: d.max,
            default: d.default,
            unit: d.unit.to_string(),
        }
    }
}

/// Serializable copy of `vibez_core::note_output::BlockNote`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BlockNoteWire {
    pub frame: u32,
    pub pitch: u8,
    pub velocity: u8,
    pub on: bool,
}

impl From<vibez_core::note_output::BlockNote> for BlockNoteWire {
    fn from(n: vibez_core::note_output::BlockNote) -> Self {
        Self {
            frame: n.frame,
            pitch: n.pitch,
            velocity: n.velocity,
            on: n.on,
        }
    }
}

/// One audio block, parent -> child. `buffer` is interleaved
/// `frames * channels` samples, exactly what the engine hands to
/// `PluginInstance::process_audio`. `inputs` are the external/sidechain
/// input blocks as `(channels, interleaved samples)`.
#[derive(Debug, Clone)]
pub struct AudioRequest {
    pub channels: usize,
    pub frames: usize,
    pub buffer: Vec<f32>,
    pub inputs: Vec<(usize, Vec<f32>)>,
}

/// Same, but the buffer is mutable: the bridge processes the payload in
/// place, so no copy is made on the processing path.
#[derive(Debug)]
pub struct AudioRequestMut<'a> {
    pub channels: usize,
    pub frames: usize,
    pub buffer: &'a mut [f32],
    pub inputs: Vec<(usize, &'a [f32])>,
}

/// One processed audio block, child -> parent. `buffer` is a zero-copy
/// view into the received frame payload.
#[derive(Debug)]
pub struct AudioResponseRef<'a> {
    pub ok: bool,
    pub buffer: &'a [f32],
    pub notes: Vec<BlockNoteWire>,
    pub dropped: u64,
    pub latency: u32,
}

/// Fire-and-forget note event, ordered with audio frames on the wire.
#[derive(Debug, Clone, Copy)]
pub struct NoteEvent {
    pub on: bool,
    pub pitch: u8,
    pub velocity: u8,
    pub frame: u32,
}

/// Fire-and-forget transport context update (mirrors
/// `DeviceAudioContext`).
#[derive(Debug, Clone, Copy)]
pub struct AudioContextWire {
    pub musical_sample: u64,
    pub continuous_sample: u64,
    pub sample_rate: u32,
    pub bpm: f64,
    pub playing: bool,
}

impl From<vibez_core::audio_context::DeviceAudioContext> for AudioContextWire {
    fn from(c: vibez_core::audio_context::DeviceAudioContext) -> Self {
        Self {
            musical_sample: c.musical_sample,
            continuous_sample: c.continuous_sample,
            sample_rate: c.sample_rate,
            bpm: c.bpm,
            playing: c.playing,
        }
    }
}

impl From<AudioContextWire> for vibez_core::audio_context::DeviceAudioContext {
    fn from(c: AudioContextWire) -> Self {
        vibez_core::audio_context::DeviceAudioContext {
            musical_sample: c.musical_sample,
            continuous_sample: c.continuous_sample,
            sample_rate: c.sample_rate,
            bpm: c.bpm,
            playing: c.playing,
        }
    }
}

// ── Frames ──
//
// Received frames are stored in a `Vec<u64>` so the base pointer is
// 8-aligned; with the 16-byte header the payload (and the f32 buffers
// inside audio payloads) is 4-aligned and can be viewed as `&[f32]`
// zero-copy on little-endian targets.

/// A received frame. The valid bytes are `len` of the backing buffer.
pub struct Frame {
    buf: Vec<u64>,
    len: usize,
}

impl Frame {
    fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.buf.as_ptr().cast::<u8>(), self.buf.len() * 8) }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.buf.as_mut_ptr().cast::<u8>(), self.buf.len() * 8)
        }
    }
    pub fn kind(&self) -> u8 {
        self.bytes()[0]
    }
    pub fn seq(&self) -> u64 {
        u64::from_le_bytes(self.bytes()[4..12].try_into().unwrap())
    }
    /// Payload bytes (after the 16-byte frame header). 4-byte aligned.
    pub fn payload(&self) -> &[u8] {
        &self.bytes()[HEADER_REST..self.len]
    }
    /// Mutable payload, for in-place audio processing in the bridge.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        let len = self.len;
        &mut self.bytes_mut()[HEADER_REST..len]
    }
}

pub fn encode_frame(kind: u8, seq: u64, payload: &[u8]) -> Vec<u8> {
    let len = (HEADER_REST + payload.len()) as u32;
    let mut out = Vec::with_capacity(4 + len as usize);
    out.extend_from_slice(&len.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&[0u8; 3]);
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn encode_control(frame: &ControlFrame) -> Vec<u8> {
    let seq = match frame {
        ControlFrame::Request { seq, .. } | ControlFrame::Response { seq, .. } => *seq,
        ControlFrame::Event(_) => 0,
    };
    let payload = serde_json::to_vec(frame).expect("control frame serializes");
    encode_frame(KIND_CONTROL, seq, &payload)
}

/// Read one frame. Returns `Ok(None)` on clean EOF before any byte.
pub fn read_frame(r: &mut impl Read) -> io::Result<Option<Frame>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len < HEADER_REST || len as u64 > MAX_FRAME as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("bad frame len {len}"),
        ));
    }
    let words = len.div_ceil(8);
    let mut buf = vec![0u64; words];
    let bytes = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), words * 8) };
    r.read_exact(&mut bytes[..len])?;
    Ok(Some(Frame { buf, len }))
}

// ── Binary audio payloads ──
//
// Layouts keep every f32 run at a 4-aligned offset inside the payload:
//   request:  [u32 channels][u32 frames][buffer f32...][u32 n_inputs][per input: u32 ch, u32 len, f32...]
//   response: [u8 ok][3 pad][u32 len][buffer f32...][u32 n_notes][per note: u32 frame, u8 pitch, u8 vel, u8 on][u64 dropped][u32 latency]

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Raw byte view of an f32 slice (little-endian targets; the slice must be
/// 4-aligned, which the frame layout guarantees for audio payloads).
fn f32_as_bytes(v: &[f32]) -> &[u8] {
    debug_assert_eq!(v.as_ptr() as usize % 4, 0);
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), v.len() * 4) }
}

fn bytes_as_f32(b: &[u8]) -> &[f32] {
    debug_assert_eq!(b.len() % 4, 0);
    debug_assert_eq!(b.as_ptr() as usize % 4, 0);
    unsafe { std::slice::from_raw_parts(b.as_ptr().cast::<f32>(), b.len() / 4) }
}

fn bytes_as_f32_mut(b: &mut [u8]) -> &mut [f32] {
    debug_assert_eq!(b.len() % 4, 0);
    debug_assert_eq!(b.as_ptr() as usize % 4, 0);
    unsafe { std::slice::from_raw_parts_mut(b.as_mut_ptr().cast::<f32>(), b.len() / 4) }
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn u32(&mut self) -> Option<u32> {
        let b = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes(b.try_into().ok()?))
    }
    fn u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn u64(&mut self) -> Option<u64> {
        let b = self.data.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        Some(u64::from_le_bytes(b.try_into().ok()?))
    }
    fn f32s(&mut self, n: usize) -> Option<Vec<f32>> {
        let bytes = self.data.get(self.pos..self.pos + n * 4)?;
        self.pos += n * 4;
        Some(bytes_as_f32(bytes).to_vec())
    }
}

pub fn encode_audio_request(req: &AudioRequest) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + req.buffer.len() * 4);
    put_u32(&mut out, req.channels as u32);
    put_u32(&mut out, req.frames as u32);
    out.extend_from_slice(f32_as_bytes(&req.buffer));
    put_u32(&mut out, req.inputs.len() as u32);
    for (ch, samples) in &req.inputs {
        put_u32(&mut out, *ch as u32);
        put_u32(&mut out, samples.len() as u32);
        out.extend_from_slice(f32_as_bytes(samples));
    }
    out
}

pub fn decode_audio_request(data: &[u8]) -> Option<AudioRequest> {
    let mut c = Cursor { data, pos: 0 };
    let channels = c.u32()? as usize;
    let frames = c.u32()? as usize;
    let buffer = c.f32s(channels.checked_mul(frames)?)?;
    let n_inputs = c.u32()? as usize;
    let mut inputs = Vec::with_capacity(n_inputs.min(64));
    for _ in 0..n_inputs {
        let ch = c.u32()? as usize;
        let len = c.u32()? as usize;
        let samples = c.f32s(len)?;
        inputs.push((ch, samples));
    }
    Some(AudioRequest {
        channels,
        frames,
        buffer,
        inputs,
    })
}

/// Decode an audio request for in-place processing: the returned buffer
/// aliases `data` (which must be 4-aligned — guaranteed by `Frame`).
pub fn decode_audio_request_mut(data: &mut [u8]) -> Option<AudioRequestMut<'_>> {
    if data.len() < 16 || data.as_ptr() as usize % 4 != 0 {
        return None;
    }
    let channels = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
    let frames = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    let buf_f32 = channels.checked_mul(frames)?;
    let buf_bytes = buf_f32.checked_mul(4)?;
    if data.len() < 8 + buf_bytes + 4 {
        return None;
    }
    let (_head, rest) = data.split_at_mut(8);
    let (buf_region, tail) = rest.split_at_mut(buf_bytes);
    let buffer = bytes_as_f32_mut(buf_region);
    // Parse the input list from the remaining bytes (immutable views).
    let mut c = Cursor { data: tail, pos: 0 };
    let n_inputs = c.u32()? as usize;
    let mut inputs = Vec::with_capacity(n_inputs.min(64));
    for _ in 0..n_inputs {
        let ch = c.u32()? as usize;
        let len = c.u32()? as usize;
        let bytes = tail.get(c.pos..c.pos + len.checked_mul(4)?)?;
        c.pos += len * 4;
        inputs.push((ch, bytes_as_f32(bytes)));
    }
    Some(AudioRequestMut {
        channels,
        frames,
        buffer,
        inputs,
    })
}

pub fn encode_audio_response(
    ok: bool,
    buffer: &[f32],
    notes: &[BlockNoteWire],
    dropped: u64,
    latency: u32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + buffer.len() * 4);
    out.push(ok as u8);
    out.extend_from_slice(&[0u8; 3]);
    put_u32(&mut out, buffer.len() as u32);
    out.extend_from_slice(f32_as_bytes(buffer));
    put_u32(&mut out, notes.len() as u32);
    for n in notes {
        put_u32(&mut out, n.frame);
        out.push(n.pitch);
        out.push(n.velocity);
        out.push(n.on as u8);
    }
    out.extend_from_slice(&dropped.to_le_bytes());
    put_u32(&mut out, latency);
    out
}

/// Decode an audio response as a zero-copy view over the frame payload
/// (which must be 4-aligned — guaranteed by `Frame`).
pub fn decode_audio_response(data: &[u8]) -> Option<AudioResponseRef<'_>> {
    if data.len() < 8 || data.as_ptr() as usize % 4 != 0 {
        return None;
    }
    let ok = data[0] != 0;
    let len = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    let buf_bytes = len.checked_mul(4)?;
    let buffer = bytes_as_f32(data.get(8..8 + buf_bytes)?);
    let mut c = Cursor {
        data,
        pos: 8 + buf_bytes,
    };
    let n_notes = c.u32()? as usize;
    let mut notes = Vec::with_capacity(n_notes.min(4096));
    for _ in 0..n_notes {
        let frame = c.u32()?;
        let pitch = c.u8()?;
        let velocity = c.u8()?;
        let on = c.u8()? != 0;
        notes.push(BlockNoteWire {
            frame,
            pitch,
            velocity,
            on,
        });
    }
    let dropped = c.u64()?;
    let latency = c.u32()?;
    Some(AudioResponseRef {
        ok,
        buffer,
        notes,
        dropped,
        latency,
    })
}

pub fn encode_note_event(ev: &NoteEvent) -> Vec<u8> {
    let mut out = Vec::with_capacity(7);
    out.push(ev.on as u8);
    out.push(ev.pitch);
    out.push(ev.velocity);
    out.extend_from_slice(&ev.frame.to_le_bytes());
    out
}

pub fn decode_note_event(data: &[u8]) -> Option<NoteEvent> {
    let mut c = Cursor { data, pos: 0 };
    let on = c.u8()? != 0;
    let pitch = c.u8()?;
    let velocity = c.u8()?;
    let frame = c.u32()?;
    Some(NoteEvent {
        on,
        pitch,
        velocity,
        frame,
    })
}

pub fn encode_audio_context(ctx: &AudioContextWire) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    out.extend_from_slice(&ctx.musical_sample.to_le_bytes());
    out.extend_from_slice(&ctx.continuous_sample.to_le_bytes());
    put_u32(&mut out, ctx.sample_rate);
    out.extend_from_slice(&ctx.bpm.to_le_bytes());
    out.push(ctx.playing as u8);
    out
}

pub fn decode_audio_context(data: &[u8]) -> Option<AudioContextWire> {
    let mut c = Cursor { data, pos: 0 };
    let musical_sample = c.u64()?;
    let continuous_sample = c.u64()?;
    let sample_rate = c.u32()?;
    let bpm_bytes = c.data.get(c.pos..c.pos + 8)?;
    c.pos += 8;
    let bpm = f64::from_le_bytes(bpm_bytes.try_into().ok()?);
    let playing = c.u8()? != 0;
    Some(AudioContextWire {
        musical_sample,
        continuous_sample,
        sample_rate,
        bpm,
        playing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trip() {
        let frame = encode_frame(KIND_CONTROL, 42, b"hello");
        let mut slice = &frame[..];
        let f = read_frame(&mut slice).unwrap().unwrap();
        assert_eq!(f.kind(), KIND_CONTROL);
        assert_eq!(f.seq(), 42);
        assert_eq!(f.payload(), b"hello");
        // Payload must be 4-aligned for zero-copy f32 views.
        assert_eq!(f.payload().as_ptr() as usize % 4, 0);
    }

    #[test]
    fn clean_eof_is_none() {
        let mut empty: &[u8] = &[];
        assert!(read_frame(&mut empty).unwrap().is_none());
    }

    #[test]
    fn control_round_trip() {
        let frame = ControlFrame::Request {
            seq: 7,
            req: BridgeRequest::SetParam {
                index: 3,
                value: 0.5,
            },
        };
        let bytes = encode_control(&frame);
        let mut slice = &bytes[..];
        let f = read_frame(&mut slice).unwrap().unwrap();
        assert_eq!(f.kind(), KIND_CONTROL);
        assert_eq!(f.seq(), 7);
        let decoded: ControlFrame = serde_json::from_slice(f.payload()).unwrap();
        match decoded {
            ControlFrame::Request { seq, req } => {
                assert_eq!(seq, 7);
                assert!(matches!(
                    req,
                    BridgeRequest::SetParam {
                        index: 3,
                        value: 0.5
                    }
                ));
            }
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn audio_request_round_trip() {
        let req = AudioRequest {
            channels: 2,
            frames: 4,
            buffer: vec![0.0, 0.5, -0.5, 1.0, 0.25, -1.0, 0.75, 0.125],
            inputs: vec![(2, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])],
        };
        let payload = encode_audio_request(&req);
        let decoded = decode_audio_request(&payload).unwrap();
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.frames, 4);
        assert_eq!(decoded.buffer, req.buffer);
        assert_eq!(decoded.inputs, req.inputs);
    }

    #[test]
    fn audio_request_in_place_round_trip() {
        let req = AudioRequest {
            channels: 2,
            frames: 2,
            buffer: vec![0.5, -0.5, 1.0, -1.0],
            inputs: vec![(1, vec![0.25, 0.75])],
        };
        // Round-trip through a frame so the payload is 4-aligned, like the
        // bridge's reader does.
        let bytes = encode_frame(KIND_AUDIO_REQ, 1, &encode_audio_request(&req));
        let mut slice = &bytes[..];
        let mut f = read_frame(&mut slice).unwrap().unwrap();
        let parsed = decode_audio_request_mut(f.payload_mut()).unwrap();
        assert_eq!(parsed.channels, 2);
        assert_eq!(parsed.frames, 2);
        assert_eq!(parsed.buffer, &[0.5, -0.5, 1.0, -1.0]);
        for s in parsed.buffer.iter_mut() {
            *s *= 2.0;
        }
        assert_eq!(parsed.buffer, &[1.0, -1.0, 2.0, -2.0]);
        assert_eq!(parsed.inputs, vec![(1, &[0.25, 0.75][..])]);
    }

    #[test]
    fn audio_response_round_trip() {
        let notes = vec![BlockNoteWire {
            frame: 3,
            pitch: 60,
            velocity: 100,
            on: true,
        }];
        let bytes = encode_frame(
            KIND_AUDIO_RESP,
            9,
            &encode_audio_response(true, &[0.1, 0.2], &notes, 2, 512),
        );
        let mut slice = &bytes[..];
        let f = read_frame(&mut slice).unwrap().unwrap();
        assert_eq!(f.kind(), KIND_AUDIO_RESP);
        let decoded = decode_audio_response(f.payload()).unwrap();
        assert!(decoded.ok);
        assert_eq!(decoded.buffer, &[0.1, 0.2]);
        assert_eq!(decoded.notes.len(), 1);
        assert_eq!(decoded.notes[0].pitch, 60);
        assert_eq!(decoded.dropped, 2);
        assert_eq!(decoded.latency, 512);
    }

    #[test]
    fn note_and_context_round_trip() {
        let ev = NoteEvent {
            on: true,
            pitch: 64,
            velocity: 90,
            frame: 128,
        };
        let decoded = decode_note_event(&encode_note_event(&ev)).unwrap();
        assert_eq!(decoded.frame, 128);
        assert!(decoded.on);

        let ctx = AudioContextWire {
            musical_sample: 1,
            continuous_sample: 2,
            sample_rate: 48000,
            bpm: 120.0,
            playing: true,
        };
        let decoded = decode_audio_context(&encode_audio_context(&ctx)).unwrap();
        assert_eq!(decoded.sample_rate, 48000);
        assert_eq!(decoded.bpm, 120.0);
        assert!(decoded.playing);
    }
}
