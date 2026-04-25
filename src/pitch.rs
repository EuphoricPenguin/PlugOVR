use rand::{RngExt, SeedableRng};
use rand::distr::Uniform;
/// Pitch/F0 modeling for the OddVoices synthesizer.
///
/// Implements portamento, preparation, overshoot, vibrato, drift LFO, and jitter
/// to produce natural-sounding pitched audio.
use rand::rngs::StdRng;
use std::sync::LazyLock;

/// Parse the CSV file contents into a [f32; 256].
fn parse_sine_table(csv: &str) -> [f32; 256] {
    let mut table = [0.0f32; 256];
    for (i, val) in csv.split(',').enumerate() {
        if i < 256 {
            table[i] = val.trim().parse().unwrap();
        }
    }
    table
}

#[rustfmt::skip]
static SINE_TABLE: LazyLock<[f32; 256]> = LazyLock::new(|| parse_sine_table(include_str!("sine_table.csv")));

pub const SINE_TABLE_SIZE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PitchState {
    Silent,
    Static,
    Preparation,
    Portamento,
    Overshoot,
}

/// A smoothstep function: 3x^2 - 2x^3.
#[inline]
fn smoothstep(x: f32) -> f32 {
    let x2 = x * x;
    3.0 * x2 - 2.0 * x2 * x
}

/// Computes sin(2*pi*x) using a lookup table with linear interpolation.
#[inline]
fn sine(x: f32) -> f32 {
    let index_float = (SINE_TABLE_SIZE as f32) * x;
    let index = index_float as usize;
    let frac = index_float - index as f32;
    let idx = index % SINE_TABLE_SIZE;
    let next_idx = (idx + 1) % SINE_TABLE_SIZE;
    SINE_TABLE[idx] * (1.0 - frac) + SINE_TABLE[next_idx] * frac
}

pub struct Pitch {
    sample_rate: f32,
    rng: StdRng,
    state: PitchState,

    // Parameters
    base_portamento_time: f32,
    preparation_time_ratio: f32,
    preparation_amount: f32,
    overshoot_time_ratio: f32,
    overshoot_amount: f32,
    vibrato_frequency: f32,
    vibrato_max_amplitude: f32,
    vibrato_attack: f32,
    drift_lfo_frequency: f32,
    drift_lfo_amplitude: f32,
    jitter_amplitude: f32,

    // State variables
    ascending: bool,
    base_frequency: f32,
    previous_frequency: f32,
    portamento_time: f32,
    preparation_frequency: f32,
    preparation_time: f32,
    overshoot_frequency: f32,
    overshoot_time: f32,
    target_frequency: f32,
    portamento_time_remaining: f32,
    preparation_time_remaining: f32,
    overshoot_time_remaining: f32,
    vibrato_amplitude: f32,
    vibrato_phase: f32,
    drift_lfo_value1: f32,
    drift_lfo_value2: f32,
    drift_lfo_phase: f32,
    jitter_value: f32,
}

impl Pitch {
    /// Create a new Pitch module.
    ///
    /// `sample_rate` is the audio sample rate in Hz (e.g., 48000.0).
    pub fn new(sample_rate: f32) -> Self {
        let mut rng = StdRng::seed_from_u64(0);
        let dist = Uniform::new(-1.0f32, 1.0f32).unwrap();

        let drift_lfo_value1: f32 = rng.sample(dist.clone());
        let drift_lfo_value2: f32 = rng.sample(dist);

        Self {
            sample_rate,
            rng: rng,
            state: PitchState::Silent,
            base_portamento_time: 0.1,
            preparation_time_ratio: 0.5,
            preparation_amount: 0.03,
            overshoot_time_ratio: 0.5,
            overshoot_amount: 0.06,
            vibrato_frequency: 5.0,
            vibrato_max_amplitude: 0.02,
            vibrato_attack: 0.5,
            drift_lfo_frequency: 6.0,
            drift_lfo_amplitude: 0.005,
            jitter_amplitude: 0.005,
            ascending: true,
            base_frequency: 0.0,
            previous_frequency: 0.0,
            portamento_time: 0.0,
            preparation_frequency: 0.0,
            preparation_time: 0.0,
            overshoot_frequency: 0.0,
            overshoot_time: 0.0,
            target_frequency: 0.0,
            portamento_time_remaining: 0.0,
            preparation_time_remaining: 0.0,
            overshoot_time_remaining: 0.0,
            vibrato_amplitude: 0.0,
            vibrato_phase: 0.0,
            drift_lfo_value1,
            drift_lfo_value2,
            drift_lfo_phase: 0.0,
            jitter_value: 0.0,
        }
    }

    /// Process one sample worth of pitch modulation.
    ///
    /// Returns the frequency modulation factor (to be multiplied by the base frequency).
    /// In the silent state, returns 0.
    pub fn process(&mut self) -> f32 {
        let dt = 1.0 / self.sample_rate;

        if self.state == PitchState::Silent {
            return 0.0;
        }

        // Update base frequency based on current state.
        match self.state {
            PitchState::Static => {
                self.base_frequency = self.target_frequency;
            }
            PitchState::Preparation => {
                if self.preparation_time_remaining <= 0.0 {
                    self.start_portamento();
                } else {
                    let t = 1.0 - self.preparation_time_remaining / self.preparation_time;
                    let x = 1.0 - sine((1.0 - t) / 4.0);
                    self.base_frequency =
                        self.previous_frequency * (1.0 - x) + self.preparation_frequency * x;
                    self.preparation_time_remaining -= dt;
                }
            }
            PitchState::Portamento => {
                if self.portamento_time_remaining <= 0.0 {
                    self.start_overshoot();
                } else {
                    let t = 1.0 - self.portamento_time_remaining / self.portamento_time;
                    let x = if self.ascending {
                        sine(t / 4.0)
                    } else {
                        1.0 - sine((1.0 - t) / 4.0)
                    };
                    self.base_frequency =
                        self.preparation_frequency * (1.0 - x) + self.overshoot_frequency * x;
                    self.portamento_time_remaining -= dt;
                }
            }
            PitchState::Overshoot => {
                if self.overshoot_time_remaining <= 0.0 {
                    self.state = PitchState::Static;
                    self.base_frequency = self.target_frequency;
                } else {
                    let t = 1.0 - self.overshoot_time_remaining / self.overshoot_time;
                    let x = sine(t / 4.0);
                    self.base_frequency =
                        self.overshoot_frequency * (1.0 - x) + self.target_frequency * x;
                    self.overshoot_time_remaining -= dt;
                }
            }
            PitchState::Silent => {} // unreachable due to early return
        }

        let mut result = self.base_frequency;

        // Vibrato
        let sine_index = (SINE_TABLE_SIZE as f32) * self.vibrato_phase;
        let idx = sine_index as usize;
        let frac = sine_index - idx as f32;
        let vibrato_idx = idx % SINE_TABLE_SIZE;
        let next_vibrato_idx = (vibrato_idx + 1) % SINE_TABLE_SIZE;
        let vibrato_val = (SINE_TABLE[vibrato_idx] * (1.0 - frac)
            + SINE_TABLE[next_vibrato_idx] * frac)
            * self.vibrato_amplitude;
        result *= 1.0 + vibrato_val;
        self.vibrato_amplitude +=
            self.vibrato_max_amplitude / (self.vibrato_attack * self.sample_rate);
        if self.vibrato_amplitude >= self.vibrato_max_amplitude {
            self.vibrato_amplitude = self.vibrato_max_amplitude;
        }
        self.vibrato_phase += self.vibrato_frequency / self.sample_rate;
        if self.vibrato_phase >= 1.0 {
            self.vibrato_phase -= 1.0;
        }

        // Drift LFO (smooth random walk)
        let t = smoothstep(self.drift_lfo_phase);
        let drift = (self.drift_lfo_value1 * (1.0 - t) + self.drift_lfo_value2 * t)
            * self.drift_lfo_amplitude;
        self.drift_lfo_phase += self.drift_lfo_frequency / self.sample_rate;
        if self.drift_lfo_phase >= 1.0 {
            self.drift_lfo_phase -= 1.0;
            self.drift_lfo_value1 = self.drift_lfo_value2;
            let dist = Uniform::new(-1.0f32, 1.0f32).unwrap();
            self.drift_lfo_value2 = self.rng.sample(dist);
        }
        result *= 1.0 + drift;

        // Jitter (random walk, clamped)
        let dist = Uniform::new(-1.0f32, 1.0f32).unwrap();
        self.jitter_value += self.rng.sample(dist) / self.sample_rate;
        self.jitter_value = self.jitter_value.max(-1.0).min(1.0);
        let jitter = self.jitter_value * self.jitter_amplitude;
        result *= 1.0 + jitter;

        result
    }

    /// Immediately set frequency (no portamento). Sets to silent if 0.
    pub fn set_frequency_immediate(&mut self, frequency: f32) {
        if frequency == 0.0 {
            self.state = PitchState::Silent;
        } else {
            self.state = PitchState::Static;
        }
        self.previous_frequency = frequency;
        self.preparation_frequency = frequency;
        self.overshoot_frequency = frequency;
        self.target_frequency = frequency;
    }

    /// Ramp to a new target frequency with portamento.
    pub fn set_target_frequency(&mut self, frequency: f32) {
        if self.state == PitchState::Silent {
            self.set_frequency_immediate(frequency);
            return;
        }
        if (frequency - self.target_frequency).abs() < f32::EPSILON {
            return;
        }

        self.previous_frequency = self.base_frequency;
        self.target_frequency = frequency;
        self.ascending = self.target_frequency > self.previous_frequency;

        // Portamento scaled by octaves in the interval.
        let octaves = (self.target_frequency / self.previous_frequency)
            .log2()
            .abs();
        let portamento_scale = 1.0 + octaves / 12.0;
        self.portamento_time = self.base_portamento_time * portamento_scale;
        self.preparation_time = self.portamento_time * self.preparation_time_ratio;
        self.overshoot_time = self.portamento_time * self.overshoot_time_ratio;

        // Clamp preparation/overshoot amounts to prevent extreme values for small intervals.
        let prep_clamp = if self.previous_frequency > 0.0 {
            ((self.target_frequency / self.previous_frequency - 1.0) * 0.5)
                .min(self.preparation_amount)
        } else {
            self.preparation_amount
        };
        let oversh_clamp = if self.target_frequency > 0.0 {
            ((self.previous_frequency / self.target_frequency - 1.0) * 0.5)
                .min(self.overshoot_amount)
        } else {
            self.overshoot_amount
        };

        // Preparation only for ascending, overshoot only for descending.
        self.preparation_frequency = if self.ascending {
            self.base_frequency * (1.0 - prep_clamp)
        } else {
            self.base_frequency
        };
        self.overshoot_frequency = if self.ascending {
            frequency
        } else {
            frequency * (1.0 - oversh_clamp)
        };

        self.start_preparation();
    }

    fn start_preparation(&mut self) {
        if !self.ascending {
            self.start_portamento();
            return;
        }
        self.state = PitchState::Preparation;
        self.preparation_time_remaining = self.preparation_time;
    }

    fn start_portamento(&mut self) {
        self.state = PitchState::Portamento;
        self.portamento_time_remaining = self.portamento_time;
    }

    fn start_overshoot(&mut self) {
        if self.ascending {
            self.state = PitchState::Static;
            self.base_frequency = self.target_frequency;
            return;
        }
        self.state = PitchState::Overshoot;
        self.overshoot_time_remaining = self.overshoot_time;
    }

    // Setters for modulation parameters.
    pub fn set_base_portamento_time(&mut self, time: f32) {
        self.base_portamento_time = time;
    }
    pub fn set_preparation_time_ratio(&mut self, ratio: f32) {
        self.preparation_time_ratio = ratio;
    }
    pub fn set_preparation_amount(&mut self, amount: f32) {
        self.preparation_amount = amount;
    }
    pub fn set_overshoot_time_ratio(&mut self, ratio: f32) {
        self.overshoot_time_ratio = ratio;
    }
    pub fn set_overshoot_amount(&mut self, amount: f32) {
        self.overshoot_amount = amount;
    }
    pub fn set_vibrato_frequency(&mut self, frequency: f32) {
        self.vibrato_frequency = frequency;
    }
    pub fn set_vibrato_max_amplitude(&mut self, amplitude: f32) {
        self.vibrato_max_amplitude = amplitude;
    }
    pub fn set_vibrato_attack(&mut self, time: f32) {
        self.vibrato_attack = time;
    }
    pub fn set_drift_lfo_frequency(&mut self, frequency: f32) {
        self.drift_lfo_frequency = frequency;
    }
    pub fn set_drift_lfo_amplitude(&mut self, amplitude: f32) {
        self.drift_lfo_amplitude = amplitude;
    }
    pub fn set_jitter_amplitude(&mut self, amplitude: f32) {
        self.jitter_amplitude = amplitude;
    }
}