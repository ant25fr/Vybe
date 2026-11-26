//! Tests of time signature editing and the automated tempo's lock.

use super::*;
use crate::domains::test_support::RecordingEngine;
use vibez_core::meter::TimeSignature;

fn submit(transport: &mut TransportState, engine: &mut RecordingEngine, text: &str) {
    transport.update(
        TransportMsg::SignatureChanged(text.into()),
        engine,
        TransportCtx::default(),
    );
    transport.update(
        TransportMsg::SignatureSubmit,
        engine,
        TransportCtx::default(),
    );
}

#[test]
fn a_signature_takes_effect_from_the_play_start_bar() {
    let mut transport = TransportState::default();
    let mut engine = RecordingEngine::default();
    transport.play_start_beats = 9.0;
    submit(&mut transport, &mut engine, "3/4");
    assert_eq!(
        transport.meter.signature_at_bar(1),
        TimeSignature::default()
    );
    assert_eq!(
        transport.meter.signature_at_bar(2),
        TimeSignature::new(3, 4).unwrap()
    );
    assert!(matches!(
        engine.0.as_slice(),
        [EngineCommand::SetMeterMap(_)]
    ));
    assert_eq!(transport.signature_text, None);

    // The signature before the change removes it.
    submit(&mut transport, &mut engine, "4/4");
    assert!(transport.meter.is_default());
}

#[test]
fn an_unchanged_or_unreadable_signature_reaches_nothing() {
    let mut transport = TransportState::default();
    let mut engine = RecordingEngine::default();
    submit(&mut transport, &mut engine, "4/4");
    submit(&mut transport, &mut engine, "nonsense");
    assert!(engine.0.is_empty());
}

#[test]
fn an_automated_tempo_refuses_manual_edits() {
    let mut transport = TransportState::default();
    let mut engine = RecordingEngine::default();
    let action = transport.update(
        TransportMsg::NudgeBpm(1.0),
        &mut engine,
        TransportCtx {
            tempo_automated: true,
            ..TransportCtx::default()
        },
    );
    assert_eq!(action, TransportAction::TempoAutomated);
    assert_eq!(transport.bpm, 120.0);
    assert!(engine.0.is_empty());
}
