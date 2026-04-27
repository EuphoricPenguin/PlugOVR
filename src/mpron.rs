/// Moby Pronunciator II (mpron.txt) parser for the OddVoices synthesizer.
///
/// Loads the mpron.txt file and provides word-to-phoneme conversion.
/// The mpron format uses IPA-like symbols (some delimited by /) to represent
/// pronunciations. We map these to X-SAMPA phonemes used by the voice segments.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

/// Mapping from mpron symbols to X-SAMPA phonemes.
///
/// Based on the Moby Pronunciator II documentation.
const MPRON_TO_XSAMPA: &[(&str, &str)] = &[
    // Multi-character symbols (must be checked first - longest first)
    ("//Oi//", "OI"),  // ɔɪ
    ("/aU/", "aU"),    // aʊ
    ("/aI/", "aI"),    // aɪ
    ("/eI/", "eI"),    // eɪ
    ("/oU/", "oU"),    // oʊ
    ("/ju/", "ju"),    // juː
    ("/tS/", "tS"),    // tʃ
    ("/dZ/", "dZ"),    // dʒ
    ("/x/", "x"),      // x
    ("/y/", "y"),      // ø
    ("/z/", "z"),      // ts
    ("/&/", "{}"),     // æ
    ("/-/", "@"),      // ə
    ("/A/", "A"),      // ɑ
    ("/D/", "D"),      // ð
    ("/E/", "E"),      // ɛ
    ("/I/", "I"),      // ɪ
    ("/N/", "N"),      // ŋ
    ("/O/", "O"),      // ɔ
    ("/S/", "S"),      // ʃ
    ("/T/", "T"),      // θ
    ("/U/", "U"),      // ʊ
    ("/i/", "i"),      // iː
    ("/j/", "j"),      // j
    ("/u/", "u"),      // uː
    // Single-character symbols
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
    // Stress markers (mapped to nothing - we strip them)
    ("'", ""),   // Primary stress
    (",", ""),   // Secondary stress
    ("_", "_"),  // Word separator (maps to silence)
];

/// Load the Moby Pronunciator II dictionary from a file.
///
/// Returns a HashMap mapping lowercase word -> phoneme sequence in X-SAMPA.
/// Uses encoding_rs to handle the Mac OS Roman encoding.
pub fn load_dictionary(path: &str) -> HashMap<String, Vec<String>> {
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

    let mut dict = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse: word[/pos] pronunciation
        // The format is: word[ /pos] pronunciation
        // Find the first space to split word from pronunciation
        let space_pos = match line.find(' ') {
            Some(p) => p,
            None => continue,
        };

        let word_part = &line[..space_pos];
        let pron_part = line[space_pos + 1..].trim();

        // Extract word (strip optional /pos suffix)
        let word = if let Some(slash_pos) = word_part.find('/') {
            word_part[..slash_pos].to_lowercase()
        } else {
            word_part.to_lowercase()
        };

        // Parse pronunciation into X-SAMPA phonemes
        let phonemes = parse_pronunciation(pron_part);

        if phonemes.is_empty() {
            continue;
        }

        dict.insert(word, phonemes);
    }

    dict
}

/// Parse an mpron pronunciation string into X-SAMPA phonemes.
///
/// The mpron format uses:
/// - Multi-char symbols delimited by / (e.g., /aI/, /eI/, /tS/)
/// - Single-char symbols (e.g., b, d, f)
/// - Stress markers: ' (primary), , (secondary)
/// - Word separators: _
fn parse_pronunciation(pron: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut remaining = pron;

    // Build sorted list of symbols (longest first for greedy matching)
    let mut symbols: Vec<&str> = MPRON_TO_XSAMPA.iter().map(|(s, _)| *s).collect();
    symbols.sort_by(|a, b| b.len().cmp(&a.len()));

    while !remaining.is_empty() {
        let mut found = false;

        for &sym in &symbols {
            if remaining.len() >= sym.len() && &remaining[..sym.len()] == sym {
                let xsampa = MPRON_TO_XSAMPA
                    .iter()
                    .find(|(s, _)| *s == sym)
                    .map(|(_, x)| *x)
                    .unwrap_or("");

                if !xsampa.is_empty() {
                    result.push(xsampa.to_string());
                }
                // If xsampa is empty (stress markers), we skip it

                remaining = &remaining[sym.len()..];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_word() {
        // "hello" in mpron: /h/@/'l/oU/
        let phonemes = parse_pronunciation("/h/@/'l/oU/");
        assert_eq!(phonemes, vec!["h", "@", "l", "oU"]);
    }

    #[test]
    fn test_parse_with_stress() {
        // "about" in mpron: /@/'b/aU/t
        let phonemes = parse_pronunciation("/@/'b/aU/t");
        assert_eq!(phonemes, vec!["@", "b", "aU", "t"]);
    }

    #[test]
    fn test_parse_diphthongs() {
        // "voice" in mpron: /v//Oi//s
        let phonemes = parse_pronunciation("/v//Oi//s");
        assert_eq!(phonemes, vec!["v", "OI", "s"]);
    }

    #[test]
    fn test_parse_word_separator() {
        // "ice cream" in mpron: /aI/s_k/r/i/m
        let phonemes = parse_pronunciation("/aI/s_k/r/i/m");
        assert_eq!(phonemes, vec!["aI", "s", "_", "r", "i", "m"]);
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
        assert_eq!(
            dict.get("world"),
            Some(&vec!["w".to_string(), "@".to_string(), "r".to_string(), "l".to_string(), "d".to_string()])
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

        // Both should be loaded (last one wins for same key)
        assert!(dict.contains_key("record"));
    }

    #[test]
    fn test_parse_ae() {
        // "cat" in mpron: /k/&/t
        let phonemes = parse_pronunciation("/k/&/t");
        assert_eq!(phonemes, vec!["k", "{}", "t"]);
    }

    #[test]
    fn test_parse_theta() {
        // "think" in mpron: /T/I/N/k
        let phonemes = parse_pronunciation("/T/I/N/k");
        assert_eq!(phonemes, vec!["T", "I", "N", "k"]);
    }
}
