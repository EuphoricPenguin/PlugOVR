#include <iostream>
#include <limits>

#include "liboddvoices.hpp"

namespace oddvoices {

int32_t read32BitIntegerLE(std::ifstream& ifstream) {
    unsigned char c[4];
    ifstream.read(reinterpret_cast<char*>(c), 4);
    return c[0] | (c[1] << 8) | (c[2] << 16) | (c[3] << 24);
}

int16_t read16BitIntegerLE(std::ifstream& ifstream) {
    unsigned char c[2];
    ifstream.read(reinterpret_cast<char*>(c), 2);
    int16_t result = c[0] | (c[1] << 8);
    return result;
}

std::string readString(std::ifstream& ifstream) {
    char string[256];
    for (int i = 0; i < 256; i++) {
        char c;
        ifstream.read(&c, 1);
        string[i] = c;
        if (c == 0) {
            return string;
        }
    }
    std::cerr << "String too long" << std::endl;
    return "";
}

Voice::Voice()
    : m_hasInitStarted(false)
    , m_hasInitFinished(false)
{
}

void Voice::initFromFile(std::string fileName) {
    if (m_hasInitStarted || m_hasInitFinished) {
        return;
    }
    m_hasInitStarted = true;

    std::ifstream stream(fileName, std::ios::binary);
    if (!stream.is_open()) {
        m_hasInitStarted = false;
        return;
    }

    {
        char c[12];
        stream.read(c, 12);
        std::string magicWord = c;
        if (magicWord != "ODDVOICES") {
            std::cerr << "Invalid magic word" << std::endl;
            m_hasInitStarted = false;
            return;
        }
    }

    m_sampleRate = read32BitIntegerLE(stream);
    m_grainLength = read32BitIntegerLE(stream);

    while (true) {
        std::string phoneme = readString(stream);
        if (phoneme.size() == 0) {
            break;
        }
        m_phonemes.push_back(phoneme);
    }
    m_phonemes.push_back("_");

    int offset = 0;
    while (true) {
        std::string segmentName = readString(stream);
        if (segmentName.size() == 0) {
            break;
        }
        int segmentNumFrames = read32BitIntegerLE(stream);
        bool segmentIsVowel = read32BitIntegerLE(stream) != 0;
        m_segments.push_back(segmentName);
        m_segmentsNumFrames.push_back(segmentNumFrames);
        m_segmentsIsVowel.push_back(segmentIsVowel);
        m_segmentsOffset.push_back(offset);
        offset += segmentNumFrames * m_grainLength;
    }

    m_silentSegmentIndex = m_segments.size();
    m_segments.push_back("_");
    m_segmentsNumFrames.push_back(0);
    m_segmentsIsVowel.push_back(true);
    m_segmentsOffset.push_back(offset);

    m_wavetableMemory.reserve(offset);
    for (int i = 0; i < offset; i++) {
        m_wavetableMemory.push_back(read16BitIntegerLE(stream));
    }

    m_hasInitFinished = true;
}

std::string Voice::phonemeIndexToPhoneme(int index) {
    return m_phonemes[index];
}

int Voice::phonemeToPhonemeIndex(std::string phoneme) {
    for (int i = 0; i < static_cast<int>(m_phonemes.size()); i++) {
        if (m_phonemes[i] == phoneme) {
            return i;
        }
    }
    return k_noSegment;
}

std::string Voice::segmentIndexToSegment(int index) {
    return m_segments[index];
}

int Voice::segmentToSegmentIndex(std::string segment) {
    for (int i = 0; i < static_cast<int>(m_segments.size()); i++) {
        if (m_segments[i] == segment) {
            return i;
        }
    }
    return k_noSegment;
}

int Voice::segmentNumFrames(int segmentIndex) {
    if (segmentIndex < 0) {
        return 0;
    }
    return m_segmentsNumFrames[segmentIndex];
}

bool Voice::segmentIsVowel(int segmentIndex) {
    if (segmentIndex < 0) {
        return false;
    }
    return m_segmentsIsVowel[segmentIndex];
}

int Voice::segmentOffset(int segmentIndex) {
    if (segmentIndex < 0) {
        return 0;
    }
    return m_segmentsOffset[segmentIndex];
}

std::vector<int> Voice::convertPhonemesToSegmentIndices(
    const std::vector<std::string>& phonemes
)
{
    std::vector<int> segmentIndices;
    int numPhonemes = phonemes.size();
    for (int i = 0; i < numPhonemes; i++) {
        std::string phoneme1 = phonemes[i];
        std::string phoneme2 = i + 1 == numPhonemes ? "_" : phonemes[i + 1];
        // Iff phoneme1 is a vowel, it is present in the segments database as a monophone.
        // Add it to the segments list.
        if (segmentToSegmentIndex(phoneme1) != k_noSegment) {
            segmentIndices.push_back(segmentToSegmentIndex(phoneme1));
        }
        // Construct diphone with string concatenation.
        std::string diphone = phoneme1 + phoneme2;
        if (segmentToSegmentIndex(diphone) != k_noSegment) {
            // If the diphone is in the database, add it to the list.
            segmentIndices.push_back(segmentToSegmentIndex(diphone));
        } else {
            // If the diphone is NOT in the database, fall back.
            // Mostly we ignore the first phoneme, but there are special cases:
            // if the first phoneme is a diphthong, add the transition in.
            if (
                (phoneme1 == "aI" || phoneme1 == "eI" || phoneme1 == "OI")
                && segmentToSegmentIndex(phoneme1 + "j") != k_noSegment
            ) {
                segmentIndices.push_back(segmentToSegmentIndex(phoneme1 + "j"));
            }
            if (
                (phoneme1 == "aU" || phoneme1 == "oU")
                && segmentToSegmentIndex(phoneme1 + "w") != k_noSegment
            ) {
                segmentIndices.push_back(segmentToSegmentIndex(phoneme1 + "w"));
            }
            // Add a transition from silence into the second phoneme.
            if (segmentToSegmentIndex("_" + phoneme2) != k_noSegment) {
                segmentIndices.push_back(segmentToSegmentIndex("_" + phoneme2));
            }
        }
    }

    // Strip out repeated copies of "_".
    std::vector<int> segmentIndicesPass2;
    int lastSegmentIndex = k_noSegment;
    int i = 0;
    for (auto segmentIndex : segmentIndices) {
        if (
            !(segmentIndex == m_silentSegmentIndex && segmentIndex == lastSegmentIndex)
            && !(segmentIndex == m_silentSegmentIndex && i == 0)
            && !(segmentIndex == m_silentSegmentIndex && (i == static_cast<int>(segmentIndices.size()) - 1))
        ) {
            segmentIndicesPass2.push_back(segmentIndex);
        }
        lastSegmentIndex = segmentIndex;
        i += 1;
    }

    return segmentIndicesPass2;
}

void Grain::play(int offset1, int offset2, float crossfade, float rate)
{
    m_rate = rate;
    m_readPos = 0;
    m_offset1 = offset1;
    m_offset2 = offset2;
    m_crossfade = crossfade;
    m_active = true;
}

int16_t Grain::process()
{
    if (m_readPos >= m_grainLength - 1) {
        m_active = false;
    }
    if (!m_active) {
        return 0;
    }
    auto readPos = static_cast<int>(m_readPos);
    auto fracReadPos = m_readPos - readPos;

    int16_t result = 0;

    if (m_offset1 >= 0) {
        result += (
            m_wavetableMemory[m_offset1 + readPos] * (1 - fracReadPos)
            + m_wavetableMemory[m_offset1 + readPos + 1] * fracReadPos
        ) * (1 - m_crossfade);
    }

    if (m_crossfade != 0 && m_offset2 >= 0) {
        result += (
            m_wavetableMemory[m_offset2 + readPos] * (1 - fracReadPos) * m_crossfade
            + m_wavetableMemory[m_offset2 + readPos + 1] * fracReadPos * m_crossfade
        );
    }

    m_readPos += m_rate;
    return result;
}


Synth::Synth(
    float sampleRate
    , Voice* database
    , int* segmentQueueMemory
    , int segmentQueueCapacity
    , int segmentQueueStart
    , int segmentQueueSize
)
    : m_sampleRate(sampleRate)
    , m_voice(database)
    , m_silentSegmentIndex(database->silentSegmentIndex())
    , m_segment(m_silentSegmentIndex)
    , m_oldSegment(m_silentSegmentIndex)
    , m_segmentQueue(
        segmentQueueMemory
        , segmentQueueCapacity
        , segmentQueueStart
        , segmentQueueSize
        , k_noSegment
    )
    , m_pitch(m_sampleRate)
{
    if (!database->hasInitStarted() || !database->hasInitFinished()) {
        m_isErrored = true;
        return;
    }
    for (auto& grain : m_grains) {
        grain.setWavetableMemory(database->getWavetableMemory());
        grain.setGrainLength(database->getGrainLength());
    }

    m_originalF0 = m_voice->getSampleRate() / (0.5 * m_voice->getGrainLength());
}

void Synth::queueSegment(int segment)
{
    m_segmentQueue.push_back(segment);
}

void Synth::newSyllable()
{
    // Clear out any "_" at the beginning.
    while (!m_segmentQueue.empty() && m_segmentQueue.front() == m_silentSegmentIndex) {
        m_segmentQueue.pop_front();
    }

    if (m_syllableDuration <= 0) {
        newSegment();
        return;
    }
    float consonantDuration = 0;
    int i = 0;
    while (true) {
        if (i >= m_segmentQueue.size()) {
            break;
        }
        int segmentIndex = m_segmentQueue[i];
        if (m_voice->segmentIsVowel(segmentIndex)) {
            break;
        }
        consonantDuration += m_voice->segmentNumFrames(segmentIndex) / m_originalF0 - m_crossfadeLength;
        i += 1;
    }
    // Skip over the vowel.
    i += 1;
    int consonantClusterFirstIndex = i;
    int consonantClusterSizeInSegments = 0;
    bool upcomingVowelIsSilence = false;
    while (true) {
        if (i >= m_segmentQueue.size()) {
            break;
        }
        int segmentIndex = m_segmentQueue[i];
        if (segmentIndex == m_silentSegmentIndex) {
            upcomingVowelIsSilence = true;
            break;
        }
        if (m_voice->segmentIsVowel(segmentIndex)) {
            break;
        }
        consonantClusterSizeInSegments += 1;
        i += 1;
    }
    float finalConsonantDuration = 0;
    if (!upcomingVowelIsSilence) {
        consonantClusterSizeInSegments /= 2;
    }
    for (int i = 0; i < consonantClusterSizeInSegments; i++) {
        int segmentIndex = m_segmentQueue[consonantClusterFirstIndex + i];
        finalConsonantDuration += (
            m_voice->segmentNumFrames(segmentIndex) / m_originalF0 - m_crossfadeLength
        );
    }
    consonantDuration += finalConsonantDuration;
    if (consonantDuration > m_syllableDuration) {
        m_phonemeSpeed = consonantDuration / m_syllableDuration;
    } else {
        m_phonemeSpeed = 1;
    }
    m_syllableTimeRemaining = m_syllableDuration - finalConsonantDuration / m_phonemeSpeed;

    newSegment();
}

void Synth::newSegment()
{
    if (m_segmentQueue.empty()) {
        m_segment = m_silentSegmentIndex;
        m_segmentTime = 0;
        m_segmentLength = 0;
        return;
    }
    m_oldSegment = m_segment;
    m_oldSegmentTime = m_segmentTime;

    m_segment = m_segmentQueue.front();
    m_segmentQueue.pop_front();
    m_segmentTime = 0;
    m_segmentLength = m_voice->segmentNumFrames(m_segment) / m_originalF0;

    if (m_oldSegment == m_silentSegmentIndex) {
        m_crossfade = 0;
        m_crossfadeRamp = 0;
    } else {
        m_crossfade = 1;
        m_crossfadeRamp = -1 / (m_crossfadeLength * m_sampleRate);
    }
}

bool Synth::isActive()
{
    return m_segment != m_silentSegmentIndex;
}

void Synth::noteOn(float syllableDuration)
{
    m_syllableDuration = syllableDuration;
    m_noteOn = true;
}

void Synth::noteOff()
{
    m_noteOff = true;
}

int Synth::getOffset(int segment, float segmentTime)
{
    int segmentNumFrames = m_voice->segmentNumFrames(segment);
    if (segmentNumFrames == 0) {
        return -1;
    }
    int frameIndex = segmentTime * m_originalF0;
    frameIndex = frameIndex % segmentNumFrames;
    int segmentOffset = m_voice->segmentOffset(segment);
    int offset = segmentOffset + frameIndex * m_voice->getGrainLength();
    return offset;
}

int32_t Synth::process()
{
    // If an error occurred during initialization, return silence.
    if (m_isErrored) {
        return 0;
    }

    m_syllableTimeRemaining -= 1 / m_sampleRate;

    if (!isActive()) {
        if (!m_noteOn) {
            // If the synth has been inactive for more than m_pitchResetTime seconds,
            // set the pitch module's frequency to zero so that there is no portamento
            // leading into the next note.
            m_pitchResetTimeRemaining -= 1 / m_sampleRate;
            if (m_pitchResetTimeRemaining <= 0) {
                m_pitch.setFrequencyImmediate(0);
            }

            // If the synth is inactive and there is no noteOn message, return silence.
            return 0;
        } else {
            // If the synth is inactive and there has been a noteOn call...
            // a. if the segment queue is empty, return silence.
            // b. if the segment queue is not empty, start a new syllable.
            if (m_segmentQueue.empty()) {
                return 0;
            } else {
                newSyllable();
            }
        }
    } else {
        m_pitchResetTimeRemaining = m_pitchResetTime;

        // If the synth is active and there has been a noteOn call, start a new syllable.
        if (m_noteOn) {
            newSyllable();
        }

        // If the synth is active and there are pending note offs, AND we are currently
        // playing a vowel, then proceed to the next segment.
        if (m_noteOff && m_voice->segmentIsVowel(m_segment)) {
            newSegment();
        }

        // If the synth is active, and the current segment is a vowel, and we need to
        // start the final consonant cluster, start a new segment.
        // BUT, if the current syllable duration is <= 0, meaning that we have an indefinite hold,
        // don't do this.
        if (
            m_voice->segmentIsVowel(m_segment)
            && m_syllableTimeRemaining <= 0
            && !(m_syllableDuration <= 0)
        ) {
            newSegment();
        }

        // If the synth is active and we have reached the end of the current segment (or
        // rather the beginning of the crossfade of the next segment)...
        // a. if the segment is a vowel, loop back to the beginning.
        // b. if the segment is not a vowel, proceed to the next segment.
        if (m_segmentTime >= m_segmentLength - m_crossfadeLength) {
            if (m_voice->segmentIsVowel(m_segment)) {
                m_segmentTime = 0;
            } else {
                newSegment();
            }
        }
    }

    m_noteOn = false;
    m_noteOff = false;

    if (m_phase >= 1) {
        m_phase -= 1;

        auto offset = getOffset(m_segment, m_segmentTime);
        auto oldOffset = getOffset(m_oldSegment, m_oldSegmentTime);
        auto rate = (static_cast<float>(m_voice->getSampleRate()) / m_sampleRate) * m_formantShift;

        m_grains[m_nextGrain].play(offset, oldOffset, m_crossfade, rate);
        m_nextGrain = (m_nextGrain + 1) % m_maxGrains;
    }
    float segmentTimePerSample = m_phonemeSpeed / m_sampleRate;
    m_segmentTime += segmentTimePerSample;
    m_oldSegmentTime += segmentTimePerSample;
    m_crossfade = std::max(m_crossfade + m_crossfadeRamp * m_phonemeSpeed, 0.0f);
    m_phase += m_pitch.process() / m_sampleRate;

    int32_t result = 0;
    for (int i = 0; i < m_maxGrains; i++) {
        result += m_grains[i].process();
    }

    if (result > std::numeric_limits<int16_t>::max()) {
        result = std::numeric_limits<int16_t>::max();
    } else if (result < std::numeric_limits<int16_t>::min()) {
        result = std::numeric_limits<int16_t>::min();
    }
    return result;
}


} // namespace oddvoices
