//! Tests of time signature maps.

use super::*;

fn signature(numerator: u8, denominator: u8) -> TimeSignature {
    TimeSignature::new(numerator, denominator).unwrap()
}

#[test]
fn only_musical_signatures_exist() {
    assert!(TimeSignature::new(0, 4).is_none());
    assert!(TimeSignature::new(33, 4).is_none());
    assert!(TimeSignature::new(4, 3).is_none());
    assert_eq!(TimeSignature::parse(" 7 / 8 "), Some(signature(7, 8)));
    assert_eq!(TimeSignature::parse("7"), None);
    assert_eq!(signature(7, 8).bar_length(), 3.5);
}

#[test]
fn bars_follow_the_signature_in_effect() {
    let mut map = MeterMap::default();
    assert!(map.set_change(2, signature(3, 4)));
    assert!(map.set_change(4, signature(7, 8)));
    assert_eq!(map.bar_start(2), 8.0);
    assert_eq!(map.bar_start(4), 14.0);
    assert_eq!(map.bar_start(6), 21.0);
    let position = map.position_at(17.5);
    assert_eq!((position.bar, position.start_beat), (5, 17.5));
    assert_eq!(map.position_at(13.999).bar, 3);
    assert!(map.is_bar_line(11.0));
    assert!(!map.is_bar_line(12.0));
}

#[test]
fn a_position_counts_the_signature_beats() {
    let mut map = MeterMap::default();
    assert_eq!(map.position_label(5.75), "2.2.4");
    map.set_change(0, signature(6, 8));
    // Three beats of an eighth, plus one sixteenth.
    assert_eq!(map.position_label(1.75), "1.4.2");
    assert_eq!(map.position_label(3.0), "2.1.1");
}

#[test]
fn changes_stay_sorted_unique_and_meaningful() {
    let mut map = MeterMap::default();
    assert!(!map.set_change(8, TimeSignature::default()));
    assert!(map.set_change(8, signature(3, 4)));
    assert!(map.set_change(4, signature(5, 4)));
    assert!(!map.set_change(4, signature(5, 4)));
    let bars: Vec<u32> = map.changes().iter().map(|change| change.bar).collect();
    assert_eq!(bars, vec![0, 4, 8]);
    assert!(!map.remove_change(0));
    assert!(map.remove_change(4));
    assert_eq!(map.signature_at_bar(7), TimeSignature::default());
    assert_eq!(map.signature_at_bar(8), signature(3, 4));
}

#[test]
fn a_full_map_refuses_a_new_bar() {
    let mut map = MeterMap::default();
    for bar in 1..MAX_METER_CHANGES as u32 {
        let numerator = if bar % 2 == 0 { 4 } else { 3 };
        assert!(map.set_change(bar, signature(numerator, 4)));
    }
    assert!(!map.set_change(100, signature(5, 4)));
    assert_eq!(map.changes().len(), MAX_METER_CHANGES);
}

#[test]
fn a_map_round_trips_and_rejects_bad_signatures() {
    let mut map = MeterMap::default();
    map.set_change(0, signature(3, 4));
    map.set_change(2, signature(12, 8));
    let json = serde_json::to_string(&map).unwrap();
    assert_eq!(serde_json::from_str::<MeterMap>(&json).unwrap(), map);
    assert!(serde_json::from_str::<MeterMap>(r#"[{"bar":0,"signature":[4,5]}]"#).is_err());
}
