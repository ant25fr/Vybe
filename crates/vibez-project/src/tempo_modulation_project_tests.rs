//! Time signatures, the tempo lane and modulators survive saving, and older
//! projects load without them.

use super::*;
use vibez_core::automation::{AutomationLane, AutomationPoint, AutomationTarget};
use vibez_core::meter::TimeSignature;
use vibez_core::modulation::{ModulationLink, Modulator};

fn project() -> Project {
    let mut project = Project::default();
    project
        .time_signatures
        .set_change(4, TimeSignature::new(7, 8).unwrap());
    let mut track = TrackInfo::new("Lead");
    track.modulators.push(Modulator {
        links: vec![ModulationLink {
            target: AutomationTarget::TrackPan,
            amount: 0.5,
        }],
        ..Default::default()
    });
    project.tracks.push(track);
    let mut tempo = AutomationLane::new(AutomationTarget::Tempo);
    tempo.insert_point(AutomationPoint {
        beat: 0.0,
        value: 0.4,
        curve: 0.0,
    });
    let mut master = TrackInfo::new("Master");
    master.id = TrackId::MASTER;
    master.automation.push(tempo);
    project.master = Some(master);
    project
}

#[test]
fn a_saved_project_keeps_signatures_tempo_and_modulators() {
    let saved = project();
    let json = serde_json::to_string(&saved).unwrap();
    let loaded: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.time_signatures, saved.time_signatures);
    assert_eq!(loaded.tracks[0].modulators, saved.tracks[0].modulators);
    assert_eq!(
        loaded.master.unwrap().automation,
        saved.master.unwrap().automation
    );
}

#[test]
fn defaults_stay_out_of_the_file_and_older_projects_load_without_them() {
    let mut plain = Project::default();
    plain.tracks.push(TrackInfo::new("Lead"));
    let json = serde_json::to_string(&plain).unwrap();
    assert!(!json.contains("time_signatures"));
    assert!(!json.contains("modulators"));
    let loaded: Project = serde_json::from_str(&json).unwrap();
    assert!(loaded.time_signatures.is_default());
    assert!(loaded.tracks[0].modulators.is_empty());
}
