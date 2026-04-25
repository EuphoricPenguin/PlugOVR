#pragma once
#include <atomic>
#include <array>
#include <iostream>
#include <fstream>
#include <random>
#include <vector>

#include "deque.hpp"
#include "pitch.hpp"

namespace oddvoices {

constexpr int k_noSegment = -1;

/// An OddVoices model, a database consisting of wavetable memory and metadata.
/// All methods are believed to be real-time safe unless otherwise noted.
/// Once initialized with initFromFile, the Voice is read-only and its methods can
/// be safely called from multiple threads.
class Voice {
public:
    Voice();

    /// Return true if initialization of this voice has started, and it cannot be initialized
    /// again. Return false otherwise.
    bool hasInitStarted() { return m_hasInitStarted; };

    /// Return true if initialization of this voice has completed, and it is safe to play a
    /// Synth from it. Return false otherwise.
    bool hasInitFinished() { return m_hasInitFinished; };

    /// Initialize the voice from a path to a .voice file.
    /// Do nothing if the voice has already been initialized or is currently initializing.
    /// This method is not real-time safe.
    /// This method is not safe from untrusted .voice files, and will probably crash if a
    /// malformed file is provided.
    /// It is safe to call this method multiple times, and even simultaneously from different
    /// threads.
    void initFromFile(std::string fileName);

    /// Return the sample rate of the voice's wavetable memory.
    ///
    int getSampleRate() { return m_sampleRate; }

    /// Return the length in samples of each grain in wavetable memory.
    ///
    int getGrainLength() { return m_grainLength; }

    /// Return a pointer to the wavetable memory.
    ///
    const int16_t* getWavetableMemory() { return m_wavetableMemory.data(); }

    /// Return the number of phonemes.
    ///
    int getNumPhonemes() { return m_phonemes.size(); }

    /// Given a phoneme name, return its index. Return -1 if not present. Not real-time safe.
    ///
    int phonemeToPhonemeIndex(std::string phoneme);

    /// Given a phoneme index, return its name as a string.
    ///
    std::string phonemeIndexToPhoneme(int index);

    /// Return the number of segments in the voice.
    ///
    int getNumSegments() { return m_segments.size(); }

    /// Given a segment name, return its index.
    /// Not real-time safe.
    int segmentToSegmentIndex(std::string segment);

    /// Given a segment index, return its name.
    ///
    std::string segmentIndexToSegment(int);

    /// Given a segment index, return the number of frames (aka grains).
    ///
    int segmentNumFrames(int segmentIndex);

    /// Return true if the segment is a vowel monophone.
    ///
    bool segmentIsVowel(int segmentIndex);

    /// Return the offset of the segment into the wavetable memory.
    ///
    int segmentOffset(int segmentIndex);

    /// Convert a list of phonemes to a list of segment indices.
    ///
    std::vector<int> convertPhonemesToSegmentIndices(
        const std::vector<std::string>&
    );

    /// Get the index of the silent segment, _.
    int silentSegmentIndex() { return m_silentSegmentIndex; };

private:
    std::atomic_bool m_hasInitStarted;
    std::atomic_bool m_hasInitFinished;

    int m_sampleRate;
    int m_grainLength;
    int m_silentSegmentIndex;
    std::vector<std::string> m_phonemes;
    std::vector<std::string> m_segments;
    std::vector<int> m_segmentsNumFrames;
    std::vector<bool> m_segmentsIsVowel;
    std::vector<int> m_segmentsOffset;
    std::vector<int16_t> m_wavetableMemory;
};


class Grain {
public:
    bool isActive() { return m_active; };
    void play(int offset1, int offset2, float crossfade, float rate);

    void setWavetableMemory(const int16_t* wavetableMemory) { m_wavetableMemory = wavetableMemory; };
    void setGrainLength(int grainLength) { m_grainLength = grainLength; };

    int16_t process();

private:
    const int16_t* m_wavetableMemory = nullptr;
    int m_grainLength = 0;

    bool m_active = false;
    int m_offset1;
    int m_offset2;
    float m_readPos = 0;
    float m_crossfade;
    float m_rate;
};


/// A synthesizer that uses a Voice to synthesize sound.
///
/// The associated Voice, which is passed in at initialization as a pointer,
/// must live at least as long as the Synth does.
///
/// If the Voice is not initialized, the Synth permanently outputs silence.
class Synth {
public:
    /// Constructor.
    ///
    /// @param sampleRate Sample rate of the synth. This is independent of the
    /// sample rate of the Voice.
    /// @param voice Pointer to a Voice. The Voice must live at least as long
    /// as the Synth does.
    /// @param segmentQueueMemory Pointer to memory for the segment queue.
    /// @param segmentQueueCapacity Size of segmentQueueMemory in ints.
    /// @param segmentQueueStart Initial starting position of the segment queue in ints.
    /// This is almost always zero, but provided for completeness.
    /// @param segmentQueueSize Initial size of the segment queue in ints. This is
    /// used to initialize the Synth with a nonempty segment queue.
    Synth(
        float sampleRate
        , Voice*
        , int* segmentQueueMemory
        , int segmentQueueCapacity
        , int segmentQueueStart = 0
        , int segmentQueueSize = 0
    );

    /// Return true if the Synth encountered an error in initialization and permanently
    /// produces silent output.
    bool isErrored();

    /// Compute one sample of output.
    ///
    int32_t process();

    /// Trigger a note on event.
    ///
    /// @param syllableDuration The length of the note, or rather the melisma. If this
    /// is <= 0, the note is held indefinitely until a noteOff event is received. Otherwise
    /// the note has finite length and all noteOn and noteOff events will be ignored
    /// for the next syllableDuration seconds.
    void noteOn(float syllableDuration);

    /// Trigger a note on event of indefinite length.
    void noteOn() { noteOn(0); };

    /// Trigger a note off event.
    ///
    void noteOff();

    /// Return true if the Synth is currently playing a segment, and false otherwise.
    /// A Synth will automatically go inactive when the queue runs out and the final
    /// segment finishes playing.
    bool isActive();

    /// Immediately set fundamental frequency in Hertz.
    ///
    void setFrequencyImmediate(float frequency) { m_pitch.setFrequencyImmediate(frequency); };

    /// Ramp to a new fundamental frequency in Hertz.
    ///
    void setTargetFrequency(float frequency) { m_pitch.setTargetFrequency(frequency); };

    /// Add a segment to the queue. The segment is given by index in the Voice.
    ///
    void queueSegment(int segment);

    /// Set formant shift, given as a ratio. 1 is default.
    ///
    void setFormantShift(float formantShift) { m_formantShift = formantShift; };

    /// Set speed of phonemes, given as a ratio. 1 is default.
    ///
    void setPhonemeSpeed(float phonemeSpeed) { m_phonemeSpeed = phonemeSpeed; };

private:
    bool m_isErrored = false;

    const float m_sampleRate;
    Voice* m_voice;
    int m_silentSegmentIndex;
    int m_segment;
    int m_oldSegment;
    Deque<int> m_segmentQueue;

    Pitch m_pitch;

    float m_phase = 1;
    float m_phonemeSpeed = 1;
    float m_formantShift = 1;

    static constexpr int m_maxGrains = 10;
    int m_nextGrain = 0;
    std::array<Grain, m_maxGrains> m_grains;

    float m_originalF0;

    bool m_noteOn = false;
    bool m_noteOff = false;

    float m_syllableDuration = 0;
    float m_syllableTimeRemaining = 0;
    int m_syllableSegmentsRemaining = 0;

    float m_segmentTime;
    float m_segmentLength;
    float m_oldSegmentTime;
    float m_oldSegmentLength;

    float m_crossfade;
    float m_crossfadeRamp;
    float m_crossfadeLength = 0.03;

    static constexpr float m_pitchResetTime = 0.01;
    float m_pitchResetTimeRemaining = 0;

    void newSegment();
    void newSyllable();
    int getOffset(int segment, float segmentTime);
};


} // namespace oddvoices
