//! Command+D: duplicates the selected notes of the open piano roll when it
//! has a selection, and falls back to the selected arrangement clips.
//! Also covers the `DuplicateNotes` message the piano roll's Ctrl+drag
//! copy sends on press.

use super::*;
use crate::domains::piano_roll::PianoRollMsg;
use crate::state::{ProjectTrack, UiNoteClip};
use iced::keyboard::{Key, Modifiers};
use std::sync::Arc;
use vibez_core::id::{ClipId, TrackId};
use vibez_core::midi::MidiNote;

fn gkh(key: Key, modifiers: Modifiers, app: &App) -> Option<Message> {
    super::keyboard::global_key_handler(key, modifiers, &app.shortcuts)
}

fn app_with_selected_notes() -> (App, TrackId, ClipId) {
    let mut app = super::test_support::app();
    app.state.view.workspace = crate::state::Workspace::Arrange;
    let mut track = ProjectTrack::new(TrackId::new(), "MIDI".into(), 0);
    track.kind = vibez_core::midi::TrackKind::Midi;
    let track_id = track.id;
    Arc::make_mut(&mut app.state.project_tracks)
        .tracks
        .push(track);
    let mut clip = UiNoteClip {
        name: "Pattern".into(),
        ..UiNoteClip::test_clip(0.0, 8.0)
    };
    clip.notes = vec![
        MidiNote {
            pitch: 60,
            velocity: 100,
            start_beat: 0.0,
            duration_beats: 1.0,
        },
        MidiNote {
            pitch: 64,
            velocity: 90,
            start_beat: 2.0,
            duration_beats: 0.5,
        },
    ];
    clip.selected_notes = [0, 1].into_iter().collect();
    let clip_id = clip.id;
    app.state
        .arrange_content_mut(track_id)
        .note_clips
        .push(clip);
    app.state.arrangement.selected_track = Some(track_id);
    app.state.arrangement.selected_note_clip = Some((track_id, clip_id));
    (app, track_id, clip_id)
}

fn note_clip_of(app: &App, track_id: TrackId, clip_id: ClipId) -> &UiNoteClip {
    app.state
        .arrange_content(track_id)
        .and_then(|content| content.note_clips.iter().find(|c| c.id == clip_id))
        .expect("the note clip is on the track")
}

#[test]
fn command_d_duplicates_the_selected_notes_in_place_and_selects_the_copies() {
    let (mut app, track_id, clip_id) = app_with_selected_notes();
    let command = if ON_MACOS {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    let message =
        gkh(Key::Character("d".into()), command, &app).expect("Command+D is a global shortcut");
    assert!(matches!(message, Message::DuplicatePressed));
    let _ = app.update(message);

    let clip = note_clip_of(&app, track_id, clip_id);
    assert_eq!(clip.notes.len(), 4);
    // Originals untouched.
    assert_eq!(clip.notes[0].pitch, 60);
    assert_eq!(clip.notes[1].start_beat, 2.0);
    // Copies appended, identical, and only they are selected.
    assert_eq!(clip.notes[2].pitch, 60);
    assert_eq!(clip.notes[2].start_beat, 0.0);
    assert_eq!(clip.notes[3].pitch, 64);
    assert_eq!(clip.notes[3].velocity, 90);
    assert_eq!(clip.selected_notes, [2, 3].into_iter().collect());
}

#[test]
fn command_d_falls_back_to_the_selected_clips_without_a_note_selection() {
    let (mut app, track_id, clip_id) = app_with_selected_notes();
    // Open the clip but select no notes: Command+D duplicates the clip.
    app.state
        .arrange_content_mut(track_id)
        .note_clips
        .iter_mut()
        .find(|c| c.id == clip_id)
        .unwrap()
        .selected_notes
        .clear();
    app.state
        .arrangement
        .selected_clips
        .insert(crate::state::ArrangementSelection::NoteClip { track_id, clip_id });

    let command = if ON_MACOS {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    let message = gkh(Key::Character("d".into()), command, &app).unwrap();
    let _ = app.update(message);

    let content = app.state.arrange_content(track_id).unwrap();
    assert_eq!(content.note_clips.len(), 2);
    assert_eq!(note_clip_of(&app, track_id, clip_id).notes.len(), 2);
}

#[test]
fn duplicate_notes_appends_copies_in_source_order_and_selects_them() {
    let (mut app, track_id, clip_id) = app_with_selected_notes();
    let _ = app.update(Message::PianoRoll(PianoRollMsg::DuplicateNotes {
        track_id,
        clip_id,
        source_indices: vec![1, 0],
    }));

    let clip = note_clip_of(&app, track_id, clip_id);
    assert_eq!(clip.notes.len(), 4);
    // source_indices [1, 0]: the copy of note 1 lands at index 2, note 0 at 3.
    assert_eq!(clip.notes[2].pitch, 64);
    assert_eq!(clip.notes[3].pitch, 60);
    assert_eq!(clip.selected_notes, [2, 3].into_iter().collect());
}
