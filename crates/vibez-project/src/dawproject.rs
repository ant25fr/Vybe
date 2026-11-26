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

    let channel_xml = |track: &TrackInfo, channel_id: &str, role: &str, dest: Option<&str>| {
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
             <Mute value=\"{}\"/>",
            track.gain.clamp(0.0, 2.0),
            track.pan.clamp(0.0, 1.0),
            track.mute
        );
        let sends: Vec<_> = track
            .sends
            .iter()
            .filter_map(|(bus, amount)| Some((bus, bus_channels.get(bus)?, amount)))
            .collect();
        if !sends.is_empty() {
            s.push_str("<Sends>");
            for (bus, destination, amount) in sends {
                let kind = if track.pre_fader_sends.contains(bus) {
                    "pre"
                } else {
                    "post"
                };
                let _ = write!(
                    s,
                    "<Send destination=\"{destination}\" type=\"{kind}\">\
                     <Volume max=\"1\" min=\"0\" unit=\"linear\" value=\"{}\"/></Send>",
                    amount.clamp(0.0, 1.0)
                );
            }
            s.push_str("</Sends>");
        }
        s.push_str("</Channel>");
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
            // A member of a group plays into the group's channel.
            channel_xml(
                track,
                channel_id,
                "regular",
                Some(
                    track
                        .output
                        .and_then(|group| bus_channels.get(&group))
                        .unwrap_or(&master_id)
                ),
            ),
        );
    }
    for bus in &project.buses {
        let color = PALETTE[usize::from(bus.color_index) % PALETTE.len()];
        let _ = write!(
            structure,
            "<Track contentType=\"audio\" loaded=\"true\" id=\"{}\" name=\"{}\" color=\"{color}\">{}</Track>",
            bus_ids[&bus.id],
            escape(&bus.name),
            channel_xml(
                bus,
                &bus_channels[&bus.id],
                if bus.is_group { "submix" } else { "effect" },
                // A nested group plays into the group around it.
                Some(
                    bus.output
                        .and_then(|group| bus_channels.get(&group))
                        .unwrap_or(&master_id)
                ),
            ),
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
        for clip in project
            .arrange
            .note_clips
            .iter()
            .filter(|c| c.track_id == track.id)
        {
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
        // Takes stay in the project: only the comp is exported.
        for clip in project
            .arrange
            .clips
            .iter()
            .filter(|c| c.track_id == track.id && c.take_lane.is_none())
        {
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
            let loop_attributes = if clip.loop_enabled && clip.loop_end > clip.loop_start {
                format!(
                    " loopStart=\"{}\" loopEnd=\"{}\"",
                    seconds(clip.loop_start),
                    seconds(clip.loop_end)
                )
            } else {
                String::new()
            };
            let _ = write!(
                clips,
                "<Clip time=\"{}\" duration=\"{}\" name=\"{}\">\
                 <Clips><Clip time=\"0\" duration=\"{}\" contentTimeUnit=\"seconds\" playStart=\"{}\"{loop_attributes}>\
                 <Audio algorithm=\"raw\" channels=\"{channels}\" duration=\"{}\" sampleRate=\"{rate}\">\
                 <File path=\"{}\"/></Audio></Clip></Clips></Clip>",
                clip.position as f64 * bps,
                clip.duration as f64 * bps,
                escape(&clip.name),
                // The window onto the file, in the beats of the timeline.
                clip.duration as f64 * bps,
                seconds(clip.start_marker.unwrap_or(clip.source_offset)),
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
         <TimeSignature denominator=\"{}\" numerator=\"{}\"/></Transport>\
         <Structure>{structure}</Structure>\
         <Arrangement><Lanes timeUnit=\"beats\">{arrangement}</Lanes></Arrangement></Project>",
        env!("CARGO_PKG_VERSION"),
        project.bpm,
        // The format's transport holds one signature: the first.
        project.time_signatures.signature_at_bar(0).denominator(),
        project.time_signatures.signature_at_bar(0).numerator(),
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
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
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
                let node = stack
                    .pop()
                    .ok_or(DawProjectError::Xml("unbalanced tags".into()))?;
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
    // Channel ids of the returns, and the sends to resolve once all are read.
    let mut bus_by_channel: HashMap<String, TrackId> = HashMap::new();
    let mut pending_sends: Vec<(TrackId, String, f32, bool)> = Vec::new();
    // The channel each track plays into, to find its group.
    let mut destinations: Vec<(TrackId, String)> = Vec::new();
    for node in tracks {
        let channel = node.child("Channel");
        let role = channel.and_then(|c| c.attr("role")).unwrap_or("regular");
        if role == "master" {
            if let Some(channel) = channel {
                let mut master = TrackInfo::new("Master");
                master.gain = value_of(channel.child("Volume"), 1.0).clamp(0.0, 2.0) as f32;
                master.pan = value_of(channel.child("Pan"), 0.5).clamp(0.0, 1.0) as f32;
                project.master = Some(master);
            }
            continue;
        }
        if channel.is_none() {
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
        if let Some(channel) = channel {
            for send in channel.child("Sends").iter().flat_map(|s| s.all("Send")) {
                if let Some(destination) = send.attr("destination") {
                    pending_sends.push((
                        track.id,
                        destination.to_string(),
                        value_of(send.child("Volume"), 0.0).clamp(0.0, 1.0) as f32,
                        send.attr("type") == Some("pre"),
                    ));
                }
            }
        }
        if let Some(destination) = channel.and_then(|c| c.attr("destination")) {
            destinations.push((track.id, destination.to_string()));
        }
        if role == "effect" || role == "submix" {
            track.is_group = role == "submix";
            if let Some(id) = channel.and_then(|c| c.attr("id")) {
                bus_by_channel.insert(id.to_string(), track.id);
            }
            project.buses.push(track);
            continue;
        }
        if let Some(id) = node.attr("id") {
            by_id.insert(id.to_string(), track.id);
        }
        project.tracks.push(track);
    }
    if project.tracks.is_empty() {
        return Err(DawProjectError::Missing("tracks"));
    }
    for (member, destination) in destinations {
        let Some(&group) = bus_by_channel.get(&destination) else {
            continue;
        };
        if !project
            .buses
            .iter()
            .any(|bus| bus.id == group && bus.is_group)
        {
            continue;
        }
        if let Some(channel) = project
            .tracks
            .iter_mut()
            .chain(project.buses.iter_mut().filter(|bus| bus.is_group))
            .find(|channel| channel.id == member && channel.id != group)
        {
            channel.output = Some(group);
        }
    }
    for (from, destination, amount, pre_fader) in pending_sends {
        let Some(&bus) = bus_by_channel.get(&destination) else {
            continue;
        };
        // A file whose returns feed each other in a circle loses the send
        // that would close it.
        let mut pending = vec![bus];
        let mut seen = Vec::new();
        let mut loops = false;
        while let Some(id) = pending.pop() {
            loops |= id == from;
            if !loops && !seen.contains(&id) {
                seen.push(id);
                if let Some(next) = project.buses.iter().find(|b| b.id == id) {
                    pending.extend(next.sends.iter().map(|(target, _)| *target));
                }
            }
        }
        if loops {
            continue;
        }
        if let Some(channel) = project
            .tracks
            .iter_mut()
            .chain(project.buses.iter_mut())
            .find(|channel| channel.id == from)
        {
            channel.sends.push((bus, amount));
            if pre_fader {
                channel.pre_fader_sends.push(bus);
            }
        }
    }

    let seconds_to_samples = |s: f64| (s.max(0.0) * f64::from(sample_rate)).round() as u64;
    let beats_to_samples = |b: f64| seconds_to_samples(b * 60.0 / project.bpm);
    let mut missing = Vec::new();
    let mut staged: HashMap<String, Option<MediaSourceRef>> = HashMap::new();

    // Each track's lane with whether its times are in seconds, a unit lanes
    // inherit from the ones around them.
    fn lanes<'a>(node: &'a XmlNode, seconds: bool, out: &mut Vec<(&'a XmlNode, bool)>) {
        for lane in node.all("Lanes") {
            let seconds = lane
                .attr("timeUnit")
                .map_or(seconds, |unit| unit == "seconds");
            if lane.attr("track").is_some() {
                out.push((lane, seconds));
            }
            lanes(lane, seconds, out);
        }
    }
    let mut track_lanes = Vec::new();
    if let Some(arrangement) = root.child("Arrangement") {
        lanes(arrangement, false, &mut track_lanes);
    }
    let beats_per_second = project.bpm / 60.0;
    for (lane, lane_seconds) in track_lanes {
        let Some(&track_id) = lane.attr("track").and_then(|id| by_id.get(id)) else {
            continue;
        };
        let Some(clips) = lane.child("Clips") else {
            continue;
        };
        let in_seconds = clips
            .attr("timeUnit")
            .map_or(lane_seconds, |unit| unit == "seconds");
        let to_beats = if in_seconds { beats_per_second } else { 1.0 };
        for clip in clips.all("Clip") {
            let time = clip.num("time").unwrap_or(0.0) * to_beats;
            let duration = clip.num("duration").unwrap_or(0.0) * to_beats;
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
                    scale: None,
                    color_index: None,
                });
                continue;
            }
            // Audio: directly in the clip or in a nested clip.
            // `window` is the clip that holds the audio, whose content unit
            // its `playStart` is written in.
            let (window, audio) = match clip.child("Audio") {
                Some(audio) => (clip, Some(audio)),
                None => clip
                    .child("Clips")
                    .and_then(|c| {
                        c.all("Clip")
                            .find_map(|inner| Some((inner, Some(inner.child("Audio")?))))
                    })
                    .unwrap_or((clip, None)),
            };
            let Some(file) = audio
                .and_then(|a| a.child("File"))
                .and_then(|f| f.attr("path"))
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
            let offset_of = |node: &XmlNode| {
                let start = node.num("playStart")?;
                // Audio content counts in seconds unless it says beats.
                let beats = node.attr("contentTimeUnit") == Some("beats");
                Some(if beats {
                    start / beats_per_second
                } else {
                    start
                })
            };
            let play_start = offset_of(window).or_else(|| offset_of(clip)).unwrap_or(0.0);
            let in_beats = window.attr("contentTimeUnit") == Some("beats");
            let content_seconds = |key: &str| {
                let value = window.num(key)?;
                Some(if in_beats {
                    value / beats_per_second
                } else {
                    value
                })
            };
            let region = content_seconds("loopStart").unwrap_or(0.0).max(0.0);
            let looped = content_seconds("loopEnd").filter(|end| *end > region);
            let start = seconds_to_samples(play_start);
            let (loop_start, loop_end) = looped.map_or((0, 0), |end| {
                (seconds_to_samples(region), seconds_to_samples(end))
            });
            // A looped clip's window opens at its loop or its start,
            // whichever comes first.
            let source_offset = if looped.is_some() {
                start.min(loop_start)
            } else {
                start
            };
            let value = serde_json::json!({
                "id": ClipId::new(),
                "track_id": track_id,
                "name": name,
                "position": beats_to_samples(time),
                "source_offset": source_offset,
                "start_marker": (start != source_offset).then_some(start),
                "loop_enabled": looped.is_some(),
                "loop_start": loop_start,
                "loop_end": loop_end,
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
            scale: None,
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
            Some(MediaSourceRef::LocalFile {
                path: sample.clone(),
            })
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
    fn a_trimmed_audio_clip_keeps_its_source_offset() {
        let dir = tempfile::tempdir().unwrap();
        let sample = dir.path().join("loop.wav");
        std::fs::write(&sample, wav(48_000)).unwrap();
        let track = TrackInfo::new("Loop");
        let mut project = Project {
            bpm: 100.0,
            sample_rate: 48_000,
            ..Project::default()
        };
        let clip = serde_json::from_value::<ClipInfo>(serde_json::json!({
            "id": ClipId::new(),
            "track_id": track.id,
            "name": "Tail",
            "position": 0,
            "source_offset": 12_000,
            "duration": 24_000,
            "source": MediaSourceRef::LocalFile { path: sample.clone() },
        }))
        .unwrap();
        project.arrange.clips.push(clip);
        project.tracks = vec![track];
        let out = dir.path().join("trim.dawproject");
        export(&project, None, &out).unwrap();

        let mut xml = String::new();
        zip::ZipArchive::new(std::fs::File::open(&out).unwrap())
            .unwrap()
            .by_name("project.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        // The offset, 0.25 s, sits on the clip whose content is in seconds.
        assert!(
            xml.contains("contentTimeUnit=\"seconds\" playStart=\"0.25\""),
            "{xml}"
        );

        let back = import(&out, 48_000, |_, _| {
            Some(MediaSourceRef::LocalFile {
                path: sample.clone(),
            })
        })
        .unwrap()
        .project;
        assert_eq!(back.arrange.clips[0].source_offset, 12_000);
        assert_eq!(back.arrange.clips[0].duration, 24_000);
    }

    #[test]
    fn returns_sends_master_pan_and_audio_loops_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let sample = dir.path().join("loop.wav");
        std::fs::write(&sample, wav(48_000)).unwrap();
        let first = TrackInfo::new("A Return");
        let mut second = TrackInfo::new("B Return");
        second.gain = 0.5;
        let mut track = TrackInfo::new("Loop");
        track.sends = vec![(first.id, 0.25)];
        track.pre_fader_sends = vec![first.id];
        let mut first = first;
        first.sends = vec![(second.id, 0.75)];
        let mut master = TrackInfo::new("Master");
        master.pan = 0.25;
        let mut project = Project {
            bpm: 120.0,
            sample_rate: 48_000,
            ..Project::default()
        };
        let clip = serde_json::from_value::<ClipInfo>(serde_json::json!({
            "id": ClipId::new(),
            "track_id": track.id,
            "name": "Beat",
            "position": 0,
            "source_offset": 0,
            "start_marker": 12_000,
            "duration": 96_000,
            "loop_enabled": true,
            "loop_start": 6_000,
            "loop_end": 36_000,
            "source": MediaSourceRef::LocalFile { path: sample.clone() },
        }))
        .unwrap();
        project.arrange.clips.push(clip);
        project.tracks = vec![track];
        project.buses = vec![first, second];
        project.master = Some(master);
        let out = dir.path().join("mix.dawproject");
        export(&project, None, &out).unwrap();
        let back = import(&out, 48_000, |_, _| {
            Some(MediaSourceRef::LocalFile {
                path: sample.clone(),
            })
        })
        .unwrap()
        .project;

        assert_eq!(back.tracks.len(), 1);
        assert_eq!(back.buses.len(), 2);
        let (first, second) = (&back.buses[0], &back.buses[1]);
        assert_eq!(first.name, "A Return");
        assert!((second.gain - 0.5).abs() < 1e-6);
        assert_eq!(back.tracks[0].sends, [(first.id, 0.25)]);
        assert_eq!(back.tracks[0].pre_fader_sends, [first.id]);
        assert_eq!(first.sends, [(second.id, 0.75)]);
        assert!(first.pre_fader_sends.is_empty());
        assert!((back.master.as_ref().unwrap().pan - 0.25).abs() < 1e-6);
        let clip = &back.arrange.clips[0];
        assert!(clip.loop_enabled);
        assert_eq!((clip.loop_start, clip.loop_end), (6_000, 36_000));
        assert_eq!(clip.source_offset, 6_000);
        assert_eq!(clip.start_marker, Some(12_000));
    }

    #[test]
    fn groups_keep_their_members() {
        let dir = tempfile::tempdir().unwrap();
        let mut group = TrackInfo::new("Drums");
        group.is_group = true;
        let mut inner = TrackInfo::new("Kit");
        inner.is_group = true;
        inner.output = Some(group.id);
        let ret = TrackInfo::new("A Return");
        let mut member = TrackInfo::new("Kick");
        member.output = Some(group.id);
        let outside = TrackInfo::new("Bass");
        let project = Project {
            tracks: vec![member, outside],
            buses: vec![group, ret, inner],
            ..Project::default()
        };
        let out = dir.path().join("groups.dawproject");
        export(&project, None, &out).unwrap();
        let back = import(&out, 48_000, |_, _| None).unwrap().project;
        let groups: Vec<_> = back.buses.iter().map(|bus| bus.is_group).collect();
        assert_eq!(groups, [true, false, true]);
        assert_eq!(back.buses[2].output, Some(back.buses[0].id));
        assert_eq!(back.tracks[0].output, Some(back.buses[0].id));
        assert_eq!(back.tracks[1].output, None);
    }

    #[test]
    fn rejects_archives_without_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("bad.dawproject");
        std::fs::write(&out, b"not a zip").unwrap();
        assert!(import(&out, 48_000, |_, _| None).is_err());
    }
}
