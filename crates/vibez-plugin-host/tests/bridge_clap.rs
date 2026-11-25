//! End-to-end test of a REAL CLAP plugin inside the sandbox bridge:
//! load (Odin2), activate, note + audio processing, params, state save, quit.
//!
//! Skipped unless a CLAP bundle is available: set `VIBEZ_TEST_CLAP` to a
//! `.clap` path, or have Odin2 installed in the standard CLAP folder.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use vibez_plugin_host::bridge::protocol::*;

fn clap_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("VIBEZ_TEST_CLAP") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let candidates = [
        "/c/Program Files/Common Files/CLAP/Odin2.clap",
        "/usr/lib/clap/Odin2.clap",
        "/usr/local/lib/clap/Odin2.clap",
        "/Library/Audio/Plug-Ins/CLAP/Odin2.clap",
        "$HOME/.clap/Odin2.clap",
    ];
    for c in candidates {
        let p = PathBuf::from(c.replace("$HOME", &home()));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn home() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default()
}

/// Ask the sandboxed scanner for the plugin id of `path`.
fn scan_uid(path: &std::path::Path) -> Option<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_vibez-plugin-scan"))
        .arg("clap")
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let infos: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).ok()?;
    infos
        .first()?
        .get("id")?
        .get("uid")?
        .as_str()
        .map(|s| s.to_string())
}

fn spawn_bridge() -> (
    std::process::Child,
    std::process::ChildStdin,
    mpsc::Receiver<(u8, u64, Vec<u8>)>,
) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vibez-plugin-bridge"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn vibez-plugin-bridge");
    let stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || loop {
        match read_frame(&mut stdout) {
            Ok(Some(f)) => {
                if tx.send((f.kind(), f.seq(), f.payload().to_vec())).is_err() {
                    break;
                }
            }
            _ => break,
        }
    });
    (child, stdin, rx)
}

fn send(stdin: &mut std::process::ChildStdin, frame: Vec<u8>) {
    stdin.write_all(&frame).unwrap();
    stdin.flush().unwrap();
}

fn recv(rx: &mpsc::Receiver<(u8, u64, Vec<u8>)>) -> (u8, u64, Vec<u8>) {
    rx.recv_timeout(Duration::from_secs(30))
        .expect("bridge answers")
}

fn control(rx: &mpsc::Receiver<(u8, u64, Vec<u8>)>) -> ControlFrame {
    let (kind, _seq, payload) = recv(rx);
    assert_eq!(kind, KIND_CONTROL);
    serde_json::from_slice(&payload).unwrap()
}

fn request(
    stdin: &mut std::process::ChildStdin,
    rx: &mpsc::Receiver<(u8, u64, Vec<u8>)>,
    seq: u64,
    req: BridgeRequest,
) -> Result<BridgeResponse, String> {
    send(stdin, encode_control(&ControlFrame::Request { seq, req }));
    match control(rx) {
        ControlFrame::Response { seq: s, result } => {
            assert_eq!(s, seq);
            result
        }
        other => panic!("expected response, got {other:?}"),
    }
}

#[test]
fn clap_plugin_loads_and_processes_inside_the_bridge() {
    let Some(path) = clap_path() else {
        eprintln!("no CLAP plugin available; skipping");
        return;
    };
    let Some(uid) = scan_uid(&path) else {
        eprintln!("scan helper did not report a uid for {}", path.display());
        return;
    };

    let (mut child, mut stdin, rx) = spawn_bridge();

    // Load + init inside the bridge process.
    let loaded = request(
        &mut stdin,
        &rx,
        1,
        BridgeRequest::Load {
            format: "clap".to_string(),
            path: path.to_string_lossy().into_owned(),
            uid,
            is_instrument_hint: true,
            sample_rate: 44_100.0,
            max_buffer_size: 512,
            state: None,
        },
    );
    let BridgeResponse::Loaded {
        name,
        is_instrument,
        descriptors,
        ..
    } = loaded.expect("CLAP plugin loads in the bridge")
    else {
        panic!("expected Loaded");
    };
    eprintln!(
        "loaded '{name}' in bridge, is_instrument={is_instrument}, {} params",
        descriptors.len()
    );
    assert!(is_instrument, "Odin2 is an instrument");
    assert!(!descriptors.is_empty(), "Odin2 exposes parameters");

    // Activate.
    let act = request(&mut stdin, &rx, 2, BridgeRequest::Activate);
    assert!(matches!(
        act,
        Ok(BridgeResponse::Bool(true)) | Ok(BridgeResponse::Unit)
    ));

    // A note-on, then one audio block: the round trip must succeed and carry
    // the note output back.
    send(
        &mut stdin,
        encode_frame(
            KIND_NOTE,
            0,
            &encode_note_event(&NoteEvent {
                on: true,
                pitch: 60,
                velocity: 100,
                frame: 0,
            }),
        ),
    );
    let audio = AudioRequest {
        channels: 2,
        frames: 512,
        buffer: vec![0.0; 1024],
        inputs: vec![],
    };
    send(
        &mut stdin,
        encode_frame(KIND_AUDIO_REQ, 1, &encode_audio_request(&audio)),
    );
    let (kind, seq, payload) = recv(&rx);
    assert_eq!((kind, seq), (KIND_AUDIO_RESP, 1));
    let resp = decode_audio_response(&payload).expect("audio response");
    assert!(resp.ok, "CLAP process succeeds in the bridge");
    assert_eq!(resp.buffer.len(), 1024);

    // Parameter round trip.
    let p = request(&mut stdin, &rx, 3, BridgeRequest::GetParam { index: 0 });
    assert!(matches!(p, Ok(BridgeResponse::Param(_))));
    let set = request(
        &mut stdin,
        &rx,
        4,
        BridgeRequest::SetParam {
            index: 0,
            value: 0.5,
        },
    );
    assert!(set.is_ok());

    // State save must produce a non-empty blob.
    let state = request(&mut stdin, &rx, 5, BridgeRequest::SaveState);
    match state {
        Ok(BridgeResponse::State(Some(data))) => assert!(!data.is_empty()),
        other => panic!("expected state blob, got {other:?}"),
    }

    // Quit cleanly.
    let quit = request(&mut stdin, &rx, 6, BridgeRequest::Quit);
    assert!(quit.is_ok());
    let status = child.wait().expect("bridge exits");
    assert!(status.success());
}
