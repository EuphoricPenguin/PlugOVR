/// A wavetable grain player for the OddVoices synthesizer.
///
/// Each grain plays back two adjacent wavetable frames (offset1 and offset2)
/// with linear interpolation and crossfading between them.

#[derive(Clone)]
pub struct Grain {
    wavetable_memory: Vec<i16>,
    grain_length: usize,
    active: bool,
    offset1: i32,
    offset2: i32,
    read_pos: f32,
    crossfade: f32,
    rate: f32,
}

impl Grain {
    pub fn new() -> Self {
        Self {
            wavetable_memory: Vec::new(),
            grain_length: 0,
            active: false,
            offset1: 0,
            offset2: 0,
            read_pos: 0.0,
            crossfade: 0.0,
            rate: 1.0,
        }
    }

    /// Check if this grain is currently active.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Set the wavetable memory.
    #[inline]
    pub fn set_wavetable_memory(&mut self, memory: &[i16]) {
        self.wavetable_memory = memory.to_vec();
    }

    /// Set the grain length in samples.
    #[inline]
    pub fn set_grain_length(&mut self, grain_length: usize) {
        self.grain_length = grain_length;
    }

    /// Start playing a grain with two offsets and a crossfade.
    ///
    /// `offset1` and `offset2` are byte offsets into the wavetable memory.
    /// `crossfade` is the crossfade amount blending between the two offsets.
    /// `rate` is the read rate (1.0 = normal speed).
    #[inline]
    pub fn play(&mut self, offset1: i32, offset2: i32, crossfade: f32, rate: f32) {
        self.rate = rate;
        self.read_pos = 0.0;
        self.offset1 = offset1;
        self.offset2 = offset2;
        self.crossfade = crossfade;
        self.active = true;
    }

    /// Process one sample from this grain.
    ///
    /// Returns the audio sample as an i16 value.
    /// The grain reads from both offset1 and offset2 with linear interpolation
    /// and crossfading between them.
    #[inline]
    pub fn process(&mut self) -> i16 {
        if !self.active {
            return 0;
        }

        if self.read_pos >= self.grain_length as f32 - 1.0 {
            self.active = false;
            return 0;
        }

        let read_pos = self.read_pos as i32;
        let frac = self.read_pos - read_pos as f32;

        let mut result: f32 = 0.0;

        // Read from offset1 (interpolated, scaled by crossfade envelope)
        if self.offset1 >= 0 {
            let base1 = self.offset1 as usize;
            let idx1 = base1 + read_pos as usize;
            let idx2 = idx1 + 1;
            if idx2 < self.wavetable_memory.len() {
                let sample1 = self.wavetable_memory[idx1] as f32;
                let sample2 = self.wavetable_memory[idx2] as f32;
                let interp = sample1 * (1.0 - frac) + sample2 * frac;
                result += interp * (1.0 - self.crossfade);
            }
        }

        // Read from offset2 (interpolated, scaled by crossfade)
        if self.crossfade != 0.0 && self.offset2 >= 0 {
            let base2 = self.offset2 as usize;
            let idx1 = base2 + read_pos as usize;
            let idx2 = idx1 + 1;
            if idx2 < self.wavetable_memory.len() {
                let sample1 = self.wavetable_memory[idx1] as f32;
                let sample2 = self.wavetable_memory[idx2] as f32;
                let interp = sample1 * (1.0 - frac) + sample2 * frac;
                result += interp * self.crossfade;
            }
        }

        self.read_pos += self.rate;

        // Clamp to i16 range and return
        result.round() as i16
    }
}