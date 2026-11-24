//! The sample map's kinds: what sort of sound a file name says it is, and
//! the colour each kind's points get. Detection follows the reference
//! browser: whole words of the file name (then its two nearest folders),
//! two-word phrases before single words, word prefixes for a fixed list,
//! and the most specific kind in the name wins.

/// The kinds a sample-map point can have, most specific first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleMapKind {
    Kick,
    Snare,
    Clap,
    ClosedHat,
    OpenHat,
    Rim,
    Tom,
    Crash,
    Ride,
    Hat,
    Cymbal,
    Percussion,
    Vocal,
    Bass,
    Tonal,
    Effect,
    Other,
}

/// Every kind but Other, in specificity order (layout and legend order).
pub const SAMPLE_MAP_KINDS: [SampleMapKind; 16] = [
    SampleMapKind::Kick,
    SampleMapKind::Snare,
    SampleMapKind::Clap,
    SampleMapKind::ClosedHat,
    SampleMapKind::OpenHat,
    SampleMapKind::Rim,
    SampleMapKind::Tom,
    SampleMapKind::Crash,
    SampleMapKind::Ride,
    SampleMapKind::Hat,
    SampleMapKind::Cymbal,
    SampleMapKind::Percussion,
    SampleMapKind::Vocal,
    SampleMapKind::Bass,
    SampleMapKind::Tonal,
    SampleMapKind::Effect,
];

impl SampleMapKind {
    pub fn label(self) -> &'static str {
        match self {
            SampleMapKind::Kick => "Kick",
            SampleMapKind::Snare => "Snare",
            SampleMapKind::Clap => "Clap",
            SampleMapKind::ClosedHat => "Closed hat",
            SampleMapKind::OpenHat => "Open hat",
            SampleMapKind::Rim => "Rim",
            SampleMapKind::Tom => "Tom",
            SampleMapKind::Crash => "Crash",
            SampleMapKind::Ride => "Ride",
            SampleMapKind::Hat => "Hat",
            SampleMapKind::Cymbal => "Cymbal",
            SampleMapKind::Percussion => "Percussion",
            SampleMapKind::Vocal => "Vocal",
            SampleMapKind::Bass => "Bass",
            SampleMapKind::Tonal => "Tonal",
            SampleMapKind::Effect => "Effect",
            SampleMapKind::Other => "Other",
        }
    }

    pub fn color(self) -> iced::Color {
        match self {
            SampleMapKind::Kick => iced::Color::from_rgb(0.82, 0.43, 0.37),
            SampleMapKind::Snare => iced::Color::from_rgb(0.88, 0.56, 0.27),
            SampleMapKind::Clap => iced::Color::from_rgb(0.91, 0.78, 0.35),
            SampleMapKind::ClosedHat => iced::Color::from_rgb(0.66, 0.79, 0.35),
            SampleMapKind::OpenHat => iced::Color::from_rgb(0.45, 0.76, 0.45),
            SampleMapKind::Hat => iced::Color::from_rgb(0.35, 0.68, 0.55),
            SampleMapKind::Rim => iced::Color::from_rgb(0.31, 0.64, 0.65),
            SampleMapKind::Tom => iced::Color::from_rgb(0.36, 0.56, 0.84),
            SampleMapKind::Crash => iced::Color::from_rgb(0.60, 0.48, 0.88),
            SampleMapKind::Ride => iced::Color::from_rgb(0.49, 0.42, 0.78),
            SampleMapKind::Cymbal => iced::Color::from_rgb(0.69, 0.48, 0.88),
            SampleMapKind::Percussion => iced::Color::from_rgb(0.84, 0.50, 0.72),
            SampleMapKind::Vocal => iced::Color::from_rgb(0.88, 0.42, 0.54),
            SampleMapKind::Bass => iced::Color::from_rgb(0.43, 0.48, 0.82),
            SampleMapKind::Tonal => iced::Color::from_rgb(0.31, 0.69, 0.78),
            SampleMapKind::Effect => iced::Color::from_rgb(0.75, 0.75, 0.80),
            SampleMapKind::Other => iced::Color::from_rgb(0.50, 0.50, 0.55),
        }
    }

    /// Specificity rank: a drum kind beats Hat/Cymbal, those beat
    /// Percussion, then Vocal, Bass, Tonal, Effect.
    pub fn rank(self) -> u8 {
        match self {
            SampleMapKind::Kick => 0,
            SampleMapKind::Snare => 1,
            SampleMapKind::Clap => 2,
            SampleMapKind::ClosedHat => 3,
            SampleMapKind::OpenHat => 4,
            SampleMapKind::Rim => 5,
            SampleMapKind::Tom => 6,
            SampleMapKind::Crash => 7,
            SampleMapKind::Ride => 8,
            SampleMapKind::Hat => 9,
            SampleMapKind::Cymbal => 10,
            SampleMapKind::Percussion => 11,
            SampleMapKind::Vocal => 12,
            SampleMapKind::Bass => 13,
            SampleMapKind::Tonal => 14,
            SampleMapKind::Effect => 15,
            SampleMapKind::Other => 16,
        }
    }
}

/// Whole-word table: two-word phrases first, then single words.
fn word_table() -> &'static [(&'static [&'static str], SampleMapKind)] {
    &[
        (&["bass", "drum"], SampleMapKind::Kick),
        (&["closed", "hat"], SampleMapKind::ClosedHat),
        (&["hat", "closed"], SampleMapKind::ClosedHat),
        (&["closed", "hihat"], SampleMapKind::ClosedHat),
        (&["hihat", "closed"], SampleMapKind::ClosedHat),
        (&["hh", "closed"], SampleMapKind::ClosedHat),
        (&["open", "hat"], SampleMapKind::OpenHat),
        (&["hat", "open"], SampleMapKind::OpenHat),
        (&["open", "hihat"], SampleMapKind::OpenHat),
        (&["hihat", "open"], SampleMapKind::OpenHat),
        (&["hh", "open"], SampleMapKind::OpenHat),
        (&["floor", "tom"], SampleMapKind::Tom),
        (&["kick"], SampleMapKind::Kick),
        (&["kik"], SampleMapKind::Kick),
        (&["kck"], SampleMapKind::Kick),
        (&["bd"], SampleMapKind::Kick),
        (&["bassdrum"], SampleMapKind::Kick),
        (&["snare"], SampleMapKind::Snare),
        (&["snr"], SampleMapKind::Snare),
        (&["sd"], SampleMapKind::Snare),
        (&["clap"], SampleMapKind::Clap),
        (&["clp"], SampleMapKind::Clap),
        (&["cp"], SampleMapKind::Clap),
        (&["snap"], SampleMapKind::Clap),
        (&["chh"], SampleMapKind::ClosedHat),
        (&["closedhat"], SampleMapKind::ClosedHat),
        (&["hhc"], SampleMapKind::ClosedHat),
        (&["ohh"], SampleMapKind::OpenHat),
        (&["openhat"], SampleMapKind::OpenHat),
        (&["hho"], SampleMapKind::OpenHat),
        (&["hat"], SampleMapKind::Hat),
        (&["hihat"], SampleMapKind::Hat),
        (&["hh"], SampleMapKind::Hat),
        (&["rim"], SampleMapKind::Rim),
        (&["rimshot"], SampleMapKind::Rim),
        (&["rs"], SampleMapKind::Rim),
        (&["stick"], SampleMapKind::Rim),
        (&["tom"], SampleMapKind::Tom),
        (&["toms"], SampleMapKind::Tom),
        (&["crash"], SampleMapKind::Crash),
        (&["ride"], SampleMapKind::Ride),
        (&["cymbal"], SampleMapKind::Cymbal),
        (&["cym"], SampleMapKind::Cymbal),
        (&["china"], SampleMapKind::Cymbal),
        (&["splash"], SampleMapKind::Cymbal),
        (&["perc"], SampleMapKind::Percussion),
        (&["percussion"], SampleMapKind::Percussion),
        (&["shaker"], SampleMapKind::Percussion),
        (&["shk"], SampleMapKind::Percussion),
        (&["tamb"], SampleMapKind::Percussion),
        (&["tambourine"], SampleMapKind::Percussion),
        (&["conga"], SampleMapKind::Percussion),
        (&["bongo"], SampleMapKind::Percussion),
        (&["cowbell"], SampleMapKind::Percussion),
        (&["clave"], SampleMapKind::Percussion),
        (&["block"], SampleMapKind::Percussion),
        (&["triangle"], SampleMapKind::Percussion),
        (&["808"], SampleMapKind::Bass),
        (&["bass"], SampleMapKind::Bass),
        (&["sub"], SampleMapKind::Bass),
        (&["stab"], SampleMapKind::Tonal),
        (&["chord"], SampleMapKind::Tonal),
        (&["pluck"], SampleMapKind::Tonal),
        (&["keys"], SampleMapKind::Tonal),
        (&["synth"], SampleMapKind::Tonal),
        (&["lead"], SampleMapKind::Tonal),
        (&["pad"], SampleMapKind::Tonal),
        (&["vox"], SampleMapKind::Vocal),
        (&["vocal"], SampleMapKind::Vocal),
        (&["voice"], SampleMapKind::Vocal),
        (&["chant"], SampleMapKind::Vocal),
        (&["fx"], SampleMapKind::Effect),
        (&["sfx"], SampleMapKind::Effect),
        (&["riser"], SampleMapKind::Effect),
        (&["impact"], SampleMapKind::Effect),
        (&["sweep"], SampleMapKind::Effect),
        (&["noise"], SampleMapKind::Effect),
    ]
}

/// Words that also count at the start of a longer word (Kickdrum, Bassline…).
const PREFIX_WORDS: &[(&str, SampleMapKind)] = &[
    ("kick", SampleMapKind::Kick),
    ("snare", SampleMapKind::Snare),
    ("clap", SampleMapKind::Clap),
    ("hihat", SampleMapKind::Hat),
    ("crash", SampleMapKind::Crash),
    ("cymbal", SampleMapKind::Cymbal),
    ("shaker", SampleMapKind::Percussion),
    ("conga", SampleMapKind::Percussion),
    ("bongo", SampleMapKind::Percussion),
    ("cowbell", SampleMapKind::Percussion),
    ("bass", SampleMapKind::Bass),
    ("synth", SampleMapKind::Tonal),
    ("chord", SampleMapKind::Tonal),
    ("pluck", SampleMapKind::Tonal),
    ("vocal", SampleMapKind::Vocal),
    ("voice", SampleMapKind::Vocal),
    ("riser", SampleMapKind::Effect),
    ("sweep", SampleMapKind::Effect),
    ("impact", SampleMapKind::Effect),
];

/// Music-style words: "bass" beside one of these in a folder name names no
/// kind (Bass House Vol 2, Drum and Bass).
const STYLE_WORDS: &[&str] = &[
    "house", "music", "future", "drum", "dnb", "uk", "and", "n", "jungle", "trap",
];

/// Split a name into lowercase words at spaces, punctuation, case changes
/// and letter/digit boundaries: "KickHard_02" → kick, hard, 02.
pub fn words_of(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev: Option<char> = None;
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            let lower = ch.to_lowercase().next().unwrap_or(ch);
            let boundary = match prev {
                Some(p) => {
                    (p.is_alphabetic() && ch.is_numeric())
                        || (p.is_numeric() && ch.is_alphabetic())
                        || (p.is_lowercase() && ch.is_uppercase())
                }
                None => false,
            };
            if boundary && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(lower);
            prev = Some(ch);
        } else {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            prev = None;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// A word with a trailing "s" also counts as the word without it when at
/// least three letters remain (Kicks → kick, Hats → hat).
fn singular(word: &str) -> Option<&str> {
    let stripped = word.strip_suffix('s')?;
    (stripped.chars().count() >= 3).then_some(stripped)
}

/// The kind of a word list (already lowercased words): two-word phrases
/// first, then single words (with their singulars), then prefixes.
fn kind_of_words(words: &[String], allow_prefixes: bool) -> Option<SampleMapKind> {
    let table = word_table();
    // Two-word phrases before single words.
    for pair in table.iter().filter(|(phrase, _)| phrase.len() == 2) {
        for window in words.windows(2) {
            if window[0] == pair.0[0] && window[1] == pair.0[1] {
                return Some(pair.1);
            }
        }
    }
    let mut best: Option<SampleMapKind> = None;
    for word in words {
        let mut candidates = vec![word.as_str()];
        if let Some(s) = singular(word) {
            candidates.push(s);
        }
        for candidate in candidates {
            for (phrase, kind) in table.iter().filter(|(phrase, _)| phrase.len() == 1) {
                if candidate == phrase[0] {
                    best = match best {
                        Some(current) if current.rank() <= kind.rank() => Some(current),
                        _ => Some(*kind),
                    };
                }
            }
            if allow_prefixes {
                for (prefix, kind) in PREFIX_WORDS {
                    if word.len() > prefix.len() && word.starts_with(prefix) {
                        best = match best {
                            Some(current) if current.rank() <= kind.rank() => Some(current),
                            _ => Some(*kind),
                        };
                    }
                }
            }
        }
    }
    best
}

/// The kind a file name says its sample is: the file name first, then its
/// two nearest folder names below the library root.
pub fn sample_kind_of(file_name: &str, folder_names: &[&str]) -> SampleMapKind {
    // The extension is not a word.
    let stem = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    if let Some(kind) = kind_of_words(&words_of(stem), true) {
        return kind;
    }
    for folder in folder_names.iter().take(2) {
        let words = words_of(folder);
        // "bass" beside a style word in a folder name names no kind.
        if words.iter().any(|w| w == "bass")
            && words.iter().any(|w| STYLE_WORDS.contains(&w.as_str()))
        {
            let filtered: Vec<String> = words
                .iter()
                .filter(|w| w.as_str() != "bass")
                .cloned()
                .collect();
            if let Some(kind) = kind_of_words(&filtered, false) {
                return kind;
            }
            continue;
        }
        if let Some(kind) = kind_of_words(&words, false) {
            return kind;
        }
    }
    SampleMapKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split_at_punctuation_case_and_digit_boundaries() {
        assert_eq!(words_of("KickHard_02"), vec!["kick", "hard", "02"]);
        assert_eq!(words_of("CLOSED-HAT_v2"), vec!["closed", "hat", "v", "2"]);
        assert_eq!(words_of("kick1"), vec!["kick", "1"]);
    }

    #[test]
    fn whole_words_and_singulars_give_the_kind() {
        assert_eq!(sample_kind_of("kick.wav", &[]), SampleMapKind::Kick);
        assert_eq!(sample_kind_of("Kicks.wav", &[]), SampleMapKind::Kick);
        assert_eq!(sample_kind_of("snare_rim.wav", &[]), SampleMapKind::Snare);
        assert_eq!(sample_kind_of("hh.wav", &[]), SampleMapKind::Hat);
        assert_eq!(sample_kind_of("808.wav", &[]), SampleMapKind::Bass);
        assert_eq!(sample_kind_of("vox chop.wav", &[]), SampleMapKind::Vocal);
        assert_eq!(sample_kind_of("riser.wav", &[]), SampleMapKind::Effect);
        assert_eq!(sample_kind_of("mystery.wav", &[]), SampleMapKind::Other);
    }

    #[test]
    fn two_word_phrases_beat_single_words_and_prefixes_extend() {
        assert_eq!(
            sample_kind_of("closed hat 3.wav", &[]),
            SampleMapKind::ClosedHat
        );
        assert_eq!(sample_kind_of("open hat.wav", &[]), SampleMapKind::OpenHat);
        assert_eq!(
            sample_kind_of("hat closed.wav", &[]),
            SampleMapKind::ClosedHat
        );
        assert_eq!(sample_kind_of("Kickdrum.wav", &[]), SampleMapKind::Kick);
        assert_eq!(sample_kind_of("Bassline.wav", &[]), SampleMapKind::Bass);
    }

    #[test]
    fn the_most_specific_kind_wins() {
        // "808 Kick" is a kick, not a bass.
        assert_eq!(sample_kind_of("808 kick.wav", &[]), SampleMapKind::Kick);
        // "Snare FX" is a snare, not an effect.
        assert_eq!(sample_kind_of("snare fx.wav", &[]), SampleMapKind::Snare);
        // Closed hat beats the plain hat word.
        assert_eq!(
            sample_kind_of("chh open.wav", &[]),
            SampleMapKind::ClosedHat
        );
        // A drum beats hat: "hat crash" is a crash? crash(7) beats hat(9).
        assert_eq!(sample_kind_of("ride hat.wav", &[]), SampleMapKind::Ride);
    }

    #[test]
    fn folders_give_the_kind_when_the_name_does_not_and_bass_style_is_ignored() {
        assert_eq!(
            sample_kind_of("one.wav", &["Kicks", "Drums"]),
            SampleMapKind::Kick
        );
        assert_eq!(
            sample_kind_of("sample.wav", &["Percussion"]),
            SampleMapKind::Percussion
        );
        // "Bass House Vol 2" names no kind.
        assert_eq!(
            sample_kind_of("loop.wav", &["Bass House Vol 2"]),
            SampleMapKind::Other
        );
        // The file name wins over the folders.
        assert_eq!(
            sample_kind_of("snare.wav", &["Kicks"]),
            SampleMapKind::Snare
        );
    }
}
