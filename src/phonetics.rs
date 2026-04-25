/// CMU Dictionary phoneme lookup for the OddVoices synthesizer.
///
/// Loads the CMUdict file and provides word-to-phoneme conversion.
/// Phoneme stress markers (0, 1, 2 suffixes) are stripped for synthesis.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

/// Load the CMU dictionary from a file and return a phoneme map.
///
/// Returns a HashMap mapping lowercase word -> phoneme sequence (stress markers stripped).
/// Uses encoding_rs to handle the legacy Windows-1252 encoding common in CMUdict files.
pub fn load_dictionary(path: &str) -> HashMap<String, Vec<String>> {
    let mut raw = Vec::new();
    let content: Cow<str> = match File::open(path) {
        Ok(mut f) => {
            f.read_to_end(&mut raw).unwrap_or_default();
            let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&raw);
            decoded
        }
        Err(e) => {
            eprintln!("Warning: Could not open dictionary file '{}': {}", path, e);
            return HashMap::new();
        }
    };

    let mut dict = HashMap::new();

    for line in content.lines() {
        // Skip comments
        if line.starts_with(';') || line.trim().is_empty() {
            continue;
        }

        // Parse: WORD  PH1 PH2 PH3 ...
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() < 2 {
            continue;
        }

        let word = parts[0].to_lowercase();
        let phonemes_str = parts[1].trim();

        // Split phonemes and strip stress markers
        let phonemes: Vec<String> = phonemes_str
            .split_whitespace()
            .filter(|p| !p.is_empty())
            .map(|p: &str| strip_stress_marker(p))
            .collect();

        if phonemes.is_empty() {
            continue;
        }

        dict.insert(word, phonemes);
    }

    dict
}

/// Strip the stress marker (0, 1, or 2) from a phoneme.
///
/// Examples:
/// - "AH0" -> "AH"
/// - "AE1" -> "AE"
/// - "NG"  -> "NG" (no marker)
pub fn strip_stress_marker(phoneme: &str) -> String {
    phoneme
        .trim_end_matches('0')
        .trim_end_matches('1')
        .trim_end_matches('2')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_stress_marker_no_marker() {
        assert_eq!(strip_stress_marker("AH"), "AH");
        assert_eq!(strip_stress_marker("NG"), "NG");
        assert_eq!(strip_stress_marker("EH"), "EH");
    }

    #[test]
    fn test_strip_stress_marker_unstressed() {
        assert_eq!(strip_stress_marker("AH0"), "AH");
        assert_eq!(strip_stress_marker("AE0"), "AE");
        assert_eq!(strip_stress_marker("NG0"), "NG");
    }

    #[test]
    fn test_strip_stress_marker_primary_stress() {
        assert_eq!(strip_stress_marker("AO1"), "AO");
        assert_eq!(strip_stress_marker("EH1"), "EH");
        assert_eq!(strip_stress_marker("IY1"), "IY");
    }

    #[test]
    fn test_strip_stress_marker_secondary_stress() {
        assert_eq!(strip_stress_marker("AH2"), "AH");
        assert_eq!(strip_stress_marker("AO2"), "AO");
        assert_eq!(strip_stress_marker("EH2"), "EH");
    }

    #[test]
    fn test_load_dictionary_empty_file() {
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        // Empty file
        let dict = load_dictionary(tmp.path().to_str().unwrap());
        assert!(dict.is_empty());
    }

    #[test]
    fn test_load_dictionary_with_entries() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        let mut f = tmp.as_file();
        f.write_all(b"; Comment line\n").unwrap();
        f.write_all(b"ABORT  AH0 B AO1 R T\n").unwrap();
        f.write_all(b"TEST  T EH1 S T\n").unwrap();
        f.write_all(b"\n").unwrap();

        let dict = load_dictionary(tmp.path().to_str().unwrap());

        assert_eq!(
            dict.get("abort"),
            Some(&vec!["AH".to_string(), "B".to_string(), "AO".to_string(), "R".to_string(), "T".to_string()])
        );
        assert_eq!(
            dict.get("test"),
            Some(&vec!["T".to_string(), "EH".to_string(), "S".to_string(), "T".to_string()])
        );
        // Dictionary keys should be lowercase
        assert!(dict.get("ABORT").is_none());
        assert!(dict.get("Abort").is_none());
        assert!(dict.get("abort").is_some());
    }

    #[test]
    fn test_load_dictionary_single_phoneme() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        let mut f = tmp.as_file();
        f.write_all(b"I  AY1\n").unwrap();
        f.write_all(b"A  EY0\n").unwrap();

        let dict = load_dictionary(tmp.path().to_str().unwrap());

        assert_eq!(
            dict.get("i"),
            Some(&vec!["AY".to_string()])
        );
        // "A" with stress 0 maps to "EY0" -> "EY"
        assert_eq!(
            dict.get("a"),
            Some(&vec!["EY".to_string()])
        );
    }

    #[test]
    fn test_load_dictionary_special_characters() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        let mut f = tmp.as_file();
        f.write_all(b"O'Connor  AH1 K AA0 N ER0\n").unwrap();
        f.write_all(b"D'Acosta  D AH0 AE1 K S AH0 T AH0\n").unwrap();

        let dict = load_dictionary(tmp.path().to_str().unwrap());

        assert!(dict.get("o'connor").is_some());
        assert!(dict.get("d'acosta").is_some());
    }

    #[test]
    fn test_load_dictionary_ignored_entries() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::new().unwrap();
        let mut f = tmp.as_file();
        // Lines with (1) suffix are alternate pronunciations - still loaded
        f.write_all(b"READ  R IH0 D\n").unwrap();
        f.write_all(b"READ(1)  R EH1 D\n").unwrap();

        let dict = load_dictionary(tmp.path().to_str().unwrap());

        // Both should be loaded
        assert!(dict.get("read").is_some());
        // The last entry for "read" overwrites the first (since we use lowercase key)
    }
}