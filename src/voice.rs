/// Voice loading for the OddVoices synthesizer.
///
/// Parses the `.voice` binary file format and provides lookup tables
/// for segments, phonemes, and the wavetable memory.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

const MAGIC_WORD: &[u8; 12] = b"ODDVOICES\0\0\0";

/// Read a null-terminated ASCII string from a reader.
fn read_string<R: Read>(reader: &mut R) -> std::io::Result<String> {
    let mut result = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        if byte[0] == 0 {
            break;
        }
        if result.len() > 255 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "String longer than 255 characters",
            ));
        }
        result.push(byte[0]);
    }
    String::from_utf8(result).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Read a little-endian i32 from a reader.
fn read_i32_le<R: Read>(reader: &mut R) -> std::io::Result<i32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

/// An OddVoices model, a database consisting of wavetable memory and metadata.
///
/// All methods are believed to be real-time safe unless otherwise noted.
/// Once initialized with `init_from_file`, the Voice is read-only and its methods can
/// be safely called from multiple threads.
pub struct Voice {
    has_init_started: std::sync::atomic::AtomicBool,
    has_init_finished: std::sync::atomic::AtomicBool,

    sample_rate: i32,
    grain_length: i32,
    silent_segment_index: i32,
    phonemes: Vec<String>,
    segments: Vec<String>,
    segments_num_frames: Vec<i32>,
    segments_is_vowel: Vec<bool>,
    segments_offset: Vec<i32>,
    wavetable_memory: Vec<i16>,
}

impl Voice {
    /// Create a new uninitialized voice.
    pub const fn new() -> Self {
        Self {
            has_init_started: std::sync::atomic::AtomicBool::new(false),
            has_init_finished: std::sync::atomic::AtomicBool::new(false),
            sample_rate: 0,
            grain_length: 0,
            silent_segment_index: 0,
            phonemes: Vec::new(),
            segments: Vec::new(),
            segments_num_frames: Vec::new(),
            segments_is_vowel: Vec::new(),
            segments_offset: Vec::new(),
            wavetable_memory: Vec::new(),
        }
    }

    /// Return true if initialization of this voice has started, and it cannot be initialized
    /// again. Return false otherwise.
    pub fn has_init_started(&self) -> bool {
        self.has_init_started
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Return true if initialization of this voice has completed, and it is safe to play a
    /// Synth from it. Return false otherwise.
    pub fn has_init_finished(&self) -> bool {
        self.has_init_finished
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Initialize the voice from a path to a `.voice` file.
    /// Do nothing if the voice has already been initialized or is currently initializing.
    /// This method is not real-time safe.
    /// This method is not safe from untrusted `.voice` files, and will probably crash if a
    /// malformed file is provided.
    /// It is safe to call this method multiple times, and even simultaneously from different
    /// threads.
    pub fn init_from_file<P: AsRef<Path>>(&mut self, path: P) {
        if self
            .has_init_started
            .load(std::sync::atomic::Ordering::SeqCst)
            || self
                .has_init_finished
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        self.has_init_started
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => {
                self.has_init_started
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                return;
            }
        };
        let mut reader = BufReader::new(file);

        // Check magic word
        let mut magic = [0u8; 12];
        if reader.read_exact(&mut magic).is_err() || magic != *MAGIC_WORD {
            self.has_init_started
                .store(false, std::sync::atomic::Ordering::SeqCst);
            return;
        }

        // Read header
        let sample_rate = match read_i32_le(&mut reader) {
            Ok(v) => v,
            Err(_) => {
                self.has_init_started
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                return;
            }
        };
        let grain_length = match read_i32_le(&mut reader) {
            Ok(v) => v,
            Err(_) => {
                self.has_init_started
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                return;
            }
        };

        // Read phoneme list
        let mut phonemes = Vec::new();
        loop {
            let phoneme = match read_string(&mut reader) {
                Ok(v) => v,
                Err(_) => {
                    self.has_init_started
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
            };
            if phoneme.is_empty() {
                break;
            }
            phonemes.push(phoneme);
        }
        // Add implicit silence phoneme
        phonemes.push("_".to_string());

        // Read segment list
        let mut segments = Vec::new();
        let mut segments_num_frames = Vec::new();
        let mut segments_is_vowel = Vec::new();
        let mut segments_offset = Vec::new();

        let mut offset = 0i32;
        loop {
            let segment_name = match read_string(&mut reader) {
                Ok(v) => v,
                Err(_) => {
                    self.has_init_started
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
            };
            if segment_name.is_empty() {
                break;
            }
            let segment_num_frames = match read_i32_le(&mut reader) {
                Ok(v) => v,
                Err(_) => {
                    self.has_init_started
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
            };
            let segment_is_vowel_val = match read_i32_le(&mut reader) {
                Ok(v) => v != 0,
                Err(_) => {
                    self.has_init_started
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
            };

            segments.push(segment_name);
            segments_num_frames.push(segment_num_frames);
            segments_is_vowel.push(segment_is_vowel_val);
            segments_offset.push(offset);
            offset += segment_num_frames * grain_length;
        }

        // Add implicit silent segment
        let silent_segment_index = segments.len() as i32;
        segments.push("_".to_string());
        segments_num_frames.push(0);
        segments_is_vowel.push(true);
        segments_offset.push(offset);

        // Read wavetable memory (16-bit samples, interleaved across all segments)
        let wavetable_len = offset as usize;
        self.wavetable_memory = Vec::with_capacity(wavetable_len);
        for _ in 0..wavetable_len {
            let mut buf = [0u8; 2];
            match reader.read_exact(&mut buf) {
                Ok(_) => self.wavetable_memory.push(i16::from_le_bytes(buf)),
                Err(_) => {
                    self.has_init_started
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
            }
        }

        self.sample_rate = sample_rate;
        self.grain_length = grain_length;
        self.silent_segment_index = silent_segment_index;
        self.phonemes = phonemes;
        self.segments = segments;
        self.segments_num_frames = segments_num_frames;
        self.segments_is_vowel = segments_is_vowel;
        self.segments_offset = segments_offset;
        self.has_init_finished
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Return the sample rate of the voice's wavetable memory.
    #[inline]
    pub fn sample_rate(&self) -> i32 {
        self.sample_rate
    }

    /// Return the length in samples of each grain in wavetable memory.
    #[inline]
    pub fn grain_length(&self) -> i32 {
        self.grain_length
    }

    /// Return a reference to the wavetable memory.
    #[inline]
    pub fn wavetable_memory(&self) -> &[i16] {
        &self.wavetable_memory
    }

    /// Return the number of phonemes.
    #[inline]
    pub fn num_phonemes(&self) -> usize {
        self.phonemes.len()
    }

    /// Given a phoneme name, return its index. Return None if not present. Not real-time safe.
    pub fn phoneme_to_phoneme_index(&self, phoneme: &str) -> Option<usize> {
        self.phonemes
            .iter()
            .position(|p| p == phoneme)
    }

    /// Given a phoneme index, return its name as a string.
    #[inline]
    pub fn phoneme_index_to_phoneme(&self, index: usize) -> Option<&str> {
        self.phonemes.get(index).map(|s| s.as_str())
    }

    /// Return the number of segments in the voice.
    #[inline]
    pub fn num_segments(&self) -> usize {
        self.segments.len()
    }

    /// Given a segment name, return its index. Return None if not found.
    /// Not real-time safe.
    pub fn segment_to_segment_index(&self, segment: &str) -> Option<usize> {
        self.segments
            .iter()
            .position(|s| s == segment)
    }

    /// Given a segment index, return its name.
    #[inline]
    pub fn segment_index_to_segment(&self, index: usize) -> Option<&str> {
        self.segments
            .get(index)
            .map(|s| s.as_str())
    }

    /// Given a segment index, return the number of frames (aka grains).
    #[inline]
    pub fn segment_num_frames(&self, index: usize) -> i32 {
        if index >= self.segments_num_frames.len() {
            return 0;
        }
        self.segments_num_frames[index]
    }

    /// Return true if the segment is a vowel monophone.
    #[inline]
    pub fn segment_is_vowel(&self, index: usize) -> bool {
        if index >= self.segments_is_vowel.len() {
            return false;
        }
        self.segments_is_vowel[index]
    }

    /// Return the offset of the segment into the wavetable memory.
    #[inline]
    pub fn segment_offset(&self, index: usize) -> i32 {
        if index >= self.segments_offset.len() {
            return 0;
        }
        self.segments_offset[index]
    }

    /// Get the index of the silent segment, _.
    #[inline]
    pub fn silent_segment_index(&self) -> i32 {
        self.silent_segment_index
    }

    /// Convert a list of phonemes to a list of segment indices.
    ///
    /// This implements the diphone concatenation logic:
    /// - Vowels are stored as monophones in the database
    /// - Consonant-vowel transitions are stored as diphones
    /// - Missing diphones fall back to segment-level substitution
    pub fn convert_phonemes_to_segment_indices(
        &self,
        phonemes: &[&str],
    ) -> Vec<i32> {
        let mut segment_indices = Vec::new();
        let num_phonemes = phonemes.len();

        for i in 0..num_phonemes {
            let phoneme1 = phonemes[i];
            let phoneme2 = if i + 1 == num_phonemes {
                "_"
            } else {
                phonemes[i + 1]
            };

            // If phoneme1 is a vowel, it is present in the segments database as a monophone.
            if let Some(idx) = self.segment_to_segment_index(phoneme1) {
                segment_indices.push(idx as i32);
            }

            // Construct diphone with string concatenation.
            let diphone = format!("{}{}", phoneme1, phoneme2);
            if let Some(idx) = self.segment_to_segment_index(&diphone) {
                segment_indices.push(idx as i32);
            } else {
                // If the diphone is NOT in the database, fall back.
                // Special cases for diphthong transitions
                let is_diphthong_front =
                    phoneme1 == "aI" || phoneme1 == "eI" || phoneme1 == "OI";
                if is_diphthong_front
                    && self
                        .segment_to_segment_index(&format!("{}j", phoneme1))
                        .is_some()
                {
                    let idx = self
                        .segment_to_segment_index(&format!("{}j", phoneme1))
                        .unwrap();
                    segment_indices.push(idx as i32);
                }

                let is_diphthong_back = phoneme1 == "aU" || phoneme1 == "oU";
                if is_diphthong_back
                    && self
                        .segment_to_segment_index(&format!("{}w", phoneme1))
                        .is_some()
                {
                    let idx = self
                        .segment_to_segment_index(&format!("{}w", phoneme1))
                        .unwrap();
                    segment_indices.push(idx as i32);
                }

                // Add a transition from silence into the second phoneme.
                let silence_transition = format!("_{}", phoneme2);
                if let Some(idx) =
                    self.segment_to_segment_index(&silence_transition)
                {
                    segment_indices.push(idx as i32);
                }
            }
        }

        // Strip out repeated copies of "_".
        let mut segment_indices_pass2 = Vec::new();
        let mut last_segment_index: i32 = -1; // k_noSegment equivalent
        let silent_idx = self.silent_segment_index;
        for (i, segment_index) in segment_indices.iter().enumerate() {
            if !(*segment_index == silent_idx
                && *segment_index == last_segment_index)
                && !(*segment_index == silent_idx && i == 0)
                && !(*segment_index == silent_idx
                    && i == segment_indices.len() - 1)
            {
                segment_indices_pass2.push(*segment_index);
            }
            last_segment_index = *segment_index;
        }

        segment_indices_pass2
    }
}