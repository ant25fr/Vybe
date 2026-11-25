//! Parent-side handle to one `vibez-plugin-bridge` subprocess.
//!
//! The child hosts a single plugin instance. All communication goes over
//! framed stdin/stdout (see [`protocol`]); the calling thread writes frames
//! directly into the child's stdin (no writer-thread hop, to keep the
//! audio round-trip as short as possible) and a reader thread demultiplexes
//! the child's stdout frames into a control channel and an audio channel.
//!
//! On Windows the pipes are created manually with 1 MiB buffers so a full
//! block never blocks on a small default pipe; on Linux the pipe size is
//! raised best-effort via `F_SETPIPE_SZ`.
//!
//! Crash/hang isolation: every call has a timeout. A timeout, a dead child
//! (EOF on stdout or non-zero exit) marks the handle crashed; further calls
//! fail fast with [`BridgeError::Dead`] and the child process is killed, so
//! a misbehaving plugin can never take the DAW down with it.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use super::protocol::*;

#[derive(Debug, Clone)]
pub enum BridgeError {
    /// The helper binary could not be spawned at all.
    Spawn(String),
    /// The bridge binary was not found next to the app executable.
    HelperMissing,
    /// Frame could not be sent (stdin gone / child wedged).
    Send(String),
    /// No answer within the timeout — the plugin is hung or crashed.
    Timeout,
    /// The child process exited or closed its stdout.
    Dead,
    /// The child answered with an error string.
    Remote(String),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::Spawn(e) => write!(f, "failed to spawn plugin bridge: {e}"),
            BridgeError::HelperMissing => write!(f, "vibez-plugin-bridge helper not found"),
            BridgeError::Send(e) => write!(f, "bridge write failed: {e}"),
            BridgeError::Timeout => write!(f, "plugin bridge timed out (hung or crashed)"),
            BridgeError::Dead => write!(f, "plugin bridge process died (plugin crashed)"),
            BridgeError::Remote(e) => write!(f, "plugin bridge error: {e}"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// What a successful audio round-trip reports besides the processed
/// samples (copied into the caller's buffer by [`BridgeChild::process_audio`]).
#[derive(Debug, Clone)]
pub struct AudioOutcome {
    pub notes: Vec<BlockNoteWire>,
    pub dropped: u64,
    pub latency: u32,
}

struct Inner {
    child: Mutex<Child>,
    stdin: Mutex<Box<dyn Write + Send>>,
    control_rx: Mutex<mpsc::Receiver<ControlFrame>>,
    /// Raw audio-response payloads, decoded by the caller.
    audio_rx: Mutex<mpsc::Receiver<(u64, Vec<u8>)>>,
    /// Serializes control requests so seq matching is trivial.
    control_lock: Mutex<()>,
    seq: AtomicU64,
    audio_seq: AtomicU64,
    crashed: AtomicBool,
    has_gui: AtomicBool,
    events: Mutex<VecDeque<BridgeEvent>>,
}

/// A running bridge subprocess hosting one plugin instance.
pub struct BridgeChild {
    inner: Arc<Inner>,
}

impl BridgeChild {
    /// Locate the `vibez-plugin-bridge` helper next to the current
    /// executable (same layout as the `vibez-plugin-scan` helper).
    pub fn helper_binary() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        let name = format!("vibez-plugin-bridge{}", std::env::consts::EXE_SUFFIX);
        [dir.to_path_buf(), dir.parent()?.to_path_buf()]
            .into_iter()
            .map(|d| d.join(&name))
            .find(|p| p.is_file())
    }

    /// Spawn the helper found next to the current executable.
    pub fn spawn() -> Result<Arc<Self>, BridgeError> {
        match Self::helper_binary() {
            Some(path) => Self::spawn_with(&path),
            None => Err(BridgeError::HelperMissing),
        }
    }

    /// Spawn a specific helper binary. Public so tests can point at a
    /// stand-in process.
    pub fn spawn_with(path: &Path) -> Result<Arc<Self>, BridgeError> {
        #[cfg(windows)]
        {
            let (child, parent_write, parent_read) = spawn_with_big_pipes(path)?;
            Self::from_parts(child, parent_write, parent_read)
        }
        #[cfg(not(windows))]
        {
            let mut command = Command::new(path);
            super::sandbox::apply_env(&mut command);
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| BridgeError::Spawn(format!("{}: {e}", path.display())))?;
            #[cfg(target_os = "linux")]
            {
                // Best-effort: give both pipes a 1 MiB buffer so a full
                // audio block never blocks on the default 64 KiB pipe.
                if let Some(s) = &child.stdin {
                    enlarge_pipe(s);
                }
                if let Some(s) = &child.stdout {
                    enlarge_pipe(s);
                }
            }
            let stdin = child.stdin.take().expect("piped stdin");
            let stdout = child.stdout.take().expect("piped stdout");
            Self::from_parts(child, stdin, stdout)
        }
    }

    /// Spawn an arbitrary command with arguments (test hook).
    #[cfg(test)]
    pub fn spawn_command(program: &Path, args: &[&str]) -> Result<Arc<Self>, BridgeError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| BridgeError::Spawn(format!("{}: {e}", program.display())))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        Self::from_parts(child, stdin, stdout)
    }

    fn from_parts<W, R>(child: Child, stdin: W, stdout: R) -> Result<Arc<Self>, BridgeError>
    where
        W: Write + Send + 'static,
        R: Read + Send + 'static,
    {
        let (control_tx, control_rx) = mpsc::channel::<ControlFrame>();
        let (audio_tx, audio_rx) = mpsc::channel::<(u64, Vec<u8>)>();

        let inner = Arc::new(Inner {
            child: Mutex::new(child),
            stdin: Mutex::new(Box::new(stdin)),
            control_rx: Mutex::new(control_rx),
            audio_rx: Mutex::new(audio_rx),
            control_lock: Mutex::new(()),
            seq: AtomicU64::new(1),
            audio_seq: AtomicU64::new(1),
            crashed: AtomicBool::new(false),
            has_gui: AtomicBool::new(false),
            events: Mutex::new(VecDeque::new()),
        });

        // Reader thread: demultiplexes stdout frames. When it ends (EOF or
        // error) its senders are dropped, so blocked receivers see
        // Disconnected and report Dead.
        {
            let weak = Arc::downgrade(&inner);
            std::thread::Builder::new()
                .name("vibez-bridge-reader".into())
                .spawn(move || {
                    let mut stdout = stdout;
                    loop {
                        match read_frame(&mut stdout) {
                            Ok(Some(frame)) => match frame.kind() {
                                KIND_CONTROL => {
                                    match serde_json::from_slice::<ControlFrame>(frame.payload()) {
                                        Ok(frame) => {
                                            if control_tx.send(frame).is_err() {
                                                break;
                                            }
                                        }
                                        Err(_) => break,
                                    }
                                }
                                KIND_AUDIO_RESP => {
                                    if audio_tx
                                        .send((frame.seq(), frame.payload().to_vec()))
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                                _ => { /* unknown kind: ignore */ }
                            },
                            Ok(None) => break,
                            Err(_) => break,
                        }
                    }
                    if let Some(inner) = weak.upgrade() {
                        inner.crashed.store(true, Ordering::SeqCst);
                    }
                })
                .map_err(|e| BridgeError::Spawn(format!("reader thread: {e}")))?;
        }

        Ok(Arc::new(BridgeChild { inner }))
    }

    /// True once the child has died, timed out, or wedged.
    pub fn is_crashed(&self) -> bool {
        if self.inner.crashed.load(Ordering::SeqCst) {
            return true;
        }
        let mut child = self.inner.child.lock().unwrap();
        matches!(child.try_wait(), Ok(Some(_)))
    }

    /// Whether the loaded plugin reported a GUI (set after `Load`).
    pub fn has_gui(&self) -> bool {
        self.inner.has_gui.load(Ordering::SeqCst)
    }

    pub(crate) fn set_has_gui(&self, v: bool) {
        self.inner.has_gui.store(v, Ordering::SeqCst);
    }

    /// Events emitted by the child since the last call.
    pub fn poll_events(&self) -> Vec<BridgeEvent> {
        self.inner.events.lock().unwrap().drain(..).collect()
    }

    fn mark_crashed(&self) {
        self.inner.crashed.store(true, Ordering::SeqCst);
        let mut child = self.inner.child.lock().unwrap();
        let _ = child.kill();
    }

    /// Write one frame straight into the child's stdin. Called from the
    /// audio thread for audio frames — no intermediate thread hop.
    fn send_frame(&self, frame: Vec<u8>) -> Result<(), BridgeError> {
        if self.is_crashed() {
            return Err(BridgeError::Dead);
        }
        let mut stdin = self.inner.stdin.lock().unwrap_or_else(|e| e.into_inner());
        match stdin.write_all(&frame).and_then(|_| stdin.flush()) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.mark_crashed();
                Err(BridgeError::Send(e.to_string()))
            }
        }
    }

    /// Send a control request and wait for its response.
    pub fn request(
        &self,
        req: BridgeRequest,
        timeout: Duration,
    ) -> Result<BridgeResponse, BridgeError> {
        if self.is_crashed() {
            return Err(BridgeError::Dead);
        }
        let _guard = self.inner.control_lock.lock().unwrap();
        let seq = self.inner.seq.fetch_add(1, Ordering::SeqCst);
        self.send_frame(encode_control(&ControlFrame::Request { seq, req }))?;
        let deadline = Instant::now() + timeout;
        let rx = self.inner.control_rx.lock().unwrap();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.mark_crashed();
                return Err(BridgeError::Timeout);
            }
            match rx.recv_timeout(remaining) {
                Ok(ControlFrame::Response { seq: s, result }) => {
                    if s == seq {
                        return result.map_err(BridgeError::Remote);
                    }
                    // Stale response (shouldn't happen: requests are
                    // serialized); keep waiting.
                }
                Ok(ControlFrame::Event(ev)) => {
                    self.inner.events.lock().unwrap().push_back(ev);
                }
                Ok(ControlFrame::Request { .. }) => { /* protocol violation: ignore */ }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.mark_crashed();
                    return Err(BridgeError::Timeout);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.mark_crashed();
                    return Err(BridgeError::Dead);
                }
            }
        }
    }

    /// Process one audio block in the child. `inputs` are the external input
    /// blocks as `(channels, samples)`. On success the processed samples are
    /// copied into `buffer` and the note output / latency are returned.
    pub fn process_audio(
        &self,
        buffer: &mut [f32],
        channels: usize,
        inputs: &[(usize, Vec<f32>)],
        timeout: Duration,
    ) -> Result<AudioOutcome, BridgeError> {
        if self.is_crashed() {
            return Err(BridgeError::Dead);
        }
        let frames = buffer.len() / channels.max(1);
        let req = AudioRequest {
            channels,
            frames,
            buffer: buffer.to_vec(),
            inputs: inputs.to_vec(),
        };
        let seq = self.inner.audio_seq.fetch_add(1, Ordering::SeqCst);
        self.send_frame(encode_frame(
            KIND_AUDIO_REQ,
            seq,
            &encode_audio_request(&req),
        ))?;
        let deadline = Instant::now() + timeout;
        let rx = self.inner.audio_rx.lock().unwrap();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.mark_crashed();
                return Err(BridgeError::Timeout);
            }
            match rx.recv_timeout(remaining) {
                Ok((s, payload)) if s == seq => {
                    let Some(resp) = decode_audio_response(&payload) else {
                        self.mark_crashed();
                        return Err(BridgeError::Dead);
                    };
                    if !resp.ok {
                        return Err(BridgeError::Remote("process failed".into()));
                    }
                    if resp.buffer.len() == buffer.len() {
                        buffer.copy_from_slice(resp.buffer);
                    }
                    return Ok(AudioOutcome {
                        notes: resp.notes,
                        dropped: resp.dropped,
                        latency: resp.latency,
                    });
                }
                Ok(_) => { /* stale audio response: keep waiting */ }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.mark_crashed();
                    return Err(BridgeError::Timeout);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.mark_crashed();
                    return Err(BridgeError::Dead);
                }
            }
        }
    }

    /// Queue a note event (ordered with audio frames). Never blocks: if the
    /// child is wedged the write fails fast and the handle is marked crashed.
    pub fn send_note(&self, ev: NoteEvent) -> Result<(), BridgeError> {
        self.send_frame(encode_frame(KIND_NOTE, 0, &encode_note_event(&ev)))
    }

    /// Queue a transport-context update (fire-and-forget).
    pub fn send_audio_context(&self, ctx: AudioContextWire) -> Result<(), BridgeError> {
        self.send_frame(encode_frame(KIND_CONTEXT, 0, &encode_audio_context(&ctx)))
    }
}

impl Drop for BridgeChild {
    fn drop(&mut self) {
        // Best effort: ask the child to quit, then kill it. If the child is
        // the last reference holder this must not block.
        if !self.inner.crashed.load(Ordering::SeqCst) {
            let _ = self.inner.stdin.lock().map(|mut s| {
                let _ = s.write_all(&encode_control(&ControlFrame::Request {
                    seq: self.inner.seq.fetch_add(1, Ordering::SeqCst),
                    req: BridgeRequest::Quit,
                }));
                let _ = s.flush();
            });
        }
        let mut child = self.inner.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}

// ── Big pipes ──

/// 1 MiB per pipe direction: a full 4096-frame 8-channel block (128 KiB)
/// fits many times over, so writes never block on pipe capacity.
#[cfg(windows)]
const PIPE_SIZE: u32 = 1 << 20;

#[cfg(windows)]
fn spawn_with_big_pipes(path: &Path) -> Result<(Child, std::fs::File, std::fs::File), BridgeError> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT};
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::Pipes::CreatePipe;

    unsafe fn make_pipe(size: u32) -> std::io::Result<(HANDLE, HANDLE)> {
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        let mut read: HANDLE = std::ptr::null_mut();
        let mut write: HANDLE = std::ptr::null_mut();
        if CreatePipe(&mut read, &mut write, &sa, size) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok((read, write))
    }

    let spawn_err = |e: std::io::Error| BridgeError::Spawn(format!("{}: {e}", path.display()));
    // Parent -> child (child stdin) and child -> parent (child stdout).
    let (in_read, in_write) = unsafe { make_pipe(PIPE_SIZE) }.map_err(spawn_err)?;
    let (out_read, out_write) = unsafe { make_pipe(PIPE_SIZE) }.map_err(spawn_err)?;
    unsafe {
        // The parent's ends must not be inherited by the child.
        SetHandleInformation(in_write, HANDLE_FLAG_INHERIT, 0);
        SetHandleInformation(out_read, HANDLE_FLAG_INHERIT, 0);
        // The child's ends keep the inherit flag set by CreatePipe.
    }
    let child_stdin = Stdio::from(unsafe { OwnedHandle::from_raw_handle(in_read) });
    let child_stdout = Stdio::from(unsafe { OwnedHandle::from_raw_handle(out_write) });
    let child = Command::new(path)
        .stdin(child_stdin)
        .stdout(child_stdout)
        .stderr(Stdio::null())
        .spawn()
        .map_err(spawn_err)?;
    let parent_write = std::fs::File::from(unsafe { OwnedHandle::from_raw_handle(in_write) });
    let parent_read = std::fs::File::from(unsafe { OwnedHandle::from_raw_handle(out_read) });
    Ok((child, parent_write, parent_read))
}

#[cfg(target_os = "linux")]
fn enlarge_pipe<T: std::os::unix::io::AsRawFd>(f: &T) {
    // Best effort; may be capped by /proc/sys/fs/pipe-max-size.
    unsafe {
        libc::fcntl(f.as_raw_fd(), libc::F_SETPIPE_SZ, PIPE_SIZE_LINUX);
    }
}

#[cfg(target_os = "linux")]
const PIPE_SIZE_LINUX: libc::c_int = 1 << 20;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_missing_binary_fails() {
        let res = BridgeChild::spawn_with(Path::new("definitely-not-a-real-binary-xyz"));
        assert!(matches!(res, Err(BridgeError::Spawn(_))));
    }

    #[cfg(windows)]
    #[test]
    fn dead_child_reports_dead() {
        // cmd /c exit 3: spawns, writes nothing, exits immediately.
        let child = BridgeChild::spawn_command(Path::new("cmd"), &["/c", "exit 3"]).unwrap();
        let result = child.request(BridgeRequest::Reset, Duration::from_millis(500));
        assert!(matches!(
            result,
            Err(BridgeError::Dead) | Err(BridgeError::Timeout)
        ));
        // After the reader saw EOF the handle must report crashed.
        std::thread::sleep(Duration::from_millis(100));
        assert!(child.is_crashed());
    }

    #[cfg(windows)]
    #[test]
    fn hung_child_times_out_and_is_killed() {
        // ping for 30s never writes to stdout: the request must time out.
        let child =
            BridgeChild::spawn_command(Path::new("cmd"), &["/c", "ping -n 30 127.0.0.1 >nul"])
                .unwrap();
        let result = child.request(BridgeRequest::Reset, Duration::from_millis(300));
        assert!(matches!(result, Err(BridgeError::Timeout)));
        assert!(child.is_crashed());
    }
}
