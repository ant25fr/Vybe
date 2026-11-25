//! Smoke test for the `vibez-plugin-bridge` subprocess: it must answer
//! control requests, refuse to load a nonexistent plugin WITHOUT dying,
//! answer audio requests (with failure) when no plugin is loaded, and exit
//! cleanly on Quit. A real crashing plugin would kill the bridge process;
//! the parent side treats that as "device bypassed" (see bridge/child.rs).

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use vibez_plugin_host::bridge::protocol::*;

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

fn recv_control(rx: &mpsc::Receiver<(u8, u64, Vec<u8>)>) -> ControlFrame {
    let (kind, _seq, payload) = rx
        .recv_timeout(Duration::from_secs(15))
        .expect("bridge answers in time");
    assert_eq!(kind, KIND_CONTROL, "expected a control frame");
    serde_json::from_slice(&payload).expect("valid control frame")
}

#[test]
fn bridge_load_failure_stays_alive_and_quits_cleanly() {
    let (mut child, mut stdin, rx) = spawn_bridge();

    let load = ControlFrame::Request {
        seq: 1,
        req: BridgeRequest::Load {
            format: "clap".to_string(),
            path: if cfg!(windows) {
                "C:\\definitely\\missing\\nope.clap".to_string()
            } else {
                "/definitely/missing/nope.clap".to_string()
            },
            uid: "00000000000000000000000000000000".to_string(),
            is_instrument_hint: false,
            sample_rate: 44_100.0,
            max_buffer_size: 512,
            state: None,
        },
    };
    stdin
        .write_all(&encode_control(&load))
        .expect("write load frame");
    stdin.flush().unwrap();
    match recv_control(&rx) {
        ControlFrame::Response { seq, result } => {
            assert_eq!(seq, 1);
            assert!(result.is_err(), "loading a missing plugin must fail");
        }
        other => panic!("expected response, got {other:?}"),
    }

    // Audio with no plugin loaded: bridge must answer ok=false, not die.
    let audio = AudioRequest {
        channels: 2,
        frames: 4,
        buffer: vec![0.0; 8],
        inputs: vec![],
    };
    stdin
        .write_all(&encode_frame(
            KIND_AUDIO_REQ,
            1,
            &encode_audio_request(&audio),
        ))
        .unwrap();
    stdin.flush().unwrap();
    let (kind, seq, payload) = rx
        .recv_timeout(Duration::from_secs(15))
        .expect("bridge answers audio frame");
    assert_eq!(kind, KIND_AUDIO_RESP);
    assert_eq!(seq, 1);
    let resp = decode_audio_response(&payload).expect("valid audio response");
    assert!(!resp.ok, "no plugin loaded -> process reports failure");

    // Control still works after all that.
    let get = ControlFrame::Request {
        seq: 2,
        req: BridgeRequest::GetParam { index: 0 },
    };
    stdin.write_all(&encode_control(&get)).unwrap();
    stdin.flush().unwrap();
    match recv_control(&rx) {
        ControlFrame::Response { seq, result } => {
            assert_eq!(seq, 2);
            assert!(result.is_err(), "no plugin loaded -> param request fails");
        }
        other => panic!("expected response, got {other:?}"),
    }

    // Quit: bridge answers and exits with code 0.
    let quit = ControlFrame::Request {
        seq: 3,
        req: BridgeRequest::Quit,
    };
    stdin.write_all(&encode_control(&quit)).unwrap();
    stdin.flush().unwrap();
    match recv_control(&rx) {
        ControlFrame::Response { seq, result } => {
            assert_eq!(seq, 3);
            assert!(result.is_ok());
        }
        other => panic!("expected response, got {other:?}"),
    }
    let status = child.wait().expect("bridge exits");
    assert!(status.success(), "bridge exits cleanly on Quit: {status}");
}
