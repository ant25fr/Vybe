//! Standard MIDI Files read into beat-timed notes, one entry per track that plays.

use std::path::Path;

use crate::midi::MidiNote;

/// Extensions a file must carry to be offered as a MIDI file.
const MIDI_FILE_EXTENSIONS: &[&str] = &["mid", "midi"];

/// The General MIDI percussion channel (channel 10), zero-based.
const DRUM_CHANNEL: u8 = 9;

/// A MIDI file's notes, timed in beats from its own quarter-note resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct MidiFile {
    /// The tracks that play notes, in file order.
    pub tracks: Vec<MidiFileTrack>,
    /// The file's last End of Track, or its last note's end if that is later.
    pub length_beats: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MidiFileTrack {
    pub name: Option<String>,
    /// Sorted by start, then pitch.
    pub notes: Vec<MidiNote>,
    /// Every note sits on the General MIDI percussion channel.
    pub on_drum_channel: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiFileError {
    /// The bytes are not a Standard MIDI File.
    Malformed(String),
    /// Timed in SMPTE frames rather than beats.
    TimecodeTiming,
    /// A type 2 file, whose tracks are independent patterns.
    SequentialPatterns,
    NoNotes,
}

impl std::fmt::Display for MidiFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(reason) => write!(f, "not a readable MIDI file ({reason})"),
            Self::TimecodeTiming => f.write_str("SMPTE-timed MIDI files are not supported"),
            Self::SequentialPatterns => {
                f.write_str("type 2 MIDI files of separate patterns are not supported")
            }
            Self::NoNotes => f.write_str("the MIDI file has no notes"),
        }
    }
}

impl std::error::Error for MidiFileError {}

/// Ticks a quarter note in written files.
const WRITE_TICKS_PER_BEAT: u16 = 960;

/// A track to write: its name, notes timed in beats, and whether it plays
/// drums (written on the General MIDI percussion channel).
pub struct MidiExportTrack {
    pub name: String,
    pub notes: Vec<MidiNote>,
    pub drums: bool,
}

/// The notes an Arrangement note clip plays, timed on the timeline: loops
/// unrolled over the clip's length and notes cut where the clip or loop
/// stops them.
pub fn timeline_notes(clip: &crate::midi::NoteClipInfo) -> Vec<MidiNote> {
    let timeline = crate::clip_timeline::BeatClipTimeline::new(
        clip.resolved_start_marker_beats(),
        clip.loop_start_beats,
        clip.loop_end_beats,
        clip.duration_beats,
        clip.loop_enabled,
    );
    let mut notes = Vec::new();
    for note in &clip.notes {
        for at in timeline.occurrences_of(note.start_beat) {
            let mut length = note.duration_beats.min(clip.duration_beats - at);
            if timeline.is_looping() {
                length = length.min(clip.loop_end_beats - note.start_beat);
            }
            if length > 0.0 {
                notes.push(MidiNote {
                    start_beat: clip.position_beats + at,
                    duration_beats: length,
                    ..*note
                });
            }
        }
    }
    notes.sort_by(|a, b| {
        a.start_beat
            .total_cmp(&b.start_beat)
            .then(a.pitch.cmp(&b.pitch))
    });
    notes
}

/// A type 1 Standard MIDI File: a tempo track, then one track each.
pub fn write(tracks: &[MidiExportTrack], bpm: f64) -> Result<Vec<u8>, MidiFileError> {
    use midly::num::{u15, u24, u28, u4, u7};
    use midly::{
        Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    };

    if tracks.iter().all(|track| track.notes.is_empty()) {
        return Err(MidiFileError::NoNotes);
    }
    let event = |delta: u32, kind| TrackEvent {
        delta: u28::new(delta),
        kind,
    };
    let end = || TrackEventKind::Meta(MetaMessage::EndOfTrack);
    let ticks = |beat: f64| (beat.max(0.0) * f64::from(WRITE_TICKS_PER_BEAT)).round() as u32;
    let micros = (60_000_000.0 / bpm.max(1.0)).round() as u32;
    let mut smf = Smf::new(Header::new(
        Format::Parallel,
        Timing::Metrical(u15::new(WRITE_TICKS_PER_BEAT)),
    ));
    smf.tracks.push(vec![
        event(
            0,
            TrackEventKind::Meta(MetaMessage::Tempo(u24::new(micros.min(0x00FF_FFFF)))),
        ),
        event(
            0,
            TrackEventKind::Meta(MetaMessage::TimeSignature(4, 2, 24, 8)),
        ),
        event(0, end()),
    ]);
    for track in tracks.iter().filter(|track| !track.notes.is_empty()) {
        let channel = u4::new(if track.drums { DRUM_CHANNEL } else { 0 });
        // (tick, is a note-on, pitch, velocity): offs sort before ons.
        let mut edges: Vec<(u32, bool, u8, u8)> = Vec::new();
        let mut notes = track.notes.clone();
        notes.sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
        for (index, note) in notes.iter().enumerate() {
            let start = ticks(note.start_beat);
            let mut stop = ticks(note.start_beat + note.duration_beats).max(start + 1);
            // A repeat of the same key ends the one before it.
            if let Some(next) = notes[index + 1..]
                .iter()
                .find(|next| next.pitch == note.pitch)
            {
                stop = stop.min(ticks(next.start_beat));
            }
            if stop > start {
                edges.push((
                    start,
                    true,
                    note.pitch.min(127),
                    note.velocity.clamp(1, 127),
                ));
                edges.push((stop, false, note.pitch.min(127), 0));
            }
        }
        edges.sort_unstable_by_key(|&(tick, on, pitch, _)| (tick, on, pitch));
        let mut events = vec![event(
            0,
            TrackEventKind::Meta(MetaMessage::TrackName(track.name.as_bytes())),
        )];
        let mut last = 0;
        for (tick, on, pitch, velocity) in edges {
            let (key, vel) = (u7::new(pitch), u7::new(velocity));
            let message = if on {
                MidiMessage::NoteOn { key, vel }
            } else {
                MidiMessage::NoteOff { key, vel }
            };
            events.push(event(
                tick - last,
                TrackEventKind::Midi { channel, message },
            ));
            last = tick;
        }
        events.push(event(0, end()));
        smf.tracks.push(events);
    }
    let mut bytes = Vec::new();
    smf.write_std(&mut bytes)
        .map_err(|error| MidiFileError::Malformed(error.to_string()))?;
    Ok(bytes)
}

pub fn is_midi_file_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            MIDI_FILE_EXTENSIONS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
}

impl MidiFile {
    pub fn read(bytes: &[u8]) -> Result<Self, MidiFileError> {
        let smf = midly::Smf::parse(bytes)
            .map_err(|error| MidiFileError::Malformed(error.to_string()))?;
        let ticks_per_beat = match smf.header.timing {
            midly::Timing::Metrical(ticks) => f64::from(ticks.as_int().max(1)),
            midly::Timing::Timecode(..) => return Err(MidiFileError::TimecodeTiming),
        };
        if smf.header.format == midly::Format::Sequential {
            return Err(MidiFileError::SequentialPatterns);
        }
        let beats = |tick: u64| tick as f64 / ticks_per_beat;
        let mut end_tick = 0;
        let mut tracks = Vec::new();
        for events in &smf.tracks {
            let track = TickTrack::read(events);
            end_tick = end_tick.max(track.end_tick);
            if track.notes.is_empty() {
                continue;
            }
            let mut notes: Vec<_> = track
                .notes
                .iter()
                .map(|note| MidiNote {
                    pitch: note.pitch,
                    velocity: note.velocity,
                    start_beat: beats(note.start),
                    // A note ended on its own tick still sounds for one.
                    duration_beats: beats((note.end - note.start).max(1)),
                })
                .collect();
            MidiNote::sort(&mut notes);
            tracks.push(MidiFileTrack {
                name: track.name,
                on_drum_channel: track.notes.iter().all(|note| note.channel == DRUM_CHANNEL),
                notes,
            });
        }
        if tracks.is_empty() {
            return Err(MidiFileError::NoNotes);
        }
        let last_note_end = tracks
            .iter()
            .flat_map(|track| &track.notes)
            .map(MidiNote::end_beat)
            .fold(0.0, f64::max);
        Ok(Self {
            tracks,
            length_beats: beats(end_tick).max(last_note_end),
        })
    }

    /// Every track's notes as one sorted list, for a single Clip.
    pub fn merged_notes(&self) -> Vec<MidiNote> {
        let mut notes: Vec<_> = self
            .tracks
            .iter()
            .flat_map(|track| track.notes.iter().copied())
            .collect();
        MidiNote::sort(&mut notes);
        notes
    }

    /// Every track is on the drum channel.
    pub fn on_drum_channel(&self) -> bool {
        self.tracks.iter().all(|track| track.on_drum_channel)
    }
}

struct TickNote {
    channel: u8,
    pitch: u8,
    velocity: u8,
    start: u64,
    end: u64,
}

/// One track's name and notes in ticks, paired from its note-on and note-off
/// events, and the tick it ends on.
struct TickTrack {
    name: Option<String>,
    notes: Vec<TickNote>,
    end_tick: u64,
}

impl TickTrack {
    fn read(events: &[midly::TrackEvent<'_>]) -> Self {
        let mut reader = Self {
            name: None,
            notes: Vec::new(),
            end_tick: 0,
        };
        // Indices into `notes` still sounding, oldest first per channel and
        // key, so overlapping notes on one key end first in, first out.
        let mut sounding: Vec<usize> = Vec::new();
        let mut tick = 0;
        for event in events {
            tick += u64::from(event.delta.as_int());
            match event.kind {
                midly::TrackEventKind::Midi { channel, message } => {
                    let channel = channel.as_int();
                    match message {
                        midly::MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                            sounding.push(reader.notes.len());
                            reader.notes.push(TickNote {
                                channel,
                                pitch: key.as_int(),
                                velocity: vel.as_int(),
                                start: tick,
                                end: tick,
                            });
                        }
                        midly::MidiMessage::NoteOn { key, .. }
                        | midly::MidiMessage::NoteOff { key, .. } => {
                            let notes = &mut reader.notes;
                            if let Some(slot) = sounding.iter().position(|&index| {
                                notes[index].channel == channel
                                    && notes[index].pitch == key.as_int()
                            }) {
                                notes[sounding.remove(slot)].end = tick;
                            }
                        }
                        _ => {}
                    }
                }
                // midly reads on to the end of the chunk; whatever follows
                // End of Track is not part of the track.
                midly::TrackEventKind::Meta(midly::MetaMessage::EndOfTrack) => break,
                midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(name)) => {
                    reader.name = Some(String::from_utf8_lossy(name).trim().to_string())
                        .filter(|name| !name.is_empty());
                }
                _ => {}
            }
        }
        reader.end_tick = tick;
        for index in sounding {
            reader.notes[index].end = tick;
        }
        reader
    }
}

#[cfg(test)]
#[path = "midi_file_tests.rs"]
mod tests;

#[cfg(test)]
mod export_tests {
    use super::*;
    use crate::id::{ClipId, TrackId};
    use crate::midi::NoteClipInfo;

    fn note(pitch: u8, start_beat: f64, duration_beats: f64) -> MidiNote {
        MidiNote {
            pitch,
            velocity: 90,
            start_beat,
            duration_beats,
        }
    }

    #[test]
    fn a_looped_clip_is_unrolled_and_reads_back() {
        let clip = NoteClipInfo {
            id: ClipId::new(),
            track_id: TrackId::new(),
            name: "Loop".into(),
            position_beats: 4.0,
            duration_beats: 5.0,
            notes: vec![note(60, 0.0, 1.0), note(64, 1.5, 2.0)],
            start_marker_beats: None,
            loop_enabled: true,
            loop_start_beats: 0.0,
            loop_end_beats: 2.0,
            groove_grid: Default::default(),
            scale: None,
            color_index: None,
        };
        let notes = timeline_notes(&clip);
        let timed: Vec<_> = notes
            .iter()
            .map(|n| (n.pitch, n.start_beat, n.duration_beats))
            .collect();
        assert_eq!(
            timed,
            [
                (60, 4.0, 1.0),
                (64, 5.5, 0.5),
                (60, 6.0, 1.0),
                (64, 7.5, 0.5),
                (60, 8.0, 1.0),
            ]
        );
        let bytes = write(
            &[MidiExportTrack {
                name: "Keys".into(),
                notes: notes.clone(),
                drums: false,
            }],
            100.0,
        )
        .unwrap();
        let read = MidiFile::read(&bytes).unwrap();
        assert_eq!(read.tracks.len(), 1);
        assert_eq!(read.tracks[0].name.as_deref(), Some("Keys"));
        assert_eq!(read.tracks[0].notes, notes);
    }

    #[test]
    fn nothing_to_play_is_an_error() {
        assert_eq!(write(&[], 120.0), Err(MidiFileError::NoNotes));
    }
}
