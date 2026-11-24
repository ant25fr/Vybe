//! Browser preview flow: seeking the playhead by mouse, and the loop-tempo
//! analysis landing after playback already started.

use std::sync::Arc;

use vibez_core::audio_buffer::DecodedAudio;
use vibez_core::track::MediaSourceRef;
use vibez_engine::commands::EngineCommand;

use crate::message::Message;

fn app_with_playing_preview() -> (super::App, rtrb::Consumer<EngineCommand>) {
    let mut app = super::test_support::app();
    let (commands, received) = rtrb::RingBuffer::new(16);
    app.cmd_tx = crate::domains::EngineCommandQueue::new(
        commands,
        vibez_engine::transport_ring::transport_ring().0,
    );
    let source = MediaSourceRef::LocalFile {
        path: "/samples/kick.wav".into(),
    };
    let browser = &mut app.state.browser;
    browser.open = true;
    browser.selected_source = Some(source.clone());
    browser.waveform_source = Some(source);
    browser.waveform_audio = Some(Arc::new(DecodedAudio {
        channels: vec![vec![0.25; 1_024]],
        sample_rate: app.state.transport.sample_rate,
    }));
    browser.audition_playing = true;
    (app, received)
}

#[test]
fn dragging_the_waveform_seeks_the_playing_audition() {
    let (mut app, mut received) = app_with_playing_preview();

    let _ = app.update(Message::SeekBrowserPreview(0.5));

    assert_eq!(app.state.browser.audition_position_frames, 512);
    let seeks: Vec<u64> = std::iter::from_fn(|| received.pop().ok())
        .filter_map(|command| match command {
            EngineCommand::SeekAudition { position_frames } => Some(position_frames),
            _ => None,
        })
        .collect();
    assert_eq!(seeks, vec![512]);
}

#[test]
fn dragging_while_stopped_only_remembers_the_position() {
    let (mut app, mut received) = app_with_playing_preview();
    app.state.browser.audition_playing = false;

    let _ = app.update(Message::SeekBrowserPreview(0.25));

    assert_eq!(app.state.browser.audition_position_frames, 256);
    assert!(std::iter::from_fn(|| received.pop().ok())
        .all(|command| !matches!(command, EngineCommand::SeekAudition { .. })));
}

#[test]
fn a_loop_fit_landing_while_a_warp_preview_plays_prepares_the_warp() {
    let (mut app, _received) = app_with_playing_preview();
    let source = app.state.browser.selected_source.clone().unwrap();
    app.state.browser.audition_mode = crate::state::AuditionMode::Warp;
    assert!(app.state.browser.waveform_loop_fit.is_none());

    let fit = crate::warp::LoopGridFit {
        bars: 4,
        source_bpm: 120.0,
    };
    let _ = app.update(Message::BrowserLoopFitReady(source, Some(fit)));

    assert_eq!(app.state.browser.waveform_loop_fit, Some(fit));
    assert!(
        app.state.browser.audition_loading,
        "warp preparation started"
    );
}

#[test]
fn a_loop_fit_for_another_source_is_dropped() {
    let (mut app, _received) = app_with_playing_preview();
    let other = MediaSourceRef::LocalFile {
        path: "/samples/snare.wav".into(),
    };

    let _ = app.update(Message::BrowserLoopFitReady(
        other,
        Some(crate::warp::LoopGridFit {
            bars: 1,
            source_bpm: 90.0,
        }),
    ));

    assert!(app.state.browser.waveform_loop_fit.is_none());
}
