//! DAWproject (`.dawproject`) import and export.
//!
//! A DAWproject is a zip holding `metadata.xml`, `project.xml` and the audio
//! it references under `audio/`. This module maps the parts of a Vibez
//! project that other DAWs can use: tempo, tracks (name, colour, volume, pan,
//! mute, solo, kind), MIDI note clips and audio clips. Devices, automation
//! and launcher content are Vibez specific and are not exported.
//!
//! Time in the file is in beats (`timeUnit="beats"`); audio clips carry their
//! source window in seconds.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use vibez_core::id::{ClipId, TrackId};
use vibez_core::midi::{MidiNote, NoteClipInfo, TrackKind};
use vibez_core::track::{ClipInfo, MediaSourceRef, TrackInfo};

use crate::Project;

#[derive(Debug)]
pub enum DawProjectError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Xml(String),
    Missing(&'static str),
}

impl std::fmt::Display for DawProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Zip(e) => write!(f, "not a valid .dawproject archive: {e}"),
            Self::Xml(e) => write!(f, "invalid project.xml: {e}"),
            Self::Missing(what) => write!(f, "the .dawproject has no {what}"),
        }
    }
}

impl std::error::Error for DawProjectError {}

impl From<std::io::Error> for DawProjectError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<zip::result::ZipError> for DawProjectError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}

/// Colours for the track palette indices, as `#rrggbb`.
const PALETTE: [&str; 8] = [
    "#e5484d", "#f76b15", "#f5d90a", "#46a758", "#12a594", "#0091ff", "#6e56cf", "#d6409f",
];

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn beats_per_sample(bpm: f64, sample_rate: u32) -> f64 {
    bpm / 60.0 / f64::from(sample_rate.max(1))
}

/// A WAV file's rate, channel count and frame count, when `bytes` is one.
fn wav_format(bytes: &[u8]) -> Option<(u32, u16, u64)> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12;
    let (mut rate, mut channels, mut bits, mut data) = (0, 0, 0, None);
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?) as usize;
        let body = at + 8;
        match &bytes[at..at + 4] {
            b"fmt " if body + 16 <= bytes.len() => {
                channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().ok()?);
                rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().ok()?);
                bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().ok()?);
            }
            b"data" => {
                data = Some(size.min(bytes.len().saturating_sub(body)));
                break;
            }
            _ => {}
        }
        at = body + size + (size & 1);
    }
    let frame_bytes = u64::from(channels) * u64::from(bits / 8);
    (rate > 0 && frame_bytes > 0).then(|| (rate, channels, data.unwrap_or(0) as u64 / frame_bytes))
}

fn read_source(source: &MediaSourceRef, container: Option<&Path>) -> Option<Vec<u8>> {
    match source {
        MediaSourceRef::LocalFile { path } => std::fs::read(path).ok(),
        MediaSourceRef::StagedProjectMedia { staging_path, .. }
        | MediaSourceRef::StagedRemoteProjectMedia { staging_path, .. } => {
            std::fs::read(staging_path).ok()
        }
        MediaSourceRef::ProjectMedia { id, .. } => {
            let container = crate::project_format_v1::ProjectContainer::open(container?).ok()?;
            container.read_media(id).ok()
        }
        MediaSourceRef::DropboxFile { .. } => None,
    }
}

struct Ids(u32);

impl Ids {
    fn next(&mut self) -> String {
        self.0 += 1;
        format!("id{}", self.0)
    }
}

/// Writes `project` to `out` as a `.dawproject`. `container` is the `.vzp`
/// the project was opened from, used to fetch its embedded media. Returns
/// the names of audio clips whose media could not be read and were skipped.
pub fn export(
    project: &Project,
    container: Option<&Path>,
    out: &Path,
) -> Result<Vec<String>, DawProjectError> {
    let mut skipped = Vec::new();
    let mut ids = Ids(0);
    let mut audio_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut file_for_source: HashMap<MediaSourceRef, Option<(String, u32, u16, u64)>> =
        HashMap::new();
    let bps = beats_per_sample(project.bpm, project.sample_rate);

    let master_id = ids.next();
    let mut structure = String::new();
    let mut track_ids: HashMap<TrackId, String> = HashMap::new();
    let mut channel_ids: HashMap<TrackId, String> = HashMap::new();
    for track in &project.tracks {
        let id = ids.next();
        let channel = ids.next();
        track_ids.insert(track.id, id.clone());
        channel_ids.insert(track.id, channel.clone());
    }
    let mut bus_ids: HashMap<TrackId, String> = HashMap::new();
    for bus in &project.buses {
        bus_ids.insert(bus.id, ids.next());
    }
    let mut bus_channels: HashMap<TrackId, String> = HashMap::new();
    for bus in &project.buses {
        bus_channels.insert(bus.id, ids.next());
    }

    let mut channel_xml = |track: &TrackInfo, channel_id: &str, role: &str, dest: Option<&str>| {
        let mut s = String::new();
        let _ = write!(
            s,
            "<Channel audioChannels=\"2\" role=\"{role}\" solo=\"{}\" id=\"{channel_id}\"",
            track.solo
        );
        if let Some(dest) = dest {
            let _ = write!(s, " destination=\"{dest}\"");
        }
        s.push('>');
        let _ = write!(
            s,
            "<Volume max=\"2\" min=\"0\" unit=\"linear\" value=\"{}\"/>\
             <Pan max=\"1\" min=\"0\" unit=\"normalized\" value=\"{}\"/>\
             <Mute value=\"{}\"/></Channel>",
            track.gain.clamp(0.0, 2.0),
            track.pan.clamp(0.0, 1.0),
            track.mute
        );
        s
    };

    for track in &project.tracks {
        let content = match track.kind {
            TrackKind::Audio => "audio",
            TrackKind::Instrument(_) | TrackKind::Midi => "notes",
        };
        let color = PALETTE[usize::from(track.color_index) % PALETTE.len()];
        let channel_id = &channel_ids[&track.id];
        let _ = write!(
            structure,
            "<Track contentType=\"{content}\" loaded=\"true\" id=\"{}\" name=\"{}\" color=\"{color}\">{}</Track>",
            track_ids[&track.id],
            escape(&track.name),
            channel_xml(track, channel_id, "regular", Some(&master_id)),
        );
    }
    for bus in &project.buses {
        let color = PALETTE[usize::from(bus.color_index) % PALETTE.len()];
        let _ = write!(
            structure,
            "<Track contentType=\"audio\" loaded=\"true\" id=\"{}\" name=\"{}\" color=\"{color}\">{}</Track>",
            bus_ids[&bus.id],
            escape(&bus.name),
            channel_xml(bus, &bus_channels[&bus.id], "effect", Some(&master_id)),
        );
    }
    if let Some(master) = &project.master {
        let _ = write!(
            structure,
            "<Track contentType=\"audio notes\" loaded=\"true\" id=\"{}\" name=\"Master\">{}</Track>",
            ids.next(),
            channel_xml(master, &master_id, "master", None),
        );
    } else {
        let _ = write!(
            structure,
            "<Track contentType=\"audio notes\" loaded=\"true\" id=\"{}\" name=\"Master\">\
             <Channel audioChannels=\"2\" role=\"master\" id=\"{master_id}\"/></Track>",
            ids.next(),
        );
    }

    let mut arrangement = String::new();
    for track in &project.tracks {
        let mut clips = String::new();
        for clip in project.arrange.note_clips.iter().filter(|c| c.track_id == track.id) {
            let _ = write!(
                clips,
                "<Clip time=\"{}\" duration=\"{}\" name=\"{}\"",
                clip.position_beats,
                clip.duration_beats,
                escape(&clip.name)
            );
            if clip.loop_enabled {
                let _ = write!(
                    clips,
                    " loopStart=\"{}\" loopEnd=\"{}\"",
                    clip.loop_start_beats, clip.loop_end_beats
                );
            }
            clips.push_str("><Notes>");
            for note in &clip.notes {
                let vel = f64::from(note.velocity) / 127.0;
                let _ = write!(
                    clips,
                    "<Note time=\"{}\" duration=\"{}\" channel=\"0\" key=\"{}\" vel=\"{vel}\" rel=\"{vel}\"/>",
                    note.start_beat, note.duration_beats, note.pitch
                );
            }
            clips.push_str("</Notes></Clip>");
        }
        for clip in project.arrange.clips.iter().filter(|c| c.track_id == track.id) {
            let Some(source) = clip.resolved_source() else {
                skipped.push(clip.name.clone());
                continue;
            };
            if !file_for_source.contains_key(source) {
                let entry = read_source(source, container).map(|bytes| {
                    let (rate, channels, frames) = wav_format(&bytes).unwrap_or((
                        project.sample_rate,
                        2,
                        clip.source_offset + clip.duration,
                    ));
                    let name = source.display_name();
                    let ext = Path::new(&name)
                        .extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .unwrap_or_else(|| "wav".into());
                    let path = format!("audio/{}-{}.{ext}", audio_files.len() + 1, sanitize(&name));
                    audio_files.push((path.clone(), bytes));
                    (path, rate, channels, frames)
                });
                file_for_source.insert(source.clone(), entry);
            }
            let Some((path, rate, channels, frames)) = file_for_source[source].clone() else {
                skipped.push(clip.name.clone());
                continue;
            };
            let seconds = |samples: u64| samples as f64 / f64::from(project.sample_rate.max(1));
            let _ = write!(
                clips,
                "<Clip time=\"{}\" duration=\"{}\" name=\"{}\" playStart=\"{}\">\
                 <Clips><Clip time=\"0\" duration=\"{}\" contentTimeUnit=\"seconds\">\
                 <Audio algorithm=\"raw\" channels=\"{channels}\" duration=\"{}\" sampleRate=\"{rate}\">\
                 <File path=\"{}\"/></Audio></Clip></Clips></Clip>",
                clip.position as f64 * bps,
                clip.duration as f64 * bps,
                escape(&clip.name),
                seconds(clip.source_offset),
                frames as f64 / f64::from(rate),
                frames as f64 / f64::from(rate),
                escape(&path),
            );
        }
        if !clips.is_empty() {
            let _ = write!(
                arrangement,
                "<Lanes track=\"{}\"><Clips>{clips}</Clips></Lanes>",
                track_ids[&track.id]
            );
        }
    }

    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n\
         <Project version=\"1.0\"><Application name=\"Vibez\" version=\"{}\"/>\
         <Transport><Tempo max=\"999\" min=\"20\" unit=\"bpm\" value=\"{}\"/>\
         <TimeSignature denominator=\"4\" numerator=\"4\"/></Transport>\
         <Structure>{structure}</Structure>\
         <Arrangement><Lanes timeUnit=\"beats\">{arrangement}</Lanes></Arrangement></Project>",
        env!("CARGO_PKG_VERSION"),
        project.bpm,
    );
    let metadata = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n\
         <MetaData><Title>{}</Title></MetaData>",
        escape(&project.name)
    );

    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let deflate = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("metadata.xml", deflate)?;
        zip.write_all(metadata.as_bytes())?;
        zip.start_file("project.xml", deflate)?;
        zip.write_all(xml.as_bytes())?;
        for (path, bytes) in &audio_files {
            zip.start_file(path, stored)?;
            zip.write_all(bytes)?;
        }
        zip.finish()?;
    }
    // Write beside the target and rename, so a failure leaves no half file.
    let temporary = out.with_extension("dawproject.part");
    std::fs::write(&temporary, buffer.into_inner())?;
    std::fs::rename(&temporary, out)?;
    Ok(skipped)
}

fn sanitize(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "audio".into());
    stem.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// An imported project plus the audio files it needs, by path in the archive.
pub struct Imported {
    pub project: Project,
    /// Audio clips whose media was missing from the archive.
    pub missing_media: Vec<String>,
}

#[derive(Default, Clone)]
struct XmlNode {
    name: String,
    attrs: HashMap<String, String>,
    children: Vec<XmlNode>,
}

impl XmlNode {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }
    fn num(&self, key: &str) -> Option<f64> {
        self.attr(key)?.parse().ok()
    }
    fn child(&self, name: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.name == name)
    }
    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlNode> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
}

fn parse_xml(text: &str) -> Result<XmlNode, DawProjectError> {
    let mut reader = Reader::from_str(text);
    let mut stack = vec![XmlNode::default()];
    let node_of = |e: &quick_xml::events::BytesStart| -> Result<XmlNode, DawProjectError> {
        let mut node = XmlNode {
            name: String::from_utf8_lossy(e.name().as_ref()).into_owned(),
            ..Default::default()
        };
        for attr in e.attributes() {
            let attr = attr.map_err(|e| DawProjectError::Xml(e.to_string()))?;
            let value = attr
                .unescape_value()
                .map_err(|e| DawProjectError::Xml(e.to_string()))?;
            node.attrs.insert(
                String::from_utf8_lossy(attr.key.as_ref()).into_owned(),
                value.into_owned(),
            );
        }
        Ok(node)
    };
    loop {
        match reader
            .read_event()
            .map_err(|e| DawProjectError::Xml(e.to_string()))?
        {
            Event::Start(e) => stack.push(node_of(&e)?),
            Event::Empty(e) => {
                let node = node_of(&e)?;
                stack.last_mut().expect("root").children.push(node);
            }
            Event::End(_) => {
                let node = stack.pop().ok_or(DawProjectError::Xml("unbalanced tags".into()))?;
                stack
                    .last_mut()
                    .ok_or(DawProjectError::Xml("unbalanced tags".into()))?
                    .children
                    .push(node);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = stack.pop().ok_or(DawProjectError::Xml("empty".into()))?;
    if !stack.is_empty() {
        return Err(DawProjectError::Xml("unclosed tag".into()));
    }
    root.children
        .into_iter()
        .next()
        .ok_or(DawProjectError::Missing("project element"))
}

fn parse_color(hex: &str) -> u8 {
    let hex = hex.trim_start_matches('#');
    let Ok(value) = u32::from_str_radix(hex, 16) else {
        return 0;
    };
    let (r, g, b) = (
        i32::from((value >> 16) as u8),
        i32::from((value >> 8) as u8),
        i32::from(value as u8),
    );
    PALETTE
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| {
            let v = u32::from_str_radix(&p[1..], 16).unwrap_or(0);
            let (pr, pg, pb) = (
                i32::from((v >> 16) as u8),
                i32::from((v >> 8) as u8),
                i32::from(v as u8),
            );
            (r - pr).pow(2) + (g - pg).pow(2) + (b - pb).pow(2)
        })
        .map_or(0, |(i, _)| i as u8)
}

fn value_of(node: Option<&XmlNode>, default: f64) -> f64 {
    node.and_then(|n| n.num("value")).unwrap_or(default)
}

/// Reads a `.dawproject`. Audio is handed to `stage` (archive path, file
/// name, bytes) which returns the media reference the clip will use.
pub fn import(
    path: &Path,
    sample_rate: u32,
    mut stage: impl FnMut(&str, &[u8]) -> Option<MediaSourceRef>,
) -> Result<Imported, DawProjectError> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let mut xml = String::new();
    archive
        .by_name("project.xml")
        .map_err(|_| DawProjectError::Missing("project.xml"))?
        .take(256 * 1024 * 1024)
        .read_to_string(&mut xml)?;
    let root = parse_xml(&xml)?;
    if root.name != "Project" {
        return Err(DawProjectError::Missing("Project element"));
    }
    let title = archive.by_name("metadata.xml").ok().and_then(|mut f| {
        let mut text = String::new();
        f.read_to_string(&mut text).ok()?;
        parse_xml(&text)
            .ok()?
            .child("Title")
            .and_then(|t| t.children.first().map(|c| c.name.clone()))
    });
    let _ = title;

    let mut project = Project {
        name: path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Imported".into()),
        sample_rate,
        ..Project::default()
    };
    if let Some(tempo) = root.child("Transport").and_then(|t| t.child("Tempo")) {
        if let Some(bpm) = tempo.num("value").filter(|b| (20.0..=999.0).contains(b)) {
            project.bpm = bpm;
        }
    }

    // Tracks, flattened from the structure tree; groups and effect/master
    // tracks are skipped, their audio is not representable here.
    let mut by_id: HashMap<String, TrackId> = HashMap::new();
    fn walk<'a>(node: &'a XmlNode, out: &mut Vec<&'a XmlNode>) {
        for child in node.all("Track") {
            out.push(child);
            walk(child, out);
        }
    }
    let mut tracks = Vec::new();
    if let Some(structure) = root.child("Structure") {
        walk(structure, &mut tracks);
    }
    for node in tracks {
        let channel = node.child("Channel");
        let role = channel.and_then(|c| c.attr("role")).unwrap_or("regular");
        if role == "master" {
            if let Some(channel) = channel {
                let mut master = TrackInfo::new("Master");
                master.gain = value_of(channel.child("Volume"), 1.0) as f32;
                project.master = Some(master);
            }
            continue;
        }
        if role == "effect" || channel.is_none() {
            continue;
        }
        let mut track = TrackInfo::new(node.attr("name").unwrap_or("Track"));
        let content = node.attr("contentType").unwrap_or("audio");
        track.kind = if content.contains("notes") && !content.contains("audio") {
            TrackKind::Midi
        } else {
            TrackKind::Audio
        };
        if let Some(channel) = channel {
            track.gain = value_of(channel.child("Volume"), 1.0).clamp(0.0, 2.0) as f32;
            track.pan = value_of(channel.child("Pan"), 0.5).clamp(0.0, 1.0) as f32;
            track.mute = channel
                .child("Mute")
                .and_then(|m| m.attr("value"))
                .is_some_and(|v| v == "true");
            track.solo = channel.attr("solo") == Some("true");
        }
        if let Some(color) = node.attr("color") {
            track.color_index = parse_color(color);
        }
        if let Some(id) = node.attr("id") {
            by_id.insert(id.to_string(), track.id);
        }
        project.tracks.push(track);
    }
    if project.tracks.is_empty() {
        return Err(DawProjectError::Missing("tracks"));
    }

    let seconds_to_samples = |s: f64| (s.max(0.0) * f64::from(sample_rate)).round() as u64;
    let beats_to_samples = |b: f64| seconds_to_samples(b * 60.0 / project.bpm);
    let mut missing = Vec::new();
    let mut staged: HashMap<String, Option<MediaSourceRef>> = HashMap::new();

    fn lanes<'a>(node: &'a XmlNode, out: &mut Vec<&'a XmlNode>) {
        for lane in node.all("Lanes") {
            if lane.attr("track").is_some() {
                out.push(lane);
            }
            lanes(lane, out);
        }
    }
    let mut track_lanes = Vec::new();
    if let Some(arrangement) = root.child("Arrangement") {
        lanes(arrangement, &mut track_lanes);
    }
    for lane in track_lanes {
        let Some(&track_id) = lane.attr("track").and_then(|id| by_id.get(id)) else {
            continue;
        };
        let Some(clips) = lane.child("Clips") else {
            continue;
        };
        for clip in clips.all("Clip") {
            let time = clip.num("time").unwrap_or(0.0);
            let duration = clip.num("duration").unwrap_or(0.0);
            let name = clip.attr("name").unwrap_or("Clip").to_string();
            if let Some(notes) = clip.child("Notes") {
                let notes: Vec<MidiNote> = notes
                    .all("Note")
                    .filter_map(|n| {
                        Some(MidiNote {
                            pitch: n.num("key")?.clamp(0.0, 127.0) as u8,
                            velocity: (n.num("vel").unwrap_or(0.8) * 127.0)
                                .round()
                                .clamp(1.0, 127.0) as u8,
                            start_beat: n.num("time")?,
                            duration_beats: n.num("duration")?.max(0.0),
                        })
                    })
                    .collect();
                project.arrange.note_clips.push(NoteClipInfo {
                    id: ClipId::new(),
                    track_id,
                    name,
                    position_beats: time,
                    duration_beats: duration,
                    notes,
                    start_marker_beats: None,
                    loop_enabled: clip.num("loopEnd").is_some(),
                    loop_start_beats: clip.num("loopStart").unwrap_or(0.0),
                    loop_end_beats: clip.num("loopEnd").unwrap_or(0.0),
                    groove_grid: Default::default(),
                    color_index: None,
                });
                continue;
            }
            // Audio: directly in the clip or in a nested clip.
            let audio = clip.child("Audio").or_else(|| {
                clip.child("Clips")
                    .and_then(|c| c.all("Clip").find_map(|inner| inner.child("Audio")))
            });
            let Some(file) = audio.and_then(|a| a.child("File")).and_then(|f| f.attr("path"))
            else {
                continue;
            };
            let source = staged
                .entry(file.to_string())
                .or_insert_with(|| {
                    let mut bytes = Vec::new();
                    archive
                        .by_name(file)
                        .ok()?
                        .take(2 * 1024 * 1024 * 1024)
                        .read_to_end(&mut bytes)
                        .ok()?;
                    stage(file, &bytes)
                })
                .clone();
            let Some(source) = source else {
                missing.push(name);
                continue;
            };
            let play_start = clip.num("playStart").unwrap_or(0.0);
            let value = serde_json::json!({
                "id": ClipId::new(),
                "track_id": track_id,
                "name": name,
                "position": beats_to_samples(time),
                "source_offset": seconds_to_samples(play_start),
                "duration": beats_to_samples(duration),
                "source": source,
            });
            if let Ok(info) = serde_json::from_value::<ClipInfo>(value) {
                project.arrange.clips.push(info);
            }
        }
    }
    Ok(Imported {
        project,
        missing_media: missing,
    })
}

/// Whether `path` names a DAWproject file.
pub fn is_dawproject(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dawproject"))
}

/// Staging for imported audio: content-addressed copies owned by Vibez.
pub fn stage_imported_audio(archive: &Path, name: &str, bytes: &[u8]) -> Option<MediaSourceRef> {
    let file_name = Path::new(name).file_name()?.to_string_lossy().into_owned();
    crate::project_format_v1::stage_local_content(&PathBuf::from(archive), &file_name, bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(frames: u32) -> Vec<u8> {
        let data = frames * 4;
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&48_000u32.to_le_bytes());
        v.extend_from_slice(&(48_000u32 * 4).to_le_bytes());
        v.extend_from_slice(&4u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data.to_le_bytes());
        v.resize(v.len() + data as usize, 0);
        v
    }

    #[test]
    fn wav_header_is_read() {
        assert_eq!(wav_format(&wav(480)), Some((48_000, 2, 480)));
        assert_eq!(wav_format(b"nope"), None);
    }

    #[test]
    fn project_round_trips_through_dawproject() {
        let dir = tempfile::tempdir().unwrap();
        let sample = dir.path().join("kick.wav");
        std::fs::write(&sample, wav(48_000)).unwrap();

        let mut project = Project {
            bpm: 100.0,
            sample_rate: 48_000,
            ..Project::default()
        };
        let mut midi = TrackInfo::new("Lead & <Bass>");
        midi.kind = TrackKind::Midi;
        midi.gain = 0.5;
        midi.pan = 0.25;
        midi.mute = true;
        let audio = TrackInfo::new("Drums");
        project.arrange.note_clips.push(NoteClipInfo {
            id: ClipId::new(),
            track_id: midi.id,
            name: "Riff".into(),
            position_beats: 4.0,
            duration_beats: 8.0,
            notes: vec![MidiNote {
                pitch: 60,
                velocity: 100,
                start_beat: 0.5,
                duration_beats: 1.0,
            }],
            start_marker_beats: None,
            loop_enabled: false,
            loop_start_beats: 0.0,
            loop_end_beats: 0.0,
            groove_grid: Default::default(),
            color_index: None,
        });
        let clip = serde_json::from_value::<ClipInfo>(serde_json::json!({
            "id": ClipId::new(),
            "track_id": audio.id,
            "name": "Kick",
            "position": 96_000,
            "source_offset": 0,
            "duration": 48_000,
            "source": MediaSourceRef::LocalFile { path: sample.clone() },
        }))
        .unwrap();
        project.arrange.clips.push(clip);
        project.tracks = vec![midi, audio];

        let out = dir.path().join("song.dawproject");
        let skipped = export(&project, None, &out).unwrap();
        assert!(skipped.is_empty());

        let imported = import(&out, 48_000, |name, bytes| {
            assert!(name.starts_with("audio/"));
            assert_eq!(wav_format(bytes), Some((48_000, 2, 48_000)));
            Some(MediaSourceRef::LocalFile { path: sample.clone() })
        })
        .unwrap();
        let back = imported.project;
        assert!((back.bpm - 100.0).abs() < 1e-9);
        assert_eq!(back.tracks.len(), 2);
        assert_eq!(back.tracks[0].name, "Lead & <Bass>");
        assert!(back.tracks[0].mute);
        assert!((back.tracks[0].gain - 0.5).abs() < 1e-6);
        assert!((back.tracks[0].pan - 0.25).abs() < 1e-6);
        assert_eq!(back.tracks[0].kind, TrackKind::Midi);
        assert_eq!(back.arrange.note_clips.len(), 1);
        let note = back.arrange.note_clips[0].notes[0];
        assert_eq!((note.pitch, note.velocity), (60, 100));
        assert_eq!(back.arrange.note_clips[0].position_beats, 4.0);
        assert_eq!(back.arrange.clips.len(), 1);
        // The position and length survive the trip through beats.
        assert_eq!(back.arrange.clips[0].position, 96_000);
        assert_eq!(back.arrange.clips[0].duration, 48_000);
    }

    #[test]
    fn rejects_archives_without_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("bad.dawproject");
        std::fs::write(&out, b"not a zip").unwrap();
        assert!(import(&out, 48_000, |_, _| None).is_err());
    }
}
