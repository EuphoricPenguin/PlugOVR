//! OddVoices VST3/CLAP plugin implementation.
//!
//! This module implements the nih-plug Plugin trait, wrapping the OddVoices
//! PSOLA synthesizer as a real-time audio plugin with MIDI input.

use nih_plug::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::g2p::G2P;
use crate::synth::Synth;
use crate::voice::Voice;

/// Plugin parameters.
#[derive(Params)]
pub struct OddVoicesParams {
    /// The editor state (persisted by the DAW).
    #[persist = "editor-state"]
    editor_state: Arc<nih_plug_egui::EguiState>,

    /// Master gain in dB.
    #[id = "gain"]
    pub gain: FloatParam,

    /// Vibrato frequency in Hz.
    #[id = "vibrato_freq"]
    pub vibrato_frequency: FloatParam,

    /// Vibrato depth (0-1).
    #[id = "vibrato_depth"]
    pub vibrato_depth: FloatParam,

    /// Portamento time in seconds.
    #[id = "portamento"]
    pub portamento_time: FloatParam,

    /// Reset trigger - toggle this to reset the synth's internal state.
    /// Automate this to match your DAW's transport start to keep
    /// phoneme timing in sync when stopping/starting playback.
    #[id = "reset"]
    pub reset: BoolParam,
}

impl Default for OddVoicesParams {
    fn default() -> Self {
        Self {
            editor_state: nih_plug_egui::EguiState::from_size(500, 450),
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            vibrato_frequency: FloatParam::new(
                "Vibrato Freq",
                5.0,
                FloatRange::Linear { min: 0.0, max: 20.0 },
            )
            .with_unit(" Hz"),
            vibrato_depth: FloatParam::new(
                "Vibrato Depth",
                0.02,
                FloatRange::Linear { min: 0.0, max: 0.1 },
            ),
            portamento_time: FloatParam::new(
                "Portamento",
                0.1,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_unit(" s"),
            reset: BoolParam::new("Reset", false),
        }
    }
}

/// Shared state between the editor and audio threads.
pub struct SharedState {
    /// Lyrics text.
    pub lyrics: Mutex<String>,
    /// Pending voice path to load (set by editor, consumed by audio thread).
    pub pending_voice: Mutex<Option<PathBuf>>,
    /// Currently loaded voice name (for the editor to display).
    pub current_voice: Mutex<String>,
    /// Available voice names (discovered at startup).
    pub available_voices: Mutex<Vec<String>>,
    /// Reset flag - set by editor button, checked and cleared by audio thread.
    pub reset_requested: AtomicBool,
}

impl SharedState {
    fn new() -> Self {
        let voices = discover_voices();
        Self {
            lyrics: Mutex::new(String::new()),
            pending_voice: Mutex::new(None),
            current_voice: Mutex::new(String::new()),
            available_voices: Mutex::new(voices),
            reset_requested: AtomicBool::new(false),
        }
    }
}

/// Discover .voice files in the compiled_voices directory.
fn discover_voices() -> Vec<String> {
    let paths = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("compiled_voices"))),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin").join("compiled_voices")),
    ];

    let mut voices = Vec::new();
    for base in paths.iter().flatten() {
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("voice") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        if !voices.contains(&name.to_string()) {
                            voices.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    voices.sort();
    voices
}

/// The OddVoices plugin.
pub struct OddVoices {
    params: Arc<OddVoicesParams>,

    /// Shared state between editor and audio thread.
    shared: Arc<SharedState>,

    /// The loaded voice (None if not yet loaded).
    voice: Option<Voice>,

    /// The synthesizer instance, stored as raw pointer to work around lifetime issues.
    /// The Synth borrows the Voice, but since both are owned by this struct and the
    /// Voice is never moved after the Synth is created, this is safe.
    synth: Option<Box<SynthManager>>,

    /// G2P converter for lyrics-to-phonemes.
    g2p: Option<G2P>,

    /// Sample rate.
    sample_rate: f32,

    /// Current active MIDI note.
    active_note: Option<u8>,

    /// Current note frequency.
    note_freq: f32,

    /// Whether a note is currently being held.
    note_on: bool,

    /// The last lyrics we processed (to detect changes).
    last_lyrics: String,

    /// Previous value of the reset parameter (to detect rising edge).
    last_reset: bool,

    /// Current syllable index.
    syllable_index: usize,

    /// Pre-computed segment indices for each syllable/word in the lyrics.
    /// Rebuilt whenever lyrics change.
    syllable_segments: Vec<Vec<i32>>,

    /// Global sample counter for calculating note durations.
    sample_counter: u64,

    /// Sample position when the current note started (for duration calculation).
    note_on_sample: u64,
}

/// Manages the Synth and its relationship with the Voice.
/// The Synth borrows the Voice, so we use unsafe to manage the lifetime.
struct SynthManager {
    /// The segment queue memory.
    segment_queue_memory: Box<[i32]>,
    /// Queue capacity.
    queue_capacity: usize,
    /// The synth, with an extended lifetime.
    /// SAFETY: The synth borrows a Voice that is owned by the parent OddVoices struct.
    /// The Voice is never moved or dropped while this Synth exists.
    synth: Synth<'static>,
}

impl Default for OddVoices {
    fn default() -> Self {
        Self {
            params: Arc::new(OddVoicesParams::default()),
            shared: Arc::new(SharedState::new()),
            voice: None,
            synth: None,
            g2p: None,
            sample_rate: 44100.0,
            active_note: None,
            note_freq: 440.0,
            note_on: false,
            last_lyrics: String::new(),
            last_reset: false,
            syllable_index: 0,
            syllable_segments: Vec::new(),
            sample_counter: 0,
            note_on_sample: 0,
        }
    }
}

impl Plugin for OddVoices {
    const NAME: &'static str = "OddVoices";
    const VENDOR: &'static str = "OddVoices Project";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = config.sample_rate;

        // Load the G2P dictionary from the mpron.txt file
        let mpron_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("oddvoices")
            .join("cmudict-0.7b");
        if mpron_path.exists() {
            self.g2p = Some(G2P::load(mpron_path.to_str().unwrap_or("")));
        }

        true
    }

    fn reset(&mut self) {
        self.active_note = None;
        self.note_on = false;
        self.synth = None;
        self.syllable_index = 0;
        self.sample_counter = 0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // ── Voice loading ──
        if let Some(voice_path) = self.shared.pending_voice.lock().unwrap().take() {
            let mut voice = Voice::new();
            voice.init_from_file(voice_path.to_str().unwrap_or(""));
            if voice.has_init_finished() {
                self.voice = Some(voice);
                self.synth = None; // Will be re-initialized on next process
            }
        }

        // ── Check for lyrics changes and rebuild syllable segments ──
        let current_lyrics = self.shared.lyrics.lock().unwrap().clone();
        let lyrics_changed = current_lyrics != self.last_lyrics;
        if lyrics_changed {
            self.last_lyrics = current_lyrics.clone();
            self.rebuild_syllable_segments(&current_lyrics);
        }

        // ── Initialize synth if we have a voice but no synth yet ──
        if self.synth.is_none() {
            if let Some(voice) = &self.voice {
                if voice.has_init_finished() {
                    let queue_size = 256;
                    let mem: Box<[i32]> = vec![-1; queue_size].into_boxed_slice();

                    // SAFETY: The voice is owned by this struct and will outlive the synth.
                    // We extend the lifetime to 'static because the synth is stored alongside
                    // the voice in the same struct, and the synth is dropped before the voice.
                    let voice_ref: &Voice = voice;
                    let voice_static: &'static Voice = unsafe { &*(voice_ref as *const Voice) };

                    let synth = Synth::new(
                        self.sample_rate,
                        voice_static,
                        vec![-1; queue_size].into_boxed_slice(),
                        queue_size,
                        0,
                        0,
                    );
                    if !synth.is_errored() {
                        self.synth = Some(Box::new(SynthManager {
                            segment_queue_memory: mem,
                            queue_capacity: queue_size,
                            synth,
                        }));
                    }
                }
            }
        }

        // ── Handle reset trigger ──
        // Check both the BoolParam (for DAW automation) and the AtomicBool flag
        // (for the editor button). The AtomicBool is a one-shot flag set by the
        // editor and cleared here after processing.
        let reset_value = self.params.reset.value();
        let reset_requested = self.shared.reset_requested.swap(false, Ordering::Relaxed);
        if reset_value || reset_requested {
            // Reset is active — keep syllable_index at 0 so the next note-on
            // will play the first syllable/word in the lyrics. While the reset
            // automation is HIGH, every note-on will play the first word.
            // When reset goes LOW, syllable_index will advance normally.
            self.syllable_index = 0;
        }
        self.last_reset = reset_value;

        // ── Process MIDI events ──
        let mut next_event = context.next_event();

        for (sample_idx, channel_samples) in buffer.iter_samples().enumerate() {
            // Process MIDI events at their exact timing
            while let Some(event) = next_event {
                if event.timing() > sample_idx as u32 {
                    break;
                }

                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        if velocity > 0.0 {
                            self.handle_note_on(note, &current_lyrics);
                        } else {
                            self.handle_note_off(note);
                        }
                    }
                    NoteEvent::NoteOff { note, .. } => {
                        self.handle_note_off(note);
                    }
                    NoteEvent::MidiPitchBend { value, .. } => {
                        let bend_semitones = (value - 0.5) * 4.0;
                        if let Some(base_note) = self.active_note {
                            let bent_note = base_note as f32 + bend_semitones;
                            self.note_freq = util::f32_midi_note_to_freq(bent_note);
                            if let Some(sm) = &mut self.synth {
                                sm.synth.set_target_frequency(self.note_freq);
                            }
                        }
                    }
                    _ => {}
                }

                next_event = context.next_event();
            }

            // ── Generate audio ──
            let output = if let Some(sm) = &mut self.synth {
                let raw = sm.synth.process();
                // Convert i16-range i32 to f32 (-1.0 to 1.0)
                (raw as f32) / (i16::MAX as f32)
            } else {
                0.0
            };

            let gain = self.params.gain.smoothed.next();
            let output = output * gain;

            for sample in channel_samples {
                *sample = output;
            }

            self.sample_counter += 1;
        }

        ProcessStatus::KeepAlive
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let editor_state = params.editor_state.clone();
        let shared = self.shared.clone();
        crate::editor::create_editor(params, editor_state, shared)
    }
}

impl OddVoices {
    /// Rebuild the pre-computed segment indices for each syllable/word in the lyrics.
    /// This is called whenever the lyrics text changes.
    fn rebuild_syllable_segments(&mut self, lyrics: &str) {
        self.syllable_segments.clear();

        if lyrics.is_empty() || self.g2p.is_none() || self.voice.is_none() {
            return;
        }

        let g2p = self.g2p.as_ref().unwrap();
        let voice = self.voice.as_ref().unwrap();

        // Split lyrics by whitespace or hyphens into syllables/words
        let syllables: Vec<&str> = lyrics.split(&[' ', '-'][..]).filter(|s| !s.is_empty()).collect();

        for syllable in &syllables {
            let phonemes = g2p.pronounce(syllable);
            let phoneme_strs: Vec<&str> = phonemes.iter().map(|s| s.as_str()).collect();
            let segments = voice.convert_phonemes_to_segment_indices(&phoneme_strs);
            self.syllable_segments.push(segments);
        }
    }

    fn handle_note_on(&mut self, note: u8, lyrics: &str) {
        self.active_note = Some(note);
        self.note_freq = util::midi_note_to_freq(note);
        self.note_on = true;
        self.note_on_sample = self.sample_counter;

        if let Some(sm) = &mut self.synth {
            // Set frequency
            sm.synth.set_frequency_immediate(self.note_freq);

            // Clear any remaining segments from the previous syllable
            // before queuing the new syllable's segments.
            sm.synth.clear_queue();

            // Use pre-computed syllable segments if available, otherwise
            // fall back to on-the-fly conversion
            if !self.syllable_segments.is_empty() {
                // Loop syllables if we've reached the end
                if self.syllable_index >= self.syllable_segments.len() {
                    self.syllable_index = 0;
                }

                let segments = &self.syllable_segments[self.syllable_index];

                // Queue all segments for this syllable
                for seg in segments {
                    sm.synth.queue_segment(*seg);
                }

                self.syllable_index += 1;
            } else if !lyrics.is_empty() {
                // Fallback: on-the-fly conversion (shouldn't normally be needed)
                if let Some(g2p) = &self.g2p {
                    let syllables: Vec<&str> = lyrics.split(&[' ', '-'][..]).filter(|s| !s.is_empty()).collect();

                    if !syllables.is_empty() {
                        if self.syllable_index >= syllables.len() {
                            self.syllable_index = 0;
                        }

                        let syllable = syllables[self.syllable_index];
                        let phonemes = g2p.pronounce(syllable);

                        if let Some(voice) = &self.voice {
                            let phoneme_strs: Vec<&str> = phonemes.iter().map(|s| s.as_str()).collect();
                            let segments = voice.convert_phonemes_to_segment_indices(&phoneme_strs);

                            for seg in &segments {
                                sm.synth.queue_segment(*seg);
                            }
                        }

                        self.syllable_index += 1;
                    }
                }
            }

            // Trigger note on with a finite duration based on the note length.
            // We use a default duration of 0.5 seconds, which will be refined
            // when the note-off event arrives. The synth's new_syllable() will
            // calculate phoneme_speed to fit the syllable within this duration.
            //
            // Using a finite duration (rather than indefinite) makes the synth
            // behave like the original OddVoices: the vowel plays for the
            // specified time, then automatically transitions to the final
            // consonant cluster, and then advances to the next segment.
            // This ensures the entire word is played within the note's duration.
            sm.synth.note_on(0.5);
        }
    }

    fn handle_note_off(&mut self, note: u8) {
        if self.active_note == Some(note) {
            self.active_note = None;
            self.note_on = false;

            if let Some(sm) = &mut self.synth {
                // Signal note-off so the current vowel transitions to
                // the remaining consonant segments naturally. This ensures
                // the final part of each word/syllable is not cut off.
                // We do NOT clear the queue here — the remaining segments
                // (e.g. final consonants after a vowel) need to play out.
                sm.synth.note_off();
            }
        }
    }
}

impl Vst3Plugin for OddVoices {
    const VST3_CLASS_ID: [u8; 16] = *b"OddVoicesVST3X!!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Synth, Vst3SubCategory::Mono];
}

impl ClapPlugin for OddVoices {
    const CLAP_ID: &'static str = "com.oddvoices.plugin";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("OddVoices singing synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Mono,
    ];
}

nih_export_vst3!(OddVoices);
nih_export_clap!(OddVoices);
