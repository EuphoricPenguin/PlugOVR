//! PlugOVR CLI — OddVoices singing synthesizer frontend.
//!
//! Usage:
//!     plugovr sing VOICE_FILE CMUDICT OUT_WAV [-m MIDI_FILE] [-l LYRICS_FILE]
//!
//! Options:
//!     -m, --midi FILE   Process a MIDI file
//!     -l, --lyrics FILE Load lyrics from file (overrides embedded MIDI lyrics)
//!     -h, --help        Print usage information

use std::fs;
use std::process;

use std::collections::HashMap;
use std::collections::HashSet;

use hound::{WavSpec, WavWriter, SampleFormat};
use midly::Smf;

use plugovr::{load_dictionary, voice::Voice, synth::Synth};

// ── Phoneme tables (matching C++ g2p.cpp) ──────────────────────────────

/// All known phonemes in X-SAMPA, sorted longest-first for greedy parsing.
fn all_phonemes() -> Vec<&'static str> {
    vec![
        "tS", "dZ", "@`", "oU", "eI", "aI", "OI", "aU",
        "l", "r", "j", "w",
        "m", "n", "N", "h", "k", "g", "p", "b", "t", "d",
        "f", "v", "T", "D", "s", "z", "S", "Z",
        "{}", "A", "I", "E", "@", "u", "U", "i",
        "_",
    ]
}

/// Vowels (used by VV fixer and is_vowel checks).
fn vowels() -> HashSet<&'static str> {
    [
        "{}", "@`", "A", "I", "E", "@", "u", "U", "i",
        "oU", "eI", "aI", "OI", "aU",
    ].iter().cloned().collect()
}

/// Vowel-to-vowel fixer: insert a glide consonant between adjacent vowels.
/// Matches C++ k_vvFixers.
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
/// Matches C++ k_phonemeAliases.
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

/// Mapping from ARPABET phonemes (CMUdict output) to X-SAMPA phonemes (voice segment names).
/// Matches C++ k_arpabetToXSAMPA.
const ARPABET_TO_XSAMPA: &[(&str, &str)] = &[
    ("AA", "A"),
    ("AE", "{}"),
    ("AH", "@"),
    ("AO", "O"),
    ("AW", "aU"),
    ("AY", "aI"),
    ("B", "b"),
    ("CH", "tS"),
    ("D", "d"),
    ("DH", "D"),
    ("EH", "E"),
    ("ER", "@`"),
    ("EY", "eI"),
    ("F", "f"),
    ("G", "g"),
    ("HH", "h"),
    ("IH", "I"),
    ("IY", "i"),
    ("JH", "dZ"),
    ("K", "k"),
    ("L", "l"),
    ("M", "m"),
    ("N", "n"),
    ("NG", "N"),
    ("OW", "oU"),
    ("OY", "OI"),
    ("P", "p"),
    ("R", "r"),
    ("S", "s"),
    ("SH", "S"),
    ("T", "t"),
    ("TH", "T"),
    ("UH", "U"),
    ("UW", "u"),
    ("V", "v"),
    ("W", "w"),
    ("Y", "j"),
    ("Z", "z"),
    ("ZH", "Z"),
];

/// Guess pronunciations for out-of-vocabulary words (matching C++ k_guessPronunciations).
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

/// CMUdict exceptions (matching C++ k_cmudictExceptions).
fn cmudict_exceptions() -> HashMap<&'static str, Vec<&'static str>> {
    let mut m = HashMap::new();
    m.insert("and", vec!["{}", "n", "d"]);
    m.insert("every", vec!["E", "v", "r", "i"]);
    m.insert("oddvoices", vec!["A", "d", "v", "OI", "s", "E", "z"]);
    m.insert("chesnokov", vec!["tS", "E", "z", "n", "oU", "k", "A", "v"]);
    m
}

/// Convert an ARPABET phoneme to X-SAMPA phoneme, stripping stress markers.
/// Returns None if no mapping exists.
fn arpabet_to_xsampa(phoneme: &str) -> Option<&'static str> {
    let last = phoneme.chars().last()?;
    let stripped = if last >= '0' && last <= '9' {
        &phoneme[..phoneme.len() - 1]
    } else {
        phoneme
    };
    ARPABET_TO_XSAMPA
        .iter()
        .find(|(arp, _)| *arp == stripped)
        .map(|(_, xs)| *xs)
}

/// Parse a pronunciation string (e.g. from /slashes/) into phonemes.
/// Matches C++ parsePronunciation.
fn parse_pronunciation(pronunciation: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut remaining = pronunciation;
    let aliases = phoneme_aliases();
    let mut all_phonemes = all_phonemes();
    // Add alias keys to the phoneme list for parsing
    for (key, _) in &aliases {
        all_phonemes.push(key);
    }
    // Sort by length descending for greedy matching
    all_phonemes.sort_by(|a, b| b.len().cmp(&a.len()));

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

/// Perform the cot-caught merger: /O/ -> /A/ (or /oU/ before /r/).
/// Matches C++ performCotCaughtMerger.
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
/// Matches C++ fixVVDiphones.
fn fix_vv_diphones(pronunciation: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let vowels = vowels();
    let fixers = vv_fixers();
    let mut last_phoneme = String::new();
    for phoneme in pronunciation {
        if vowels.contains(phoneme.as_str()) && fixers.contains_key(last_phoneme.as_str()) {
            result.push(fixers[last_phoneme.as_str()].to_string());
        }
        result.push(phoneme.clone());
        last_phoneme = phoneme.clone();
    }
    result
}

/// Normalize pronunciation by adding leading/trailing silence markers.
/// Matches C++ normalizePronunciation.
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
/// Matches C++ pronounceOOV.
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
/// Matches C++ tokenize().
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

/// Full G2P wrapper around the dictionary HashMap, matching C++ G2P::pronounce().
struct G2P {
    dict: HashMap<String, Vec<String>>,
}

impl G2P {
    fn new(dict: HashMap<String, Vec<String>>) -> Self {
        // Apply CMUdict exceptions
        let mut dict = dict;
        for (word, phonemes) in cmudict_exceptions() {
            dict.insert(word.to_string(), phonemes.iter().map(|s| s.to_string()).collect());
        }
        G2P { dict }
    }

    /// Pronounce a single word, returning X-SAMPA phonemes.
    /// Matches C++ G2P::pronounceWord().
    fn pronounce_word(&self, word: &str) -> Vec<String> {
        let result = if word.starts_with('/') {
            // Explicit phonetic input: /phonemes/
            let inner = &word[1..word.len().saturating_sub(1)];
            parse_pronunciation(inner)
        } else if let Some(phonemes) = self.dict.get(word) {
            // Look up in dictionary
            let mut result: Vec<String> = phonemes.iter()
                .filter_map(|p| arpabet_to_xsampa(p))
                .map(|s| s.to_string())
                .collect();
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
    /// Matches C++ G2P::pronounce().
    fn pronounce(&self, text: &str) -> Vec<String> {
        let words = tokenize(text);
        let mut result = Vec::new();
        for word in words {
            let pronunciation = self.pronounce_word(&word);
            result.extend(pronunciation);
        }
        result
    }
}

// ── Event-based synth interface (C++ mirror) ──────────────────────────

/// Event types for the synth, matching C++ EventType enum.
#[derive(Clone, PartialEq)]
enum EventType {
    SetFrequencyImmediate,
    SetTargetFrequency,
    NoteOn,
    NoteOnRT,
    NoteOffRT,
}

/// A timed event for the synth.
struct Event {
    event_type: EventType,
    seconds: f32,
    frequency: f32,
    end_seconds: f32,
}

/// Execute synth events with sample-accurate timing, matching C++ sing() function.
fn execute_events(
    voice: &Voice,
    events: &[Event],
    total_duration_seconds: f32,
    g2p: &G2P,
    lyrics: &str,
    output_wav: &str,
) -> Result<(), String> {
    let sample_rate = voice.sample_rate() as f32;

    // Get phonemes from lyrics using the full G2P pipeline
    let phoneme_list = g2p.pronounce(lyrics);

    let ref_phonemes: Vec<&str> = phoneme_list.iter().map(|s| s.as_str()).collect();
    let seg_indices = voice.convert_phonemes_to_segment_indices(&ref_phonemes);

    // Match C++ exactly: pass the segment indices as the queue memory buffer,
    // with capacity = len, start = 0, and initial size = len (pre-populated).
    let queue_capacity = seg_indices.len();
    let mem: Box<[i32]> = seg_indices.into_boxed_slice();
    let mut synth = Synth::new(
        sample_rate,
        voice,
        mem,
        queue_capacity,
        0,
        queue_capacity,
    );

    if synth.is_errored() {
        return Err("Synth initialization failed".to_string());
    }

    // Allocate output buffer
    let num_samples = (sample_rate * total_duration_seconds).ceil() as usize;
    let mut total_samples: Vec<i16> = Vec::with_capacity(num_samples);
    total_samples.resize(num_samples, 0);

    // Process events in chronological order, matching C++ iterative time tracking.
    let mut time_in_samples: usize = 0;
    let mut time_in_seconds: f32 = 0.0;

    // Sort events by time
    let mut sorted_events: Vec<(f32, &Event)> = events.iter().map(|e| (e.seconds, e)).collect();
    sorted_events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    for (event_time, event) in &sorted_events {
        let num_samples_to_process = ((*event_time - time_in_seconds) * sample_rate).max(0.0) as usize;

        for _ in 0..num_samples_to_process {
            if time_in_samples < num_samples {
                total_samples[time_in_samples] = synth.process() as i16;
                time_in_samples += 1;
            }
        }

        time_in_seconds += num_samples_to_process as f32 / sample_rate;

        match event.event_type {
            EventType::NoteOn => {
                let duration = event.end_seconds - event.seconds;
                synth.note_on(duration.max(0.0));
            }
            EventType::NoteOnRT => {
                synth.note_on(0.0);
            }
            EventType::NoteOffRT => {
                synth.note_off();
            }
            EventType::SetTargetFrequency => {
                synth.set_target_frequency(event.frequency);
            }
            EventType::SetFrequencyImmediate => {
                synth.set_frequency_immediate(event.frequency);
            }
        }
    }

    // Process remaining samples
    while time_in_samples < num_samples {
        total_samples[time_in_samples] = synth.process() as i16;
        time_in_samples += 1;
    }

    // Write WAV file
    let spec = WavSpec {
        channels: 1,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut writer = WavWriter::create(output_wav, spec)
        .map_err(|e| format!("Failed to create WAV file '{}': {}", output_wav, e))?;

    for &sample in &total_samples[..time_in_samples.min(total_samples.len())] {
        writer.write_sample(sample)
            .map_err(|e| format!("Failed to write sample: {}", e))?;
    }

    writer.finalize()
        .map_err(|e| format!("Failed to finalize WAV file: {}", e))?;

    eprintln!(
        "Synthesized {} samples ({:.2}s)",
        time_in_samples.min(total_samples.len()),
        time_in_samples.min(total_samples.len()) as f32 / sample_rate
    );
    Ok(())
}

/// Compute frequency from MIDI key number.
/// MIDI 69 = A4 = 440 Hz.
fn midi_key_to_freq(key: u8) -> f32 {
    440.0 * 2.0f32.powf((key as i32 - 69) as f32 / 12.0)
}

// ── MIDI Processing ────────────────────────────────────────────────────

/// Convert a tick to seconds given a tempo track.
/// Uses cumulative time calculation to handle tempo changes correctly.
fn tick_to_seconds(
    abs_tick: u64,
    tempo_events: &[(u64, f64)], // (tick, seconds_per_tick_at_this_point)
) -> f64 {
    let mut total_secs = 0.0f64;
    let mut prev_tick: u64 = 0;
    let mut current_spt = 0.0; // seconds per tick

    for &(tick, spt) in tempo_events {
        if abs_tick <= tick {
            // The target tick is before this tempo change
            let delta_ticks = (abs_tick - prev_tick) as f64;
            total_secs += delta_ticks * current_spt;
            return total_secs;
        }
        let delta_ticks = (tick - prev_tick) as f64;
        total_secs += delta_ticks * current_spt;
        prev_tick = tick;
        current_spt = spt;
    }

    // After all tempo events
    let delta_ticks = (abs_tick - prev_tick) as f64;
    total_secs += delta_ticks * current_spt;
    total_secs
}

fn process_midi(
    voice: &Voice,
    g2p: &G2P,
    midi_path: &str,
    lyrics_text: &str,
    output_wav: &str,
) -> Result<(), String> {
    // Read MIDI file
    let raw = fs::read(midi_path)
        .map_err(|e| format!("Failed to read MIDI file '{}': {}", midi_path, e))?;

    // Parse MIDI file
    let smf = Smf::parse(&raw)
        .map_err(|e| format!("Failed to parse MIDI file: {}", e))?;

    eprintln!("MIDI: {} tracks, format {:?}, timing: {:?}",
        smf.tracks.len(), smf.header.format, smf.header.timing);

    // Get lyrics: use embedded MIDI lyrics if no external lyrics provided
    let use_midi_lyrics = lyrics_text.is_empty();
    let mut lyrics_text = lyrics_text.to_string();

    if use_midi_lyrics {
        let mut midi_lyrics = String::new();
        for track in &smf.tracks {
            for event in track {
                if let midly::TrackEventKind::Meta(meta) = &event.kind {
                    let text_bytes: &[u8] = match meta {
                        midly::MetaMessage::Lyric(s) => s,
                        _ => continue,
                    };
                    if let Ok(text) = std::str::from_utf8(text_bytes) {
                        midi_lyrics.push_str(text.trim());
                        midi_lyrics.push(' ');
                    }
                }
            }
        }
        if !midi_lyrics.is_empty() {
            lyrics_text = midi_lyrics.trim().to_string();
        }
    }

    if !lyrics_text.is_empty() {
        eprintln!("Lyrics: {}", lyrics_text);
    }

    // Get ticks per beat
    let ticks_per_beat = match smf.header.timing {
        midly::Timing::Metrical(u) => u.as_int() as f64,
        midly::Timing::Timecode(..) => return Err("Timecode-style MIDI timing not supported".into()),
    };

    // ── First pass: collect all tempo events across all tracks ──
    // Build a sorted list of (tick, seconds_per_tick) events.
    let mut tempo_events: Vec<(u64, f64)> = Vec::new();
    // Default: 120 BPM = 0.5 seconds per quarter note = 0.5 / ticks_per_beat seconds per tick
    let default_spt = 0.5 / ticks_per_beat;
    let mut has_tempo_at_tick_zero = false;

    for track in &smf.tracks {
        let mut abs_tick: u64 = 0;
        for event in track {
            abs_tick += event.delta.as_int() as u64;
            if let midly::TrackEventKind::Meta(meta) = &event.kind {
                if let midly::MetaMessage::Tempo(msqn) = meta {
                    let micros: u32 = (*msqn).into();
                    let bpm = 60_000_000.0 / micros as f64;
                    let seconds_per_tick = 60.0 / (bpm * ticks_per_beat);
                    if abs_tick == 0 {
                        has_tempo_at_tick_zero = true;
                    }
                    tempo_events.push((abs_tick, seconds_per_tick));
                }
            }
        }
    }

    // Only add default tempo if no tempo event exists at tick 0
    if !has_tempo_at_tick_zero {
        tempo_events.push((0, default_spt));
    }

    // Sort by tick, then by position (first wins for same tick)
    tempo_events.sort_by(|a, b| a.0.cmp(&b.0));

    // ── Find the track with note events ──
    let mut target_track_index: Option<usize> = None;
    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut has_notes = false;
        for event in track {
            if let midly::TrackEventKind::Midi { channel, message } = &event.kind {
                if channel.as_int() == 0 {
                    match message {
                        midly::MidiMessage::NoteOn { .. } | midly::MidiMessage::NoteOff { .. } => {
                            has_notes = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        if has_notes {
            target_track_index = Some(track_idx);
            break;
        }
    }

    let track_index = target_track_index.unwrap_or(0);
    let track = &smf.tracks[track_index];

    // ── Second pass: process the target track ──
    let mut abs_tick: u64 = 0;
    let mut time_of_track_end: Option<f64> = None;
    let mut notes_on: Vec<u8> = Vec::new(); // stack of currently playing notes

    // Collect events: each event is (time_seconds, event_type, frequency, end_time_or_0)
    let mut events: Vec<(f64, EventType, f32, f32)> = Vec::new();
    let mut melisma_start: Option<f64> = None;

    for event in track {
        abs_tick += event.delta.as_int() as u64;

        // Check for end of track
        if let midly::TrackEventKind::Meta(meta) = &event.kind {
            if matches!(meta, midly::MetaMessage::EndOfTrack) {
                time_of_track_end = Some(tick_to_seconds(abs_tick, &tempo_events));
            }
        }

        if let midly::TrackEventKind::Midi { channel, message } = &event.kind {
            if channel.as_int() != 0 {
                continue;
            }

            match message {
                midly::MidiMessage::NoteOn { key, vel } => {
                    if vel.as_int() == 0 {
                        // Note-on with velocity 0 is equivalent to note-off
                        let midi_note = key.as_int() as u8;
                        let time_sec = tick_to_seconds(abs_tick, &tempo_events);

                        notes_on.retain(|&n| n != midi_note);

                        if let Some(start_time) = melisma_start {
                            if notes_on.is_empty() {
                                events.push((start_time, EventType::NoteOn, 0.0, time_sec as f32));
                                melisma_start = None;
                            }
                        }

                        if !notes_on.is_empty() {
                            let top_note = notes_on[notes_on.len() - 1];
                            let freq = midi_key_to_freq(top_note);
                            events.push((time_sec, EventType::SetTargetFrequency, freq as f32, 0.0));
                        }
                        continue;
                    }
                    let midi_note = key.as_int() as u8;
                    let time_sec = tick_to_seconds(abs_tick, &tempo_events);

                    if notes_on.is_empty() {
                        // Start of a melisma - queue the noteOn event
                        melisma_start = Some(time_sec);
                    }

                    notes_on.push(midi_note);

                    let freq = midi_key_to_freq(midi_note) as f32;
                    events.push((time_sec, EventType::SetTargetFrequency, freq, 0.0));
                }
                midly::MidiMessage::NoteOff { key, .. } => {
                    let midi_note = key.as_int() as u8;
                    let time_sec = tick_to_seconds(abs_tick, &tempo_events);

                    notes_on.retain(|&n| n != midi_note);

                    if let Some(start_time) = melisma_start {
                        if notes_on.is_empty() {
                            // End of melisma - complete the noteOn event
                            events.push((start_time, EventType::NoteOn, 0.0, time_sec as f32));
                            melisma_start = None;
                        }
                    }

                    if !notes_on.is_empty() {
                        // Switch to the new top note
                        let top_note = notes_on[notes_on.len() - 1];
                        let freq = midi_key_to_freq(top_note);
                        events.push((time_sec, EventType::SetTargetFrequency, freq as f32, 0.0));
                    }
                }
                _ => {}
            }
        }
    }

    let total_duration = time_of_track_end.unwrap_or(0.0) as f32;

    eprintln!("MIDI: {} events, {}s duration", events.len(), total_duration);

    // Convert events to proper Event structs
    let synth_events: Vec<Event> = events.iter().map(|(time, etype, freq, end)| Event {
        event_type: etype.clone(),
        seconds: *time as f32,
        frequency: *freq,
        end_seconds: *end,
    }).collect();

    execute_events(
        voice,
        &synth_events,
        total_duration.max(1.0),
        g2p,
        &lyrics_text,
        output_wav,
    )
}

// ── Utility ────────────────────────────────────────────────────────────

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("    plugovr sing VOICE_FILE CMUDICT OUT_WAV");
    eprintln!("        [-m MIDI_FILE] [-l LYRICS_FILE]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("    -m, --midi FILE    Process a MIDI file");
    eprintln!("    -l, --lyrics FILE  Load lyrics from file");
    eprintln!("    -h, --help         Print this help message");
}

/// Read an entire file as a string.
fn read_file_string(path: &str) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file '{}': {}", path, e))
}

// ── Main ───────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Handle top-level help flag
    if args.len() >= 2 && (args[1] == "-h" || args[1] == "--help") {
        print_usage();
        process::exit(0);
    }

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let mode = &args[1];

    if mode != "sing" {
        eprintln!("Unknown command: {}", mode);
        print_usage();
        process::exit(1);
    }

    // Parse 'sing' subcommand arguments
    let sing_args: Vec<String> = args[2..].to_vec();
    let mut positional = Vec::new();
    let mut midi_file: Option<String> = None;
    let mut lyrics_file: Option<String> = None;

    let mut i = 0;
    while i < sing_args.len() {
        match sing_args[i].as_str() {
            "--" => {
                for remaining in &sing_args[i + 1..] {
                    positional.push(remaining.clone());
                }
                break;
            }
            "-m" | "--midi" => {
                i += 1;
                if i >= sing_args.len() {
                    eprintln!("Error: -m/--midi requires a file argument");
                    process::exit(1);
                }
                midi_file = Some(sing_args[i].clone());
            }
            "-l" | "--lyrics" => {
                i += 1;
                if i >= sing_args.len() {
                    eprintln!("Error: -l/--lyrics requires a file argument");
                    process::exit(1);
                }
                lyrics_file = Some(sing_args[i].clone());
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            arg if arg.starts_with('-') => {
                eprintln!("Error: Unrecognized option: {}", arg);
                print_usage();
                process::exit(1);
            }
            _ => {
                positional.push(sing_args[i].clone());
            }
        }
        i += 1;
    }

    // Validate arguments
    if positional.len() < 3 {
        eprintln!("Error: At least 3 positional arguments required: VOICE_FILE CMUDICT OUT_WAV");
        print_usage();
        process::exit(1);
    }

    if midi_file.is_none() {
        eprintln!("Error: You must provide a MIDI file (-m).");
        print_usage();
        process::exit(1);
    }

    let voice_file = &positional[0];
    let cmudict_file = &positional[1];
    let out_wav = &positional[2];

    // Load lyrics from file if provided
    let lyrics = match &lyrics_file {
        Some(path) => {
            match read_file_string(path) {
                Ok(text) => text.trim().to_string(),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    process::exit(1);
                }
            }
        }
        None => String::new(),
    };

    // Load voice
    let mut voice = Voice::new();
    voice.init_from_file(voice_file);

    if !voice.has_init_finished() {
        eprintln!("Error: Voice file '{}' failed to initialize.", voice_file);
        process::exit(1);
    }

    eprintln!("Voice: rate={}, grain_length={}, phonemes={}, segments={}",
        voice.sample_rate(), voice.grain_length(),
        voice.num_phonemes(), voice.num_segments());

    // Load CMU dictionary
    let dict = load_dictionary(cmudict_file);
    eprintln!("Loaded {} entries from dictionary", dict.len());

    // Create G2P instance with the dictionary
    let g2p = G2P::new(dict);

    // Process MIDI
    let midi_path = midi_file.as_ref().unwrap();
    match process_midi(&voice, &g2p, midi_path, &lyrics, out_wav) {
        Ok(()) => {
            eprintln!("Successfully wrote output to: {}", out_wav);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
