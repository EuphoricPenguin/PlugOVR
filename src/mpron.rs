/// Moby Pronunciator II (mpron.txt) parser for the OddVoices synthesizer.
///
/// At compile time, `build.rs` parses `bin/mpron.txt` (Windows-1252 encoded)
/// and generates `dict_data.rs` — a sorted `&[(&str, &[&str])]` array of
/// (word, phonemes) pairs. At runtime, binary search provides O(log n)
/// lookups with **zero allocation** and sub-microsecond latency.
///
/// The old file-loading and HashMap-building functions are kept behind
/// `#[cfg(test)]` for test compatibility.
///
/// Key format details:
/// - Phonemes are enclosed in /slashes/, e.g. /eI/, /&/, /@/, /tS/
/// - Adjacent slash-delimited phonemes share a single / delimiter: /T/I/N/
///   means /T/, /I/, /N/ (three separate phonemes)
/// - Double slashes //Oi// are used for the OI diphthong to avoid ambiguity
/// - Single-char phonemes may appear bare (outside slashes): b, d, k, @
/// - Stress markers: ' (primary), , (secondary) — bare, always stripped
/// - Underscore _ separates words in compound pronunciations

use std::collections::HashMap;

// Auto-generated sorted dictionary array (build.rs)
include!("dict_data.rs");

/// Look up a word in the compile-time embedded dictionary.
///
/// Uses binary search over the sorted `DICT_ENTRIES` array.
/// Returns `Some(&[&str])` with the X-SAMPA phoneme sequence, or `None` if
/// the word is not in the dictionary.
///
/// O(log n) — sub-microsecond lookup, zero allocation.
#[inline]
pub fn lookup_static(word: &str) -> Option<&'static [&'static str]> {
    let idx = DICT_ENTRIES.binary_search_by(|(w, _)| w.cmp(&word));
    match idx {
        Ok(i) => Some(DICT_ENTRIES[i].1),
        Err(_) => None,
    }
}

/// Mapping from naked Moby phoneme identifiers (without slashes) to X-SAMPA phonemes.
const NAKED_TO_XSAMPA: &[(&str, &str)] = &[
    // Multi-character identifiers
    ("Oi", "OI"),   // ɔɪ  (used in //Oi//)
    ("aU", "aU"),   // aʊ
    ("aI", "aI"),   // aɪ
    ("eI", "eI"),   // eɪ
    ("oU", "oU"),   // oʊ
    ("ju", "ju"),   // juː
    ("tS", "tS"),   // tʃ
    ("dZ", "dZ"),   // dʒ
    ("[@]", "@"),   // ə  — alternative bracket notation used in mpron.txt
    // Single-character identifiers
    ("x", "x"),     // x (velar fricative)
    ("y", "y"),     // ø
    ("&", "{}"),    // æ  (ash)
    ("-", "@"),     // ə  (schwa, hyphen form)
    ("@", "@"),     // ə  (schwa, bare form)
    ("A", "A"),     // ɑ
    ("D", "D"),     // ð
    ("E", "E"),     // ɛ
    ("I", "I"),     // ɪ
    ("N", "N"),     // ŋ
    ("O", "O"),     // ɔ
    ("S", "S"),     // ʃ
    ("T", "T"),     // θ
    ("U", "U"),     // ʊ
    ("i", "i"),     // iː
    ("j", "j"),     // j
    ("u", "u"),     // uː
    ("b", "b"),
    ("d", "d"),
    ("f", "f"),
    ("g", "g"),
    ("h", "h"),
    ("k", "k"),
    ("l", "l"),
    ("m", "m"),
    ("n", "n"),
    ("p", "p"),
    ("r", "r"),
    ("s", "s"),
    ("t", "t"),
    ("v", "v"),
    ("w", "w"),
    ("z", "z"),
];

/// Look up a naked phoneme identifier in the X-SAMPA mapping table.
fn lookup_phoneme(phoneme_id: &str) -> Option<&'static str> {
    for &(key, xsampa) in NAKED_TO_XSAMPA {
        if key == phoneme_id {
            return Some(xsampa);
        }
    }
    None
}

/// Split accumulated slash-group content into individual phoneme identifiers
/// using greedy multi-char matching (longest identifier first).
///
/// Stress markers (' and ,) within the content are stripped before matching.
fn split_slash_content(content: &str) -> Vec<String> {
    let cleaned: String = content
        .chars()
        .filter(|c| *c != '\'' && *c != ',')
        .collect();

    let mut result = Vec::new();
    let mut remaining = cleaned.as_str();

    // Build sorted list of identifiers, longest first
    let mut ids: Vec<&str> = NAKED_TO_XSAMPA.iter().map(|(k, _)| *k).collect();
    ids.sort_by(|a, b| b.len().cmp(&a.len()));

    while !remaining.is_empty() {
        let mut found = false;
        for &id in &ids {
            if remaining.starts_with(id) {
                if let Some(xsampa) = lookup_phoneme(id) {
                    result.push(xsampa.to_string());
                }
                remaining = &remaining[id.len()..];
                found = true;
                break;
            }
        }
        if !found {
            // Skip unrecognized character
            if let Some(ch) = remaining.chars().next() {
                remaining = &remaining[ch.len_utf8()..];
            } else {
                break;
            }
        }
    }

    result
}

/// Build a dictionary HashMap from raw mpron bytes (Windows-1252 encoded).
#[cfg(test)]
pub fn build_dictionary_from_bytes(data: &[u8]) -> HashMap<String, Vec<String>> {
    let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(data);
    build_dictionary(&decoded)
}

/// Build a dictionary HashMap from the raw mpron text content.
///
/// Used by tests and the build script.
pub fn build_dictionary(content: &str) -> HashMap<String, Vec<String>> {
    let mut dict = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(";;;") {
            continue;
        }

        let space_pos = match line.find(' ') {
            Some(p) => p,
            None => continue,
        };

        let word_part = &line[..space_pos];
        let pron_part = line[space_pos + 1..].trim();

        let word = if let Some(slash_pos) = word_part.find('/') {
            word_part[..slash_pos].to_lowercase()
        } else {
            word_part.to_lowercase()
        };

        let phonemes = parse_pronunciation(pron_part);

        if phonemes.is_empty() {
            continue;
        }

        dict.insert(word, phonemes);
    }

    dict
}

/// Load the Moby Pronunciator II dictionary from a file on disk (tests only).
#[cfg(test)]
pub fn load_dictionary(path: &str) -> HashMap<String, Vec<String>> {
    use std::fs::File;
    use std::io::Read;
    let mut raw = Vec::new();
    let content: String = match File::open(path) {
        Ok(mut f) => {
            f.read_to_end(&mut raw).unwrap_or_default();
            let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&raw);
            decoded.to_string()
        }
        Err(e) => {
            eprintln!("Warning: Could not open mpron file '{}': {}", path, e);
            return HashMap::new();
        }
    };
    build_dictionary(&content)
}

/// Parse an mpron pronunciation string into X-SAMPA phonemes.
pub fn parse_pronunciation(pron: &str) -> Vec<String> {
    let mut result = Vec::new();
    let chars: Vec<char> = pron.chars().collect();
    let mut i = 0;
    let mut in_slash = false;
    let mut current = String::new();

    fn is_bare_marker(ch: char) -> bool {
        ch == '\'' || ch == ',' || ch == '_'
    }

    while i < chars.len() {
        let ch = chars[i];

        if ch == '/' {
            if in_slash {
                if !current.is_empty() {
                    let phonemes = split_slash_content(&current);
                    result.extend(phonemes);
                    current.clear();
                }
                i += 1;
                if i >= chars.len() || is_bare_marker(chars[i]) {
                    in_slash = false;
                }
            } else {
                in_slash = true;
                current.clear();
                i += 1;
            }
        } else if in_slash {
            current.push(ch);
            i += 1;
        } else {
            match ch {
                '\'' | ',' => {}
                '_' => result.push("_".to_string()),
                _ => {
                    let ch_str = ch.to_string();
                    if let Some(xsampa) = lookup_phoneme(&ch_str) {
                        result.push(xsampa.to_string());
                    }
                }
            }
            i += 1;
        }
    }

    if in_slash && !current.is_empty() {
        let phonemes = split_slash_content(&current);
        result.extend(phonemes);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_static_found() {
        // "hello" should be in the embedded dictionary
        let phonemes = lookup_static("hello");
        assert!(phonemes.is_some(), "hello should be in the dictionary");
    }

    #[test]
    fn test_lookup_static_not_found() {
        let phonemes = lookup_static("xyznonexistentword999");
        assert!(phonemes.is_none());
    }

    #[test]
    fn test_lookup_static_common_words() {
        // Test several common words
        assert!(lookup_static("the").is_some());
        assert!(lookup_static("a").is_some());
        assert!(lookup_static("world").is_some());
    }

    #[test]
    fn test_parse_simple_word() {
        let phonemes = parse_pronunciation("/h/@/'l/oU/");
        assert_eq!(phonemes, vec!["h", "@", "l", "oU"]);
    }

    #[test]
    fn test_parse_with_stress() {
        let phonemes = parse_pronunciation("/@/'b/aU/t");
        assert_eq!(phonemes, vec!["@", "b", "aU", "t"]);
    }

    #[test]
    fn test_parse_diphthongs() {
        let phonemes = parse_pronunciation("/v//Oi//s");
        assert_eq!(phonemes, vec!["v", "OI", "s"]);
    }

    #[test]
    fn test_parse_word_separator() {
        let phonemes = parse_pronunciation(",&/b/@/'l/oU/n/i/_/S//E/l");
        assert_eq!(
            phonemes,
            vec!["{}", "b", "@", "l", "oU", "n", "i", "_", "S", "E", "l"]
        );
    }

    #[test]
    fn test_parse_ae() {
        let phonemes = parse_pronunciation("/k/&/t");
        assert_eq!(phonemes, vec!["k", "{}", "t"]);
    }

    #[test]
    fn test_parse_shared_slash_convention() {
        let phonemes = parse_pronunciation("/T/I/N/k");
        assert_eq!(phonemes, vec!["T", "I", "N", "k"]);
    }

    #[test]
    fn test_parse_bare_at() {
        let phonemes = parse_pronunciation("/h/@/'l/oU/");
        assert_eq!(phonemes, vec!["h", "@", "l", "oU"]);
    }

    #[test]
    fn test_parse_mixed_bare_and_slashed() {
        let phonemes = parse_pronunciation("'/A/rd,v/A/rk");
        assert_eq!(phonemes, vec!["A", "r", "d", "v", "A", "r", "k"]);
    }

    #[test]
    fn test_parse_exclamation() {
        let phonemes = parse_pronunciation("/!/");
        assert!(phonemes.is_empty());
    }

    #[test]
    fn test_build_dictionary_from_str() {
        let content = "hello /h/@/'l/oU/\n";
        let dict = build_dictionary(content);
        assert_eq!(
            dict.get("hello"),
            Some(&vec!["h".to_string(), "@".to_string(), "l".to_string(), "oU".to_string()])
        );
    }

    #[test]
    fn test_load_dictionary_empty_file() {
        use tempfile::NamedTempFile;
        let tmp = NamedTempFile::new().unwrap();
        let dict = load_dictionary(tmp.path().to_str().unwrap());
        assert!(dict.is_empty());
    }

    #[test]
    fn test_load_dictionary_with_entries() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        let mut f = tmp.as_file();
        f.write_all(b"hello /h/@/'l/oU/\n").unwrap();
        f.write_all(b"world /w/@/r/ld\n").unwrap();

        let dict = load_dictionary(tmp.path().to_str().unwrap());

        assert_eq!(
            dict.get("hello"),
            Some(&vec!["h".to_string(), "@".to_string(), "l".to_string(), "oU".to_string()])
        );
    }

    #[test]
    fn test_load_dictionary_with_pos() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        let mut f = tmp.as_file();
        f.write_all(b"record/n /r/@/'k/O/rd\n").unwrap();
        f.write_all(b"record/v /r/I/'k/O/rd\n").unwrap();

        let dict = load_dictionary(tmp.path().to_str().unwrap());
        assert!(dict.contains_key("record"));
    }
}