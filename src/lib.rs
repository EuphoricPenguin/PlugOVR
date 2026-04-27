// OddVoices synthesizer — VST3/CLAP plugin
//
// This crate provides a PSOLA (Pitch Synchronous Overlap Add) singing
// synthesizer for General American English, wrapped as a VST3/CLAP plugin
// using nih-plug.

pub mod deque;
pub mod editor;
pub mod g2p;
pub mod grain;
pub mod mpron;
pub mod pitch;
pub mod plugin;
pub mod synth;
pub mod voice;

// Re-export public API
pub use deque::Deque;
pub use g2p::G2P;
pub use grain::Grain;
pub use mpron::load_dictionary;
pub use pitch::Pitch;
pub use plugin::OddVoices;
pub use synth::Synth;
pub use voice::Voice;

#[cfg(test)]
mod tests {
    use crate::deque::Deque;
    use crate::pitch::Pitch;
    use crate::grain::Grain;
    use crate::voice::Voice;

    // ===== Deque Tests =====

    #[test]
    fn test_deque_new_empty() {
        let mem: Box<[i32]> = Box::new([0; 16]);
        let dq: Deque<i32> = Deque::new(mem, 0, 0, -1);
        assert_eq!(dq.size(), 0);
        assert!(dq.empty());
        assert_eq!(dq.front(), -1); // no_value when empty
    }

    #[test]
    fn test_deque_push_pop() {
        let mem: Box<[i32]> = Box::new([0; 16]);
        let mut dq: Deque<i32> = Deque::new(mem, 0, 0, -1);
        dq.push_back(10);
        dq.push_back(20);
        dq.push_back(30);
        assert_eq!(dq.size(), 3);
        assert!(!dq.empty());
        assert_eq!(dq.front(), 10);
        assert_eq!(dq.get(0), 10);
        assert_eq!(dq.get(1), 20);
        assert_eq!(dq.get(2), 30);

        dq.pop_front();
        assert_eq!(dq.size(), 2);
        assert_eq!(dq.front(), 20);
        assert_eq!(dq.get(0), 20);
        assert_eq!(dq.get(1), 30);
    }

    #[test]
    fn test_deque_wrap_around() {
        let mem: Box<[i32]> = Box::new([0; 4]);
        let mut dq: Deque<i32> = Deque::new(mem, 0, 0, -1);
        // Fill and drain to force wrap-around
        dq.push_back(1);
        dq.push_back(2);
        dq.pop_front();
        dq.pop_front();
        // Now push more to wrap around
        dq.push_back(3);
        dq.push_back(4);
        dq.push_back(5);
        dq.push_back(6);
        assert_eq!(dq.get(0), 3);
        assert_eq!(dq.get(1), 4);
        assert_eq!(dq.get(2), 5);
        assert_eq!(dq.get(3), 6);
        assert_eq!(dq.size(), 4);
    }

    #[test]
    fn test_deque_get_out_of_bounds() {
        let mem: Box<[i32]> = Box::new([0; 16]);
        let dq: Deque<i32> = Deque::new(mem, 0, 0, -1);
        assert_eq!(dq.get(0), -1); // no_value
        assert_eq!(dq.get(999), -1);
    }

    #[test]
    fn test_deque_peek() {
        let mem: Box<[i32]> = Box::new([0; 16]);
        let mut dq: Deque<i32> = Deque::new(mem, 0, 0, -1);
        dq.push_back(42);
        dq.push_back(99);
        assert_eq!(dq.peek(0), 42);
        assert_eq!(dq.peek(1), 99);
        assert_eq!(dq.peek(5), -1); // out of bounds
    }

    #[test]
    fn test_deque_capacity_limit() {
        let mem: Box<[i32]> = Box::new([0; 4]);
        let mut dq: Deque<i32> = Deque::new(mem, 0, 0, -1);
        dq.push_back(1);
        dq.push_back(2);
        dq.push_back(3);
        dq.push_back(4);
        assert_eq!(dq.size(), 4);
        // This should be dropped (full)
        dq.push_back(5);
        assert_eq!(dq.size(), 4); // size unchanged
        assert_eq!(dq.get(3), 4); // 5 never got in
    }

    // ===== Pitch Tests =====

    #[test]
    fn test_pitch_new_silent() {
        let mut pitch = Pitch::new(48000.0);
        // Should start in silent state (frequency 0)
        for _ in 0..100 {
            let freq = pitch.process();
            assert_eq!(freq, 0.0);
        }
    }

    #[test]
    fn test_pitch_set_frequency_immediate() {
        let mut pitch = Pitch::new(48000.0);
        pitch.set_frequency_immediate(440.0);
        // After setting frequency, process should return non-zero
        let freq = pitch.process();
        assert!(freq > 0.0);
        // Should be approximately 440 Hz (with small modulation from jitter/vibrato)
        assert!(freq >= 430.0 && freq <= 450.0);
    }

    #[test]
    fn test_pitch_set_zero_frequency() {
        let mut pitch = Pitch::new(48000.0);
        pitch.set_frequency_immediate(440.0);
        // Verify it's producing sound
        assert!(pitch.process() > 0.0);
        // Set to zero should go silent
        pitch.set_frequency_immediate(0.0);
        assert_eq!(pitch.process(), 0.0);
    }

    #[test]
    fn test_pitch_process_consistency() {
        let mut pitch = Pitch::new(48000.0);
        pitch.set_frequency_immediate(220.0);
        for _ in 0..1000 {
            let freq = pitch.process();
            // Frequency should stay roughly in the 220 Hz range
            // (small variations from jitter and drift are expected)
            assert!(freq > 200.0 && freq < 240.0, "Unexpected frequency: {freq}");
        }
    }

    #[test]
    fn test_pitch_vibrato_params() {
        let mut pitch = Pitch::new(48000.0);
        pitch.set_frequency_immediate(440.0);
        pitch.set_vibrato_frequency(6.0);
        pitch.set_vibrato_max_amplitude(0.03);
        // Should not panic
        for _ in 0..100 {
            pitch.process();
        }
    }

    #[test]
    fn test_pitch_jitter_params() {
        let mut pitch = Pitch::new(48000.0);
        pitch.set_frequency_immediate(440.0);
        pitch.set_jitter_amplitude(0.01);
        for _ in 0..100 {
            pitch.process();
        }
    }

    #[test]
    fn test_pitch_drift_params() {
        let mut pitch = Pitch::new(48000.0);
        pitch.set_frequency_immediate(440.0);
        pitch.set_drift_lfo_frequency(4.0);
        pitch.set_drift_lfo_amplitude(0.003);
        for _ in 0..100 {
            pitch.process();
        }
    }

    #[test]
    fn test_pitch_set_target_frequency() {
        let mut pitch = Pitch::new(48000.0);
        pitch.set_frequency_immediate(220.0);
        // Drain the initial transient
        for _ in 0..100 {
            pitch.process();
        }
        // Now ramp to a new target
        pitch.set_target_frequency(440.0);
        // Should produce non-zero frequencies
        for _ in 0..500 {
            let freq = pitch.process();
            assert!(freq > 0.0);
        }
    }

    // ===== Grain Tests =====

    #[test]
    fn test_grain_new_inactive() {
        let grain = Grain::new();
        assert!(!grain.is_active());
    }

    #[test]
    fn test_grain_play_active() {
        let wavetable = vec![0i16; 256];
        let mut grain = Grain::new();
        grain.set_wavetable_memory(&wavetable);
        grain.set_grain_length(64);
        grain.play(0, -1, 0.0, 1.0);
        assert!(grain.is_active());
    }

    #[test]
    fn test_grain_process_samples() {
        // Simple: a single DC value at position 0
        let wavetable = vec![1000i16; 128];
        let mut grain = Grain::new();
        grain.set_wavetable_memory(&wavetable);
        grain.set_grain_length(10); // Short grain
        grain.play(0, -1, 0.0, 1.0);
        assert!(grain.is_active());

        let mut active_count = 0;
        for _ in 0..20 {
            if grain.is_active() {
                active_count += 1;
            }
        }
        // Grain should have gone through its samples
        assert!(active_count > 0);
    }

    #[test]
    fn test_grain_crossfade() {
        let wavetable = vec![1000i16; 256];
        let mut grain = Grain::new();
        grain.set_wavetable_memory(&wavetable);
        grain.set_grain_length(10);
        // Play with a valid crossfade offset
        grain.play(0, 128, 0.5, 1.0);
        assert!(grain.is_active());
        for _ in 0..20 {
            grain.process();
        }
    }

    #[test]
    fn test_grain_rate() {
        let wavetable = vec![1i16; 128];
        let mut grain = Grain::new();
        grain.set_wavetable_memory(&wavetable);
        grain.set_grain_length(5);
        grain.play(0, -1, 0.0, 0.5); // Half-speed
        assert!(grain.is_active());
        for _ in 0..20 {
            grain.process();
        }
    }

    // ===== Voice Tests =====

    #[test]
    fn test_voice_new_uninitialized() {
        let voice = Voice::new();
        assert!(!voice.has_init_started());
        assert!(!voice.has_init_finished());
        assert_eq!(voice.sample_rate(), 0);
        assert_eq!(voice.grain_length(), 0);
        assert_eq!(voice.wavetable_memory().len(), 0);
    }

    #[test]
    fn test_voice_nonexistent_file() {
        let mut voice = Voice::new();
        // Should not crash, should not initialize
        voice.init_from_file("nonexistent_file.voice");
        assert!(!voice.has_init_finished());
    }

    #[test]
    fn test_voice_reinit_prevented() {
        let mut voice = Voice::new();
        // Second call should be a no-op (or fail gracefully)
        voice.init_from_file("nonexistent.voice");
        voice.init_from_file("also_nonexistent.voice");
        // Should still not have initialized
        assert!(!voice.has_init_finished());
    }

    #[test]
    fn test_voice_phoneme_lookup_empty() {
        let voice = Voice::new();
        assert_eq!(voice.num_phonemes(), 0);
        assert_eq!(voice.phoneme_to_phoneme_index("a"), None);
        assert_eq!(voice.phoneme_index_to_phoneme(0), None);
    }

    #[test]
    fn test_voice_segments_empty() {
        let voice = Voice::new();
        assert_eq!(voice.num_segments(), 0);
        assert_eq!(voice.segment_to_segment_index("a"), None);
        assert_eq!(voice.segment_index_to_segment(0), None);
    }

    // ===== Integration: Deque + Pitch Together =====

    #[test]
    fn test_deque_and_pitch_together() {
        // Verify both modules work independently
        let mem: Box<[i32]> = Box::new([0; 16]);
        let mut dq: Deque<i32> = Deque::new(mem, 0, 0, -1);
        dq.push_back(440);
        dq.push_back(880);

        let mut pitch = Pitch::new(48000.0);
        pitch.set_frequency_immediate(440.0);

        for _ in 0..100 {
            let freq = pitch.process();
            assert!(freq > 400.0 && freq < 480.0);
        }
        assert_eq!(dq.front(), 440);
        dq.pop_front();
        assert_eq!(dq.front(), 880);
    }
}
