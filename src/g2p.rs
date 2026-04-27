//! Grapheme-to-Phoneme (G2P) conversion for the OddVoices synthesizer.
//!
//! Uses the Moby Pronunciator II (mpron.txt) dictionary to convert
//! English text into X-SAMPA phoneme sequences suitable for the
//! OddVoices PSOLA synthesizer.

use std::collections::HashMap;

use crate::mpron;

/// Vowels (used by VV fixer and is_vowel checks).
fn vowels() -> Vec<&'static str> {
    vec![
        "{}", "@`", "A", "I", "E", "@", "u", "U", "i",
        "oU", "eI", "aI", "OI", "aU",
    ]
}

/// Vowel-to-vowel fixer: insert a glide consonant between adjacent vowels.
fn vv_fixers() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("i", "j");
    m.insert("aI", "j");
    m.insert("eI", "j");
    m.insert("OI", "j");
    m.insert("u", "w");
    m.insert("aU", "w");
    m.insert("oU", "w");
    m.insert("@`", "r");
    m
}

/// Phoneme aliases (alternative symbols that map to canonical phonemes).
fn phoneme_aliases() -> HashMap<&'static str, Vec<&'static str>> {
    let mut m = HashMap::new();
    m.insert("V", vec!["@"]);
    m.insert("3`", vec!["@`"]);
    m.insert("O", vec!["A"]);
    m.insert("&", vec!["{}"]);
    m.insert("{", vec!["{}"]);
    m.insert("Or", vec!["oU", "r"]);
    m.insert("?", vec!["_"]);
    m.insert(" ", vec!["_"]);
    m
}

/// Guess pronunciations for out-of-vocabulary words.
fn guess_pronunciations() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut pairs = vec![
        ("a", vec!["{}"]),
        ("ai", vec!["aI"]),
        ("ar", vec!["A", "r"]),
        ("au", vec!["aU"]),
        ("augh", vec!["A"]),
        ("b", vec!["b"]),
        ("c", vec!["k"]),
        ("ch", vec!["tS"]),
        ("d", vec!["d"]),
        ("e", vec!["E"]),
        ("ei", vec!["eI"]),
        ("ee", vec!["i"]),
        ("ea", vec!["i"]),
        ("er", vec!["@`"]),
        ("f", vec!["f"]),
        ("g", vec!["g"]),
        ("h", vec!["h"]),
        ("i", vec!["I"]),
        ("ie", vec!["aI"]),
        ("igh", vec!["aI"]),
        ("j", vec!["dZ"]),
        ("k", vec!["k"]),
        ("l", vec!["l"]),
        ("m", vec!["m"]),
        ("n", vec!["n"]),
        ("ng", vec!["N"]),
        ("o", vec!["oU"]),
        ("oi", vec!["OI"]),
        ("oo", vec!["u"]),
        ("ou", vec!["aU"]),
        ("ough", vec!["A"]),
        ("ow", vec!["aU"]),
        ("p", vec!["p"]),
        ("q", vec!["k", "w"]),
        ("r", vec!["r"]),
        ("s", vec!["s"]),
        ("sh", vec!["S"]),
        ("t", vec!["t"]),
        ("th", vec!["T"]),
        ("u", vec!["@"]),
        ("ur", vec!["@`"]),
        ("v", vec!["v"]),
        ("w", vec!["w"]),
        ("x", vec!["k", "s"]),
        ("y", vec!["j"]),
        ("y$", vec!["i"]),
        ("z", vec!["z"]),
    ];
    // Sort by key length descending (longest first for greedy matching)
    pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    pairs
}

/// Dictionary exceptions (hard-coded overrides).
fn dictionary_exceptions() -> HashMap<&'static str, Vec<&'static str>> {
    let mut m = HashMap::new();
    m.insert("and", vec!["{}", "n", "d"]);
    m.insert("every", vec!["E", "v", "r", "i"]);
    m.insert("oddvoices", vec!["A", "d", "v", "OI", "s", "E", "z"]);
    m.insert("chesnokov", vec!["tS", "E", "z", "n", "oU", "k", "A", "v"]);
    m
}

/// Perform the cot-caught merger: /O/ -> /A/ (or /oU/ before /r/).
fn perform_cot_caught_merger(pronunciation: &mut Vec<String>) {
    let len = pronunciation.len();
    for i in 0..len {
        if pronunciation[i] == "O" {
            if i + 1 < len && pronunciation[i + 1] == "r" {
                pronunciation[i] = "oU".to_string();
            } else {
                pronunciation[i] = "A".to_string();
            }
        }
    }
}

/// Fix vowel-vowel diphones by inserting glide consonants.
fn fix_vv_diphones(pronunciation: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let vowels = vowels();
    let fixers = vv_fixers();
    let mut last_phoneme = String::new();
    for phoneme in pronunciation {
        if vowels.contains(&phoneme.as_str()) && fixers.contains_key(last_phoneme.as_str()) {
            result.push(fixers[last_phoneme.as_str()].to_string());
        }
        result.push(phoneme.clone());
        last_phoneme = phoneme.clone();
    }
    result
}

/// Normalize pronunciation by adding leading/trailing silence markers.
fn normalize_pronunciation(pronunciation: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    if pronunciation.is_empty() {
        return vec!["_".to_string()];
    }
    if pronunciation[0] != "_" {
        result.push("_".to_string());
    }
    let last = pronunciation.last().map(|s| s.to_string());
    result.extend(pronunciation);
    if last.as_deref() != Some("_") {
        result.push("_".to_string());
    }
    result
}

/// Guess pronunciation for out-of-vocabulary words using heuristic rules.
fn pronounce_oov(word: &str) -> Vec<String> {
    let guesses = guess_pronunciations();
    let mut remaining = format!("{}", word);
    remaining.push('$');
    let mut pass1 = Vec::new();
    while !remaining.is_empty() {
        let mut found = false;
        for &(key, ref phonemes) in &guesses {
            if remaining.len() >= key.len() && &remaining[..key.len()] == key {
                remaining = remaining[key.len()..].to_string();
                pass1.extend(phonemes.iter().map(|s| s.to_string()));
                found = true;
                break;
            }
        }
        if !found {
            remaining = remaining[1..].to_string();
        }
    }
    // Remove consecutive duplicates
    let mut pass2 = Vec::new();
    let mut last: Option<String> = None;
    for ph in pass1 {
        if last.as_deref() != Some(&ph) {
            pass2.push(ph.clone());
            last = Some(ph);
        }
    }
    pass2
}

/// Tokenize text into words, handling punctuation and explicit phonetic input.
fn tokenize(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    for island in text.split_whitespace() {
        if island.starts_with('/') && island.ends_with('/') {
            // Explicit phonetic input between slashes
            words.push(island.to_string());
        } else {
            let mut current = String::new();
            for c in island.chars().map(|c| c.to_ascii_lowercase()) {
                if (c >= 'a' && c <= 'z') || c == '\'' {
                    current.push(c);
                } else {
                    if !current.is_empty() {
                        words.push(current.clone());
                        current.clear();
                    }
                }
            }
            if !current.is_empty() {
                words.push(current);
            }
        }
    }
    words
}

/// Parse a pronunciation string (e.g. from /slashes/) into phonemes.
fn parse_pronunciation(pronunciation: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut remaining = pronunciation;
    let aliases = phoneme_aliases();

    // Build list of all known phonemes (from aliases + common ones)
    let mut all_phonemes: Vec<&str> = aliases.keys().cloned().collect();
    all_phonemes.extend(vowels());
    all_phonemes.extend(vec![
        "tS", "dZ", "@`", "l", "r", "j", "w",
        "m", "n", "N", "h", "k", "g", "p", "b", "t", "d",
        "f", "v", "T", "D", "s", "z", "S", "Z",
        "_",
    ]);
    // Sort by length descending for greedy matching
    all_phonemes.sort_by(|a, b| b.len().cmp(&a.len()));
    all_phonemes.dedup();

    while !remaining.is_empty() {
        let mut found = false;
        for &ph in &all_phonemes {
            if remaining.len() >= ph.len() && &remaining[..ph.len()] == ph {
                if let Some(alias_phonemes) = aliases.get(ph) {
                    result.extend(alias_phonemes.iter().map(|s| s.to_string()));
                } else {
                    result.push(ph.to_string());
                }
                remaining = &remaining[ph.len()..];
                found = true;
                break;
            }
        }
        if !found {
            // Skip unrecognized character
            remaining = &remaining[1..];
        }
    }
    result
}

/// Full G2P wrapper around the mpron dictionary.
pub struct G2P {
    dict: HashMap<String, Vec<String>>,
}

impl G2P {
    /// Create a new G2P instance from an mpron dictionary.
    ///
    /// The dictionary maps lowercase words to X-SAMPA phoneme sequences.
    pub fn new(dict: HashMap<String, Vec<String>>) -> Self {
        // Apply dictionary exceptions
        let mut dict = dict;
        for (word, phonemes) in dictionary_exceptions() {
            dict.insert(word.to_string(), phonemes.iter().map(|s| s.to_string()).collect());
        }
        G2P { dict }
    }

    /// Load the mpron dictionary from a file path.
    pub fn load(path: &str) -> Self {
        let dict = mpron::load_dictionary(path);
        G2P::new(dict)
    }

    /// Pronounce a single word, returning X-SAMPA phonemes.
    pub fn pronounce_word(&self, word: &str) -> Vec<String> {
        let result = if word.starts_with('/') {
            // Explicit phonetic input: /phonemes/
            let inner = &word[1..word.len().saturating_sub(1)];
            parse_pronunciation(inner)
        } else if let Some(phonemes) = self.dict.get(word) {
            // Look up in mpron dictionary (already in X-SAMPA)
            let mut result = phonemes.clone();
            perform_cot_caught_merger(&mut result);
            result
        } else {
            // Out of vocabulary: guess
            pronounce_oov(word)
        };
        let result = fix_vv_diphones(&result);
        normalize_pronunciation(result)
    }

    /// Pronounce a full text string (multiple words).
    pub fn pronounce(&self, text: &str) -> Vec<String> {
        let words = tokenize(text);
        let mut result = Vec::new();
        for word in words {
            let pronunciation = self.pronounce_word(&word);
            result.extend(pronunciation);
        }
        result
    }

    /// Get the number of entries in the dictionary.
    pub fn num_entries(&self) -> usize {
        self.dict.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple() {
        let words = tokenize("hello world");
        assert_eq!(words, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_with_punctuation() {
        let words = tokenize("hello, world!");
        assert_eq!(words, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_phonetic() {
        let words = tokenize("say /h @ l oU/");
        assert_eq!(words, vec!["say", "/h @ l oU/"]);
    }

    #[test]
    fn test_pronounce_oov_simple() {
        let phonemes = pronounce_oov("test");
        assert!(!phonemes.is_empty());
    }

    #[test]
    fn test_normalize_pronunciation() {
        let result = normalize_pronunciation(vec!["h".to_string(), "i".to_string()]);
        assert_eq!(result[0], "_");
        assert_eq!(result.last().unwrap(), "_");
    }

    #[test]
    fn test_fix_vv_diphones() {
        // "i" followed by vowel should insert "j"
        let result = fix_vv_diphones(&["i".to_string(), "A".to_string()]);
        assert_eq!(result, vec!["i", "j", "A"]);
    }

    #[test]
    fn test_g2p_with_mpron() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        let mut f = tmp.as_file();
        f.write_all(b"hello /h/@/'l/oU/\n").unwrap();
        f.write_all(b"world /w/@/r/ld\n").unwrap();

        let g2p = G2P::load(tmp.path().to_str().unwrap());
        assert!(g2p.num_entries() >= 2);

        let phonemes = g2p.pronounce("hello world");
        assert!(!phonemes.is_empty());
        // Should start and end with silence
        assert_eq!(phonemes[0], "_");
        assert_eq!(phonemes.last().unwrap(), "_");
    }

    #[test]
    fn test_g2p_exception() {
        let g2p = G2P::new(HashMap::new());
        let phonemes = g2p.pronounce("and");
        assert!(!phonemes.is_empty());
    }

    #[test]
    fn test_g2p_phonetic_input() {
        let g2p = G2P::new(HashMap::new());
        let phonemes = g2p.pronounce("/h @ l oU/");
        assert!(!phonemes.is_empty());
    }
}
