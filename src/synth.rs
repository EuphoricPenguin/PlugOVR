/// Real-time audio synthesis for the OddVoices synthesizer.
///
/// The Synth orchestrates grain scheduling, segment queue consumption,
/// crossfade transitions, vowel looping, and all the note-on/note-off/phoneme-speed
/// formant-shift logic.

use crate::deque::Deque;
use crate::grain::Grain;
use crate::pitch::Pitch;
use crate::voice::Voice;

/// The maximum number of grains.
const MAX_GRAINS: usize = 10;

/// A synthesizer that uses a Voice to synthesize sound.
///
/// The associated Voice, which is passed in at initialization as a reference,
/// must live at least as long as the Synth does.
///
/// If the Voice is not initialized, the Synth permanently outputs silence.
#[allow(dead_code)]
pub struct Synth<'a> {
    is_errored: bool,

    sample_rate: f32,
    voice: &'a Voice,
    silent_segment_index: i32,
    segment: i32,
    old_segment: i32,
    segment_queue: Deque<i32>,

    pitch: Pitch,

    phase: f32,
    phoneme_speed: f32,
    formant_shift: f32,

    max_grains: usize,
    next_grain: usize,
    grains: [Grain; MAX_GRAINS],

    original_f0: f32,

    note_on: bool,
    note_off: bool,

    syllable_duration: f32,
    syllable_time_remaining: f32,
    syllable_segments_remaining: i32,

    segment_time: f32,
    segment_length: f32,
    old_segment_time: f32,
    old_segment_length: f32,

    crossfade: f32,
    crossfade_ramp: f32,
    crossfade_length: f32,

    pitch_reset_time: f32,
    pitch_reset_time_remaining: f32,
}

impl<'a> Synth<'a> {
    /// Create a new synthesizer.
    ///
    /// # Arguments
    /// * `sample_rate` — Sample rate of the synth (independent of the Voice's sample rate).
    /// * `voice` — Pointer/reference to a Voice. The Voice must live at least as long as the Synth.
    /// * `segment_queue_memory` — Memory for the segment queue.
    /// * `segment_queue_capacity` — Size of segment_queue_memory in ints.
    /// * `segment_queue_start` — Initial starting position of the segment queue.
    /// * `segment_queue_size` — Initial size of the segment queue.
    pub fn new(
        sample_rate: f32,
        voice: &'a Voice,
        segment_queue_memory: Box<[i32]>,
        segment_queue_capacity: usize,
        segment_queue_start: usize,
        segment_queue_size: usize,
    ) -> Self {
        // Determine segment queue capacity (min of provided capacity and memory length)
        let _queue_capacity = if segment_queue_capacity < segment_queue_memory.len() {
            segment_queue_capacity
        } else {
            segment_queue_memory.len()
        };

        // Create the deque with a "no value" sentinel (using the voice's k_noSegment equivalent)
        let no_value = -1i32;
        let segment_queue = Deque::new(segment_queue_memory, segment_queue_start, segment_queue_size, no_value);

        if !voice.has_init_started() || !voice.has_init_finished() {
            return Synth {
                is_errored: true,
                sample_rate,
                voice,
                silent_segment_index: 0,
                segment: 0,
                old_segment: 0,
                segment_queue,
                pitch: Pitch::new(sample_rate),
                phase: 1.0,
                phoneme_speed: 1.0,
                formant_shift: 1.0,
                max_grains: MAX_GRAINS,
                next_grain: 0,
                grains: Self::make_grains(voice),
                original_f0: 0.0,
                note_on: false,
                note_off: false,
                syllable_duration: 0.0,
                syllable_time_remaining: 0.0,
                syllable_segments_remaining: 0,
                segment_time: 0.0,
                segment_length: 0.0,
                old_segment_time: 0.0,
                old_segment_length: 0.0,
                crossfade: 0.0,
                crossfade_ramp: 0.0,
                crossfade_length: 0.03,
                pitch_reset_time: 0.01,
                pitch_reset_time_remaining: 0.0,
            };
        }

        let silent_segment_index = voice.silent_segment_index();
        let original_f0 = (voice.sample_rate() as f32) / (0.5 * voice.grain_length() as f32);

        Synth {
            is_errored: false,
            sample_rate,
            voice,
            silent_segment_index,
            segment: silent_segment_index,
            old_segment: silent_segment_index,
            segment_queue,
            pitch: Pitch::new(sample_rate),
            phase: 1.0,
            phoneme_speed: 1.0,
            formant_shift: 1.0,
            max_grains: MAX_GRAINS,
            next_grain: 0,
            grains: Self::make_grains(voice),
            original_f0,
            note_on: false,
            note_off: false,
            syllable_duration: 0.0,
            syllable_time_remaining: 0.0,
            syllable_segments_remaining: 0,
            segment_time: 0.0,
            segment_length: 0.0,
            old_segment_time: 0.0,
            old_segment_length: 0.0,
            crossfade: 0.0,
            crossfade_ramp: 0.0,
            crossfade_length: 0.03,
            pitch_reset_time: 0.01,
            pitch_reset_time_remaining: 0.0,
        }
    }

    fn make_grains(voice: &Voice) -> [Grain; MAX_GRAINS] {
        let mem = voice.wavetable_memory();
        let gl = voice.grain_length() as usize;
        let mut grains = [Grain::new(), Grain::new(), Grain::new(), Grain::new(), Grain::new(),
                           Grain::new(), Grain::new(), Grain::new(), Grain::new(), Grain::new()];
        for grain in &mut grains {
            grain.set_wavetable_memory(mem);
            grain.set_grain_length(gl);
        }
        grains
    }

    /// Return true if the Synth encountered an error during initialization.
    pub fn is_errored(&self) -> bool {
        self.is_errored
    }

    /// Compute one sample of output.
    pub fn process(&mut self) -> i32 {
        // If an error occurred during initialization, return silence.
        if self.is_errored {
            return 0;
        }

        let dt = 1.0 / self.sample_rate;

        self.syllable_time_remaining -= dt;

        if !self.is_active() {
            if !self.note_on {
                // If the synth has been inactive for more than pitch_reset_time seconds,
                // set the pitch module's frequency to zero so that there is no portamento
                // leading into the next note.
                self.pitch_reset_time_remaining -= dt;
                if self.pitch_reset_time_remaining <= 0.0 {
                    self.pitch.set_frequency_immediate(0.0);
                }

                // If the synth is inactive and there is no noteOn message, return silence.
                return 0;
            } else {
                // If the synth is inactive and there has been a noteOn call...
                // a. if the segment queue is empty, return silence.
                // b. if the segment queue is not empty, start a new syllable.
                if self.segment_queue.empty() {
                    return 0;
                } else {
                    self.new_syllable();
                }
            }
        } else {
            self.pitch_reset_time_remaining = self.pitch_reset_time;

            // If the synth is active and there has been a noteOn call, start a new syllable.
            if self.note_on {
                self.new_syllable();
            }

            // If the synth is active and there are pending note offs, AND we are currently
            // playing a vowel, then proceed to the next segment.
            if self.note_off && self.voice.segment_is_vowel(self.segment as usize) {
                self.new_segment();
            }

            // If the synth is active, and the current segment is a vowel, and we need to
            // start the final consonant cluster, start a new segment.
            // BUT, if the current syllable duration is <= 0, meaning that we have an indefinite hold,
            // don't do this.
            if self.voice.segment_is_vowel(self.segment as usize)
                && self.syllable_time_remaining <= 0.0
                && !(self.syllable_duration <= 0.0)
            {
                self.new_segment();
            }

            // If the synth is active and we have reached the end of the current segment (or
            // rather the beginning of the crossfade of the next segment)...
            // a. if the segment is a vowel, loop back to the beginning.
            // b. if the segment is not a vowel, proceed to the next segment.
            if self.segment_time >= self.segment_length - self.crossfade_length {
                if self.voice.segment_is_vowel(self.segment as usize) {
                    self.segment_time = 0.0;
                } else {
                    self.new_segment();
                }
            }
        }

        self.note_on = false;
        self.note_off = false;

        if self.phase >= 1.0 {
            self.phase -= 1.0;

            let offset = self.get_offset(self.segment, self.segment_time);
            let old_offset = self.get_offset(self.old_segment, self.old_segment_time);
            let rate = (self.voice.sample_rate() as f32 / self.sample_rate) * self.formant_shift;

            // Determine crossfade offset
            let crossfade_offset = if self.old_segment == self.silent_segment_index {
                -1i32
            } else {
                old_offset as i32
            };

            self.grains[self.next_grain]
                .play(offset as i32, crossfade_offset, self.crossfade, rate);
            self.next_grain = (self.next_grain + 1) % self.max_grains;
        }

        let segment_time_per_sample = self.phoneme_speed / self.sample_rate;
        self.segment_time += segment_time_per_sample;
        self.old_segment_time += segment_time_per_sample;
        self.crossfade = (self.crossfade + self.crossfade_ramp * self.phoneme_speed).max(0.0);
        self.phase += self.pitch.process() / self.sample_rate;

        let mut result: i32 = 0;
        for i in 0..self.max_grains {
            result += self.grains[i].process() as i32;
        }

        // Clamp to i32 range (int32 in C++ code)
        if result > i32::MAX {
            result = i32::MAX;
        } else if result < i32::MIN {
            result = i32::MIN;
        }

        result
    }

    /// Trigger a note on event.
    ///
    /// # Arguments
    /// * `syllable_duration` — The length of the note, or rather the melisma. If this
    ///   is <= 0, the note is held indefinitely until a noteOff event is received. Otherwise
    ///   the note has finite length and all noteOn and noteOff events will be ignored
    ///   for the next syllable_duration seconds.
    pub fn note_on(&mut self, syllable_duration: f32) {
        self.syllable_duration = syllable_duration;
        self.note_on = true;
    }

    /// Trigger a note on event of indefinite length.
    pub fn note_on_indefinite(&mut self) {
        self.note_on(0.0);
    }

    /// Trigger a note off event.
    pub fn note_off(&mut self) {
        self.note_off = true;
    }

    /// Return true if the Synth is currently playing a segment, and false otherwise.
    /// A Synth will automatically go inactive when the queue runs out and the final
    /// segment finishes playing.
    pub fn is_active(&self) -> bool {
        self.segment != self.silent_segment_index
    }

    /// Immediately set fundamental frequency in Hertz.
    pub fn set_frequency_immediate(&mut self, frequency: f32) {
        self.pitch.set_frequency_immediate(frequency);
    }

    /// Ramp to a new fundamental frequency in Hertz.
    pub fn set_target_frequency(&mut self, frequency: f32) {
        self.pitch.set_target_frequency(frequency);
    }

    /// Add a segment to the queue. The segment is given by index in the Voice.
    pub fn queue_segment(&mut self, segment: i32) {
        self.segment_queue.push_back(segment);
    }

    /// Set formant shift, given as a ratio. 1 is default.
    pub fn set_formant_shift(&mut self, formant_shift: f32) {
        self.formant_shift = formant_shift;
    }

    /// Set speed of phonemes, given as a ratio. 1 is default.
    pub fn set_phoneme_speed(&mut self, phoneme_speed: f32) {
        self.phoneme_speed = phoneme_speed;
    }

    /// Start a new syllable (internal).
    fn new_syllable(&mut self) {
        // Clear out any "_" at the beginning.
        while !self.segment_queue.empty()
            && self.segment_queue.front() == self.silent_segment_index
        {
            self.segment_queue.pop_front();
        }

        if self.syllable_duration <= 0.0 {
            self.new_segment();
            return;
        }

        let mut consonant_duration = 0.0f32;
        let mut i = 0usize;

        // Count consonant duration before the vowel.
        loop {
            if i >= self.segment_queue.size() {
                break;
            }
            let segment_index = self.segment_queue.get(i) as i32;
            if self.voice.segment_is_vowel(segment_index as usize) {
                break;
            }
            let seg_frames = self.voice.segment_num_frames(segment_index as usize) as f32;
            consonant_duration += seg_frames / self.original_f0 - self.crossfade_length;
            i += 1;
        }

        // Skip over the vowel.
        i += 1;
        let consonant_cluster_first_index = i;
        let mut consonant_cluster_size_in_segments = 0usize;
        let mut upcoming_vowel_is_silence = false;

        // Count final consonants.
        loop {
            if i >= self.segment_queue.size() {
                break;
            }
            let segment_index = self.segment_queue.get(i) as i32;
            if segment_index == self.silent_segment_index {
                upcoming_vowel_is_silence = true;
                break;
            }
            if self.voice.segment_is_vowel(segment_index as usize) {
                break;
            }
            consonant_cluster_size_in_segments += 1;
            i += 1;
        }

        let mut final_consonant_duration = 0.0f32;
        if !upcoming_vowel_is_silence {
            consonant_cluster_size_in_segments /= 2;
        }

        for j in 0..consonant_cluster_size_in_segments {
            let segment_index = self.segment_queue.get(consonant_cluster_first_index + j) as i32;
            let seg_frames = self.voice.segment_num_frames(segment_index as usize) as f32;
            final_consonant_duration += seg_frames / self.original_f0 - self.crossfade_length;
        }

        consonant_duration += final_consonant_duration;

        if consonant_duration > self.syllable_duration {
            self.phoneme_speed = consonant_duration / self.syllable_duration;
        } else {
            self.phoneme_speed = 1.0;
        }

        self.syllable_time_remaining =
            self.syllable_duration - final_consonant_duration / self.phoneme_speed;

        self.new_segment();
    }

    /// Start a new segment (internal).
    fn new_segment(&mut self) {
        if self.segment_queue.empty() {
            self.segment = self.silent_segment_index;
            self.segment_time = 0.0;
            self.segment_length = 0.0;
            return;
        }

        self.old_segment = self.segment;
        self.old_segment_time = self.segment_time;

        self.segment = self.segment_queue.front();
        self.segment_queue.pop_front();
        self.segment_time = 0.0;
        self.segment_length =
            self.voice.segment_num_frames(self.segment as usize) as f32 / self.original_f0;


        if self.old_segment == self.silent_segment_index {
            self.crossfade = 0.0;
            self.crossfade_ramp = 0.0;
        } else {
            self.crossfade = 1.0;
            self.crossfade_ramp =
                -1.0 / (self.crossfade_length * self.sample_rate);
        }
    }

    /// Get the wavetable offset for a segment at a given time.
    fn get_offset(&self, segment: i32, segment_time: f32) -> usize {
        let segment_num_frames = self.voice.segment_num_frames(segment as usize);
        if segment_num_frames == 0 {
            return 0;
        }
        let frame_index = (segment_time * self.original_f0) as i32;
        let frame_index = if frame_index < 0 {
            0
        } else {
            frame_index % segment_num_frames
        };
        let segment_offset = self.voice.segment_offset(segment as usize) as usize;
        let offset = segment_offset
            + (frame_index as usize) * (self.voice.grain_length() as usize);
        offset
    }
}