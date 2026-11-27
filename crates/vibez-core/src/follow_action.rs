//! Follow actions: what a launcher Clip does by itself after a while.

use serde::{Deserialize, Serialize};

use crate::perform::MusicalBoundary;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowKind {
    /// Keep going.
    #[default]
    None,
    Stop,
    PlayAgain,
    /// The neighbouring Clips of the group: the filled slots touching this one
    /// in its column.
    Previous,
    Next,
    First,
    Last,
    /// Any Clip of the group.
    Any,
    /// Any Clip of the group but this one.
    Other,
}

impl FollowKind {
    pub const ALL: [Self; 9] = [
        Self::None,
        Self::Stop,
        Self::PlayAgain,
        Self::Previous,
        Self::Next,
        Self::First,
        Self::Last,
        Self::Any,
        Self::Other,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "No Action",
            Self::Stop => "Stop",
            Self::PlayAgain => "Play Again",
            Self::Previous => "Previous",
            Self::Next => "Next",
            Self::First => "First",
            Self::Last => "Last",
            Self::Any => "Any",
            Self::Other => "Other",
        }
    }

    /// The index in a group of `count` Clips this action picks from `current`,
    /// using `random` where it chooses; `None` stops or does nothing.
    pub fn pick(self, current: usize, count: usize, random: u64) -> Option<usize> {
        if count == 0 || current >= count {
            return None;
        }
        let draw = |span: usize| (random % span as u64) as usize;
        match self {
            Self::None | Self::Stop => None,
            Self::PlayAgain => Some(current),
            Self::Previous => Some((current + count - 1) % count),
            Self::Next => Some((current + 1) % count),
            Self::First => Some(0),
            Self::Last => Some(count - 1),
            Self::Any => Some(draw(count)),
            Self::Other if count == 1 => Some(current),
            Self::Other => {
                let other = draw(count - 1);
                Some(if other >= current { other + 1 } else { other })
            }
        }
    }
}

impl std::fmt::Display for FollowKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// How long a Clip plays before its follow action.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowTime {
    /// Once through the Clip, as Live's linked follow action does.
    #[default]
    ClipEnd,
    OneBeat,
    OneBar,
    TwoBars,
    FourBars,
    EightBars,
    SixteenBars,
}

impl FollowTime {
    pub const ALL: [Self; 7] = [
        Self::ClipEnd,
        Self::OneBeat,
        Self::OneBar,
        Self::TwoBars,
        Self::FourBars,
        Self::EightBars,
        Self::SixteenBars,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::ClipEnd => "Clip End",
            Self::OneBeat => "1 Beat",
            Self::OneBar => "1 Bar",
            Self::TwoBars => "2 Bars",
            Self::FourBars => "4 Bars",
            Self::EightBars => "8 Bars",
            Self::SixteenBars => "16 Bars",
        }
    }

    /// The time in beats for a Clip `clip_beats` long.
    pub fn beats(self, clip_beats: f64) -> f64 {
        match self {
            Self::ClipEnd => clip_beats,
            Self::OneBeat => 1.0,
            Self::OneBar => 4.0,
            Self::TwoBars => 8.0,
            Self::FourBars => 16.0,
            Self::EightBars => 32.0,
            Self::SixteenBars => 64.0,
        }
    }

    /// The coarsest launch grid a time of `beats` lands on.
    pub fn boundary(beats: f64) -> MusicalBoundary {
        let fits = |grid: f64| (beats / grid - (beats / grid).round()).abs() < 1e-3;
        if fits(4.0) {
            MusicalBoundary::OneBar
        } else if fits(1.0) {
            MusicalBoundary::OneBeat
        } else {
            MusicalBoundary::Sixteenth
        }
    }
}

impl std::fmt::Display for FollowTime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// A Clip's follow action: A or B, by chance, after `time`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FollowAction {
    pub a: FollowKind,
    pub b: FollowKind,
    /// Chance of A over B, in percent.
    pub chance_a: u8,
    pub time: FollowTime,
}

impl Default for FollowAction {
    fn default() -> Self {
        Self {
            a: FollowKind::None,
            b: FollowKind::None,
            chance_a: 100,
            time: FollowTime::default(),
        }
    }
}

impl FollowAction {
    /// Whether it can ever do something.
    pub fn is_active(&self) -> bool {
        (self.a != FollowKind::None && self.chance_a > 0)
            || (self.b != FollowKind::None && self.chance_a < 100)
    }

    /// A or B for a random draw.
    pub fn choose(&self, random: u64) -> FollowKind {
        if random % 100 < u64::from(self.chance_a.min(100)) {
            self.a
        } else {
            self.b
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_pick_clips_within_the_group() {
        assert_eq!(FollowKind::Next.pick(2, 3, 0), Some(0));
        assert_eq!(FollowKind::Previous.pick(0, 3, 0), Some(2));
        assert_eq!(FollowKind::First.pick(2, 3, 0), Some(0));
        assert_eq!(FollowKind::Last.pick(0, 3, 0), Some(2));
        assert_eq!(FollowKind::PlayAgain.pick(1, 3, 0), Some(1));
        assert_eq!(FollowKind::Stop.pick(1, 3, 0), None);
        for random in 0..12 {
            assert_ne!(FollowKind::Other.pick(1, 3, random), Some(1));
        }
        assert_eq!(FollowKind::Other.pick(0, 1, 5), Some(0));
        assert_eq!(FollowKind::Next.pick(3, 3, 0), None);
    }

    #[test]
    fn chance_splits_between_a_and_b() {
        let follow = FollowAction {
            a: FollowKind::Next,
            b: FollowKind::Stop,
            chance_a: 25,
            ..Default::default()
        };
        assert_eq!(follow.choose(24), FollowKind::Next);
        assert_eq!(follow.choose(25), FollowKind::Stop);
        assert!(follow.is_active());
        assert!(!FollowAction::default().is_active());
    }

    #[test]
    fn a_time_lands_on_the_coarsest_grid_it_fits() {
        assert_eq!(FollowTime::boundary(8.0), MusicalBoundary::OneBar);
        assert_eq!(FollowTime::boundary(3.0), MusicalBoundary::OneBeat);
        assert_eq!(FollowTime::boundary(1.5), MusicalBoundary::Sixteenth);
        assert_eq!(FollowTime::ClipEnd.beats(6.0), 6.0);
    }

    #[test]
    fn a_follow_action_round_trips_and_fills_missing_fields() {
        let follow: FollowAction = serde_json::from_str(r#"{"a":"next"}"#).unwrap();
        assert_eq!((follow.a, follow.chance_a), (FollowKind::Next, 100));
    }
}
