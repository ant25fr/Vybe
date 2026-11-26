//! Exporting the project's parts: stems (one WAV per track and return) and
//! the Arrangement's notes as a Standard MIDI File.

use std::collections::HashSet;

use vibez_core::id::TrackId;
use vibez_core::midi::InstrumentKind;
use vibez_core::midi_file::{timeline_notes, MidiExportTrack};
use vibez_engine::render::{BounceMode, BounceRequest, OfflinePlugins};

use super::*;

pub(super) fn pick_stem_folder() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title("Export Stems into a folder")
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf())
        },
        Message::StemFolderSelected,
    )
}

/// `name` as part of a file name on every platform.
fn file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').to_string();
    if cleaned.is_empty() {
        "Track".into()
    } else {
        cleaned
    }
}

/// A path in `folder` for `name`.wav that overwrites nothing.
fn free_path(folder: &std::path::Path, name: &str) -> PathBuf {
    let mut path = folder.join(format!("{name}.wav"));
    let mut copy = 2;
    while path.exists() {
        path = folder.join(format!("{name} ({copy}).wav"));
        copy += 1;
    }
    path
}

/// The channels a stem export writes, numbered in mixer order. Muted
/// channels are silent in the mix and are left out.
fn stem_list(request: &BounceRequest) -> Vec<(TrackId, String)> {
    request
        .tracks
        .iter()
        .chain(request.buses.iter())
        .enumerate()
        .filter(|(_, channel)| !channel.mute)
        .map(|(index, channel)| {
            (
                channel.id,
                format!("{:02} {}", index + 1, file_stem(&channel.name)),
            )
        })
        .collect()
}

pub(super) async fn stems_async(
    request: BounceRequest,
    mut plugins: OfflinePlugins,
    folder: PathBuf,
    stems: Vec<(TrackId, String)>,
    progress: Arc<std::sync::atomic::AtomicU8>,
    plugin_return: std::sync::mpsc::Sender<OfflinePlugins>,
) -> Result<PathBuf, String> {
    tokio::task::spawn_blocking(move || {
        use std::sync::atomic::Ordering;

        let outcome = (|| {
            let ids: Vec<TrackId> = stems.iter().map(|(id, _)| *id).collect();
            let (rendered, warnings) = vibez_engine::render::render_stems_with_plugins(
                &request,
                &mut plugins,
                &ids,
                |percent| {
                    let scaled = 10 + ((u16::from(percent) * 80) / 100) as u8;
                    progress.store(scaled.min(90), Ordering::Relaxed);
                },
            )?;
            if !warnings.is_empty() {
                return Err(format!("render was incomplete: {}", warnings.join("; ")));
            }
            for (id, audio) in rendered {
                let Some((_, name)) = stems.iter().find(|(stem, _)| *stem == id) else {
                    continue;
                };
                vibez_audio_io::file_io::write_wav_file(&free_path(&folder, name), &audio)
                    .map_err(|error| format!("could not write stem {name}: {error}"))?;
            }
            progress.store(100, Ordering::Relaxed);
            Ok(folder)
        })();
        // Plugins are torn down on the main thread.
        let _ = plugin_return.send(plugins);
        outcome
    })
    .await
    .map_err(|error| format!("export task failed: {error}"))?
}

impl App {
    pub(super) fn export_stems(&mut self, folder: Option<PathBuf>) -> Task<Message> {
        let Some(folder) = folder else {
            return Task::none();
        };
        if self.export_job.is_some() || self.export_render_progress.is_some() {
            self.state.status_text = "An export is already in progress".into();
            return Task::none();
        }
        let total = self.state.total_duration_samples();
        if total == 0 {
            self.state.status_text = "Nothing to export: project is empty".into();
            return Task::none();
        }
        let mut request = self.offline_render_request(BounceMode::Master, (0, total));
        // Solo is for listening: every stem is written.
        for channel in request.tracks.iter_mut().chain(request.buses.iter_mut()) {
            channel.solo = false;
        }
        let stems = stem_list(&request);
        if stems.is_empty() {
            self.state.status_text = "Nothing to export: every channel is muted".into();
            return Task::none();
        }
        let task = self.start_offline_render(request, folder, None);
        if let Some(job) = self.export_job.as_mut() {
            job.stems = Some(stems);
        }
        task
    }

    /// The Arrangement's note tracks, as they play.
    fn midi_export_tracks(&self) -> Vec<MidiExportTrack> {
        let project = self.project_for_save();
        let mut seen = HashSet::new();
        project
            .tracks
            .iter()
            .filter(|track| seen.insert(track.id))
            .filter_map(|track| {
                let notes: Vec<_> = project
                    .arrange
                    .note_clips
                    .iter()
                    .filter(|clip| clip.track_id == track.id)
                    .flat_map(timeline_notes)
                    .collect();
                (!notes.is_empty()).then(|| MidiExportTrack {
                    name: track.name.clone(),
                    notes,
                    drums: track.instrument == Some(InstrumentKind::DrumRack),
                })
            })
            .collect()
    }

    pub(super) fn pick_midi_export_path(&mut self) -> Task<Message> {
        if self.midi_export_tracks().is_empty() {
            self.state.status_text = "Nothing to export: the Arrangement has no notes".into();
            return Task::none();
        }
        let default_name = self
            .state
            .project
            .current_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .map_or_else(
                || "vibez-export.mid".to_string(),
                |name| format!("{}.mid", name.to_string_lossy()),
            );
        Task::perform(
            async move {
                rfd::AsyncFileDialog::new()
                    .set_title("Export MIDI")
                    .set_file_name(&default_name)
                    .add_filter("MIDI", &["mid"])
                    .save_file()
                    .await
                    .map(|file| file.path().to_path_buf())
            },
            Message::MidiExportPathSelected,
        )
    }

    pub(super) fn export_midi(&mut self, path: Option<PathBuf>) {
        let Some(mut path) = path else {
            return;
        };
        if !vibez_core::midi_file::is_midi_file_path(&path) {
            path.set_extension("mid");
        }
        let written =
            vibez_core::midi_file::write(&self.midi_export_tracks(), self.state.transport.bpm)
                .map_err(|error| error.to_string())
                .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
        self.state.status_text = match written {
            Ok(()) => format!("Exported MIDI: {}", path.display()),
            Err(error) => format!("MIDI export failed: {error}"),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_files_have_safe_names_and_never_overwrite() {
        assert_eq!(file_stem("Kick/Snare: A?"), "Kick_Snare_ A_");
        assert_eq!(file_stem("  .. "), "Track");
        let folder = std::env::temp_dir().join(format!("vibez-stems-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let first = free_path(&folder, "01 Bass");
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(
            free_path(&folder, "01 Bass"),
            folder.join("01 Bass (2).wav")
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
