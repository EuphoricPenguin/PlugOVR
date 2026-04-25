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

use hound::{WavSpec, WavWriter, SampleFormat};
use midly::Smf;

use plugovr::{load_dictionary, voice::Voice, synth::Synth};

/// Mapping from ARPABET phonemes (CMUdict output) to X-SAMMA phonemes (voice segment names).
/// This matches the mapping in python/phonology.py.
const ARPABET_TO_XSAMMA: &[(&str, &str)] = &[
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

/// Convert an ARPABET phoneme to X-SAMMA phoneme.
/// Returns the same phoneme if no mapping exists.
fn arpabet_to_xsampa(phoneme: &str) -> &str {
    ARPABET_TO_XSAMMA
        .iter()
        .find(|(arp, _)| *arp == phoneme)
        .map(|(_, xs)| *xs)
        .unwrap_or(phoneme)
}

/// Simple G2P wrapper around the dictionary HashMap.
struct G2P {
    dict: HashMap<String, Vec<String>>,
}

impl G2P {
    fn new(dict: HashMap<String, Vec<String>>) -> Self {
        G2P { dict }
    }

    /// Look up a word and return its phonemes (stress markers stripped, converted to X-SAMMA).
    /// Returns None if the word is not in the dictionary.
    fn pronounce_word(&self, word: &str) -> Option<Vec<String>> {
        let key = word.to_lowercase()
            .trim_end_matches(|c: char| c.is_ascii_punctuation())
            .to_string();
        let phonemes = self.dict.get(&key)?;
        let xsampa_phonemes: Vec<String> = phonemes.iter().map(|p| arpabet_to_xsampa(p).to_string()).collect();
        Some(xsampa_phonemes)
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
    segment_indices: &[i32],
    g2p: &G2P,
    lyrics: &str,
    output_wav: &str,
) -> Result<(), String> {
    let sample_rate = voice.sample_rate() as f32;

    // Get phonemes from lyrics
    let mut phoneme_list: Vec<String> = Vec::new();
    for word in lyrics.split_whitespace() {
        let clean_word = word.trim_end_matches(|c: char| c.is_ascii_punctuation()).to_lowercase();
        if let Some(phonemes) = g2p.pronounce_word(&clean_word) {
            phoneme_list.extend(phonemes);
        }
    }

    let ref_phonemes: Vec<&str> = phoneme_list.iter().map(|s| s.as_str()).collect();
    let seg_indices = voice.convert_phonemes_to_segment_indices(&ref_phonemes);

    // Pre-queue all segments
    let queue_capacity = seg_indices.len().max(256) as i32;
    let mem: Box<[i32]> = vec![-1; queue_capacity as usize].into_boxed_slice();
    let mut synth = Synth::new(
        sample_rate,
        voice,
        mem,
        queue_capacity as usize,
        0,
        0,
    );

    if synth.is_errored() {
        return Err("Synth initialization failed".to_string());
    }

    for &seg_idx in &seg_indices {
        synth.queue_segment(seg_idx);
    }

    // Allocate output buffer
    let num_samples = (sample_rate * total_duration_seconds).ceil() as usize;
    let mut total_samples: Vec<i16> = Vec::with_capacity(num_samples);
    total_samples.resize(num_samples, 0);

    // Process events in chronological order, matching C++ iterative time tracking.
    // The C++ code uses (event.seconds - timeInSeconds) * sampleRate to compute
    // numSamplesToProcess, which avoids cumulative floating-point rounding errors
    // that would occur with absolute time * sample_rate calculations.
    let mut time_in_samples: usize = 0;
    let mut time_in_seconds: f32 = 0.0;

    // Sort events by time
    let mut sorted_events: Vec<(f32, &Event)> = events.iter().map(|e| (e.seconds, e)).collect();
    sorted_events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    for (event_time, event) in &sorted_events {
        // Iterative sample count: matches C++ exactly
        let num_samples_to_process = ((*event_time - time_in_seconds) * sample_rate).max(0.0) as usize;

        for _ in 0..num_samples_to_process {
            if time_in_samples < num_samples {
                total_samples[time_in_samples] = synth.process() as i16;
                time_in_samples += 1;
            }
        }

        // Update time using the same iterative approach as C++
        time_in_seconds += num_samples_to_process as f32 / sample_rate;

        // Execute the event
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

    // Write WAV file, matching the C++ write16BitMonoWAVFile which writes samples as-is.
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

    // Determine sample rate from voice
    let sample_rate = voice.sample_rate() as f32;

    // Get ticks per beat and compute seconds for each tick
    let ticks_per_beat = match smf.header.timing {
        midly::Timing::Metrical(u) => u.as_int() as f64,
        midly::Timing::Timecode(..) => return Err("Timecode-style MIDI timing not supported".into()),
    };

    // Use 120 BPM default if no tempo meta message found
    let mut bpm = 120.0;
    let mut tempo_found = false;

    // First pass: find tempo in the track with notes
    let mut target_track_index: Option<usize> = None;
    let mut active_notes: std::collections::HashMap<u8, (u64, f64)> = std::collections::HashMap::new();
    let mut note_events: Vec<(f64, f64, u8)> = Vec::new(); // (start_sec, end_sec, midi_note)

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut abs_tick: u64 = 0;
        let mut track_bpm = 120.0;

        for event in track {
            abs_tick += event.delta.as_int() as u64;

            // Check for tempo meta message
            if let midly::TrackEventKind::Meta(meta) = &event.kind {
                if let midly::MetaMessage::Tempo(msqn) = meta {
                    // MIDI tempo = microseconds per quarter note
                    let micros: u32 = (*msqn).into();
                    track_bpm = 60_000_000.0 / micros as f64;
                }
            }

            if let midly::TrackEventKind::Midi { channel, message } = &event.kind {
                if channel.as_int() != 0 {
                    continue;
                }

                match message {
                    midly::MidiMessage::NoteOn { key, vel } => {
                        if vel.as_int() != 0 {
                            active_notes.insert(key.as_int(), (abs_tick, track_bpm));
                        }
                    }
                    midly::MidiMessage::NoteOff { key, .. } => {
                        let key_val = key.as_int() as u8;
                        if let Some((start_tick, track_bpm)) = active_notes.remove(&key_val) {
                            note_events.push((start_tick as f64, abs_tick as f64, key_val));
                        }
                    }
                    _ => {}
                }
            }
        }

        // Count note events
        if !active_notes.is_empty() {
            if target_track_index.is_none() {
                target_track_index = Some(track_idx);
            }
        }
    }

    if !target_track_index.is_some() {
        // Check if any track has note events
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
    }

    let track_index = target_track_index.unwrap_or(0);
    let track = &smf.tracks[track_index];

    // Second pass: process the target track
    let mut abs_tick: u64 = 0;
    let mut track_bpm = bpm;
    let mut time_of_track_end: Option<f64> = None;
    let mut active_notes: std::collections::HashMap<u8, f64> = std::collections::HashMap::new(); // midi_note -> start_time
    let mut notes_on: Vec<u8> = Vec::new(); // stack of currently playing notes

    // Collect events: each event is (time_seconds, event_type, frequency, end_time_or_0)
    let mut events: Vec<(f64, EventType, f32, f32)> = Vec::new();
    let mut melisma_start: Option<f64> = None;

    for event in track {
        abs_tick += event.delta.as_int() as u64;

        // Check for tempo changes
        if let midly::TrackEventKind::Meta(meta) = &event.kind {
            if let midly::MetaMessage::Tempo(msqn) = meta {
                let micros: u32 = (*msqn).into();
                track_bpm = 60_000_000.0 / micros as f64;
            }
            if matches!(meta, midly::MetaMessage::EndOfTrack) {
                let tick = abs_tick as f64;
                let ticks_per_sec = track_bpm / 60.0 * ticks_per_beat;
                let sec = tick / ticks_per_sec;
                time_of_track_end = Some(sec);
            }
        }

        if let midly::TrackEventKind::Midi { channel, message } = &event.kind {
            if channel.as_int() != 0 {
                continue;
            }

            match message {
                midly::MidiMessage::NoteOn { key, vel } => {
                    if vel.as_int() == 0 {
                        continue;
                    }
                    let midi_note = key.as_int() as u8;
                    let tick = abs_tick as f64;
                    let ticks_per_sec = track_bpm / 60.0 * ticks_per_beat;
                    let time_sec = tick / ticks_per_sec;

                    if notes_on.is_empty() {
                        // Start of a melisma - queue the noteOn event
                        melisma_start = Some(time_sec);
                    }

                    notes_on.push(midi_note);
                    active_notes.insert(midi_note, time_sec);

                    let freq = midi_key_to_freq(midi_note) as f32;
                    events.push((time_sec, EventType::SetTargetFrequency, freq, 0.0));
                }
                midly::MidiMessage::NoteOff { key, .. } => {
                    let midi_note = key.as_int() as u8;
                    let tick = abs_tick as u64;
                    let ticks_per_sec = track_bpm / 60.0 * ticks_per_beat;
                    let time_sec = tick as f64 / ticks_per_sec as f64;

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
                        events.push((time_sec as f64, EventType::SetTargetFrequency, freq as f32, 0.0));
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
        &vec![], // Not used in this approach - phonemes come from lyrics
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_key_to_freq() {
        // MIDI 69 = A4 = 440 Hz
        assert_eq!(midi_key_to_freq(69), 440.0);
        // MIDI 60 = Middle C, should be ~261.63 Hz
        let freq_c4 = midi_key_to_freq(60);
        assert!(freq_c4 > 260.0 && freq_c4 < 263.0);
        // MIDI 72 = C5 = ~523.25 Hz
        let freq_c5 = midi_key_to_freq(72);
        assert!(freq_c5 > 520.0 && freq_c5 < 526.0);
    }
}