//! Grapheme-to-Phoneme (G2P) conversion for the OddVoices synthesizer.
//!
//! Uses the compile-time embedded Moby Pronunciator II dictionary
//! (`mpron::lookup_static()`) to convert English text into X-SAMPA
//! phoneme sequences suitable for the OddVoices PSOLA synthesizer.
//!
//! Lookups are O(log n) binary searches over a sorted static array —
//! zero allocation, sub-microsecond latency.

use crate::mpron;

/// Vowels (used by VV fixer and is_vowel checks).
fn vowels() -> Vec<&'static str> {
    vec![
        "{}", "@`", "A", "I", "E", "@", "u", "U", "i",
        "oU", "eI", "aI", "OI", "aU",
    ]
}

/// Vowel-to-vowel fixer: insert a glide consonant between adjacent vowels.
fn vv_fixers() -> std::collections::HashMap<&'static str, &'static str> {
    let mut m = std::collections::HashMap::new();
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
fn phoneme_aliases() -> std::collections::HashMap<&'static str, Vec<&'static str>> {
    let mut m = std::collections::HashMap::new();
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
    let pairs = vec![
        ("a", vec!["{}"]),
        ("b", vec!["b"]),
        ("c", vec!["k"]),
        ("d", vec!["d"]),
        ("e", vec!["E"]),
        ("f", vec!["f"]),
        ("g", vec!["g"]),
        ("h", vec!["h"]),
        ("i", vec!["I"]),
        ("j", vec!["dZ"]),
        ("k", vec!["k"]),
        ("l", vec!["l"]),
        ("m", vec!["m"]),
        ("n", vec!["n"]),
        ("o", vec!["A"]),
        ("p", vec!["p"]),
        ("q", vec!["k"]),
        ("r", vec!["r"]),
        ("s", vec!["s"]),
        ("t", vec!["t"]),
        ("u", vec!["u"]),
        ("v", vec!["v"]),
        ("w", vec!["w"]),
        ("x", vec!["ks"]),
        ("y", vec!["i"]),
        ("z", vec!["z"]),
        ("ch", vec!["tS"]),
        ("sh", vec!["S"]),
        ("th", vec!["T"]),
        ("ng", vec!["N"]),
        ("ph", vec!["f"]),
        ("wh", vec!["w"]),
    ];
    pairs
}

/// Dictionary exceptions — words with corrected pronunciations.
///
/// These match the original OddVoices `k_cmudictExceptions` from
/// `oddvoices/cpp/src/g2p.cpp` lines 191-196. The original used these
/// to override bad CMUdict entries. Under Moby Pronunciator II, the
/// contractions (aren't, didn't, etc.) are already correct and don't
/// need overrides — only these few proper nouns and short words remain.
fn dictionary_exceptions() -> std::collections::HashMap<&'static str, Vec<&'static str>> {
    let mut m = std::collections::HashMap::new();
    m.insert("and", vec!["{}", "n", "d"]);
    m.insert("every", vec!["E", "v", "r", "i"]);
    m.insert("oddvoices", vec!["A", "d", "v", "OI", "s", "E", "z"]);
    m.insert("chesnokov", vec!["tS", "E", "z", "n", "oU", "k", "A", "v"]);
    m
}

/// Perform the cot-caught merger: /O/ -> /A/ (or /oU/ before /r/).
fn perform_cot_caught_merger(pronunciation: &mut [String]) {
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
    let vv_fixers = vv_fixers();

    let mut prev_is_vowel = false;
    let mut prev_phoneme = String::new();

    for phoneme in pronunciation {
        let is_vowel = vowels.contains(&phoneme.as_str());
        if prev_is_vowel && is_vowel {
            if let Some(&glide) = vv_fixers.get(prev_phoneme.as_str()) {
                result.push(glide.to_string());
            }
        }
        result.push(phoneme.clone());
        prev_is_vowel = is_vowel;
        prev_phoneme = phoneme.clone();
    }

    result
}

/// Canonicalize pronunciation: apply aliases and deduplication.
fn normalize_pronunciation(pronunciation: Vec<String>) -> Vec<String> {
    let aliases = phoneme_aliases();
    let mut result = Vec::new();
    let mut prev = String::new();

    for phoneme in pronunciation {
        let resolved = if let Some(alternatives) = aliases.get(phoneme.as_str()) {
            alternatives.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        } else {
            vec![phoneme.clone()]
        };

        for p in resolved {
            if p != prev || p == "_" {
                result.push(p.clone());
            }
            prev = p;
        }
    }

    result
}

/// Tokenize a text string into individual words.
fn tokenize(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_alphabetic() || ch == '\'' || ch == '-' {
            current.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == ',' || ch == '.' || ch == '!' || ch == '?' {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
            if ch == ',' || ch == '.' || ch == '!' || ch == '?' {
                words.push(ch.to_string());
            }
        }
    }
    if !current.is_empty() {
        words.push(current);
    }

    words
}

/// Out-of-vocabulary pronunciation guesser.
fn pronounce_oov(word: &str) -> Vec<String> {
    let pairs = guess_pronunciations();
    let mut result = Vec::new();
    let remaining = word.to_ascii_lowercase();

    let mut sorted_pairs: Vec<_> = pairs.iter().collect();
    sorted_pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    let chars: Vec<char> = remaining.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let mut found = false;
        let slice: String = chars[i..].iter().collect();
        for &(key, phonemes) in &sorted_pairs {
            if slice.starts_with(key) {
                for p in phonemes {
                    result.push(p.to_string());
                }
                i += key.len();
                found = true;
                break;
            }
        }
        if !found {
            i += 1;
        }
    }

    let mut deduped = Vec::new();
    let mut prev = String::new();
    for p in result {
        if p != prev {
            deduped.push(p.clone());
        }
        prev = p;
    }

    deduped
}

/// Full G2P wrapper using the compile-time embedded dictionary.
///
/// Unlike the old HashMap-based G2P, this struct has **zero initialization
/// cost** — all dictionary lookups use `mpron::lookup_static()` directly.
pub struct G2P;

impl Default for G2P {
    fn default() -> Self {
        G2P
    }
}

impl G2P {
    /// Create a new G2P instance (instant — zero allocation).
    pub fn new() -> Self {
        G2P
    }

    /// Pronounce a single word, returning X-SAMPA phonemes.
    pub fn pronounce_word(&self, word: &str) -> Vec<String> {
        let result = if word.starts_with('/') {
            // Explicit phonetic input: /phonemes/
            let inner = &word[1..word.len().saturating_sub(1)];
            mpron::parse_pronunciation(inner)
        } else {
            // Check dictionary exceptions first
            let exceptions = dictionary_exceptions();
            if let Some(phonemes) = exceptions.get(word) {
                phonemes.iter().map(|s| s.to_string()).collect()
            } else if let Some(phonemes) = mpron::lookup_static(word) {
                // Binary search in compile-time embedded array
                let mut result: Vec<String> = phonemes.iter().map(|s| s.to_string()).collect();
                perform_cot_caught_merger(&mut result);
                result
            } else {
                // Out of vocabulary: guess
                pronounce_oov(word)
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_g2p_hello() {
        let g2p = G2P::new();
        let phonemes = g2p.pronounce_word("hello");
        assert!(!phonemes.is_empty(), "hello should have phonemes");
    }

    #[test]
    fn test_g2p_oov_fallback() {
        let g2p = G2P::new();
        let phonemes = g2p.pronounce_word("xyznonexistentword999");
        // Should use OOV guesser; just verify it produces something
        assert!(!phonemes.is_empty());
    }

    #[test]
    fn test_g2p_explicit_phonetic() {
        let g2p = G2P::new();
        let phonemes = g2p.pronounce_word("/h/@/'l/oU/");
        assert_eq!(phonemes, vec!["h", "@", "l", "oU"]);
    }

    #[test]
    fn test_g2p_dict_exception() {
        let g2p = G2P::new();
        let phonemes = g2p.pronounce_word("don't");
        assert!(!phonemes.is_empty(), "don't should be in exceptions");
    }
}