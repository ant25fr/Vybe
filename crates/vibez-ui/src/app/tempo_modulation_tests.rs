//! Tempo, signature and modulator edits across undo, saving, freezing and
//! bouncing in place.

use std::sync::Arc;

use vibez_core::automation::AutomationTarget;
use vibez_core::id::{ClipId, TrackId};
use vibez_core::meter::TimeSignature;
use vibez_core::modulation::ModulatorKind;

use crate::domains::automation::AutomationMsg;
use crate::domains::modulation::ModulationMsg;
use crate::domains::project::ProjectMsg;
use crate::domains::transport::TransportMsg;
use crate::message::Message;
use crate::state::ProjectTrack;

use super::*;

fn app_with_track() -> (App, TrackId) {
    let mut app = super::test_support::app();
    let track = ProjectTrack::new(TrackId::new(), "Keys".into(), 1);
    let id = track.id;
    Arc::make_mut(&mut app.state.project_tracks)
        .tracks
        .push(track);
    (app, id)
}

fn send(app: &mut App, message: Message) {
    let _ = app.update(message);
}

#[test]
fn a_signature_is_one_undo_step_and_is_saved() {
    let (mut app, _) = app_with_track();
    send(
        &mut app,
        Message::Transport(TransportMsg::SignatureChanged("3/4".into())),
    );
    send(&mut app, Message::Transport(TransportMsg::SignatureSubmit));
    let waltz = TimeSignature::new(3, 4).unwrap();
    assert_eq!(app.state.transport.meter.signature_at_bar(0), waltz);
    assert_eq!(
        app.project_from_state().time_signatures.signature_at_bar(0),
        waltz
    );
    assert_eq!(app.state.project.history.undo.len(), 1);

    send(&mut app, Message::Project(ProjectMsg::Undo));
    assert!(app.state.transport.meter.is_default());
}

#[test]
fn submitting_the_current_signature_changes_nothing() {
    let (mut app, _) = app_with_track();
    send(
        &mut app,
        Message::Transport(TransportMsg::SignatureChanged("4/4".into())),
    );
    send(&mut app, Message::Transport(TransportMsg::SignatureSubmit));
    assert!(app.state.project.history.undo.is_empty());
    assert!(!app.state.project.dirty);
}

#[test]
fn modulators_undo_and_are_saved_with_their_track() {
    let (mut app, id) = app_with_track();
    send(
        &mut app,
        Message::Modulation(ModulationMsg::Add(id, ModulatorKind::Lfo)),
    );
    send(
        &mut app,
        Message::Modulation(ModulationMsg::AddLink(id, 0, AutomationTarget::TrackPan)),
    );
    assert_eq!(app.project_from_state().tracks[0].modulators.len(), 1);
    send(&mut app, Message::Project(ProjectMsg::Undo));
    send(&mut app, Message::Project(ProjectMsg::Undo));
    assert!(app.state.project_tracks.tracks[0].modulators.is_empty());
    send(&mut app, Message::Project(ProjectMsg::Redo));
    assert_eq!(app.state.project_tracks.tracks[0].modulators.len(), 1);
}

#[test]
fn a_frozen_track_keeps_the_tempo_from_being_automated() {
    let (mut app, _) = app_with_track();
    Arc::make_mut(&mut app.state.project_tracks).tracks[0].frozen_clip = Some(ClipId::new());
    send(
        &mut app,
        Message::Automation(AutomationMsg::AddTempoLane { bpm: 120.0 }),
    );
    assert!(app.state.tempo_lane().is_none());
}

#[test]
fn an_automated_tempo_refuses_bouncing_a_track() {
    let (mut app, id) = app_with_track();
    send(
        &mut app,
        Message::Automation(AutomationMsg::AddTempoLane { bpm: 120.0 }),
    );
    assert!(app.state.tempo_lane().is_some());
    let _ = app.render_track_in_place(id, true);
    assert!(app.state.status_text.contains("tempo automation"));
}

#[test]
fn bouncing_in_place_keeps_the_mixer_modulation() {
    let (mut app, id) = app_with_track();
    let audio = ProjectTrack::new(TrackId::new(), "Render".into(), 2);
    let audio_id = audio.id;
    Arc::make_mut(&mut app.state.project_tracks)
        .tracks
        .push(audio);
    for message in [
        ModulationMsg::Add(id, ModulatorKind::Lfo),
        ModulationMsg::AddLink(id, 0, AutomationTarget::TrackPan),
    ] {
        send(&mut app, Message::Modulation(message));
    }
    app.finish_track_render(
        super::track_freeze::TrackRender {
            source: id,
            replace: true,
        },
        audio_id,
    );
    let kept = app.state.find_track(audio_id).unwrap();
    assert_eq!(kept.modulators.len(), 1);
    assert_eq!(kept.modulators[0].links.len(), 1);
}
