#include "pitch.hpp"

#include <iostream>

namespace oddvoices {

std::array<float, k_sineTableSize> k_sineTable = {
#include "sine_table_256.inc"
};

std::uniform_real_distribution<> k_bipolar(-1.0, 1.0);

static float smoothstep(float x)
{
    return 3 * x * x - 2 * x * x * x;
}

/// Computes sin(2 pi x) using a lookup table with linear interpolation.
static float sine(float x)
{
    float sineTableIndexFloat = k_sineTableSize * x;
    int sineTableIndex = sineTableIndexFloat;
    float frac = sineTableIndexFloat - sineTableIndex;
    return (
        k_sineTable[sineTableIndex] * (1 - frac)
        + k_sineTable[(sineTableIndex + 1) % k_sineTableSize] * frac
    );
}

Pitch::Pitch(float sampleRate)
    : m_sampleRate(sampleRate)
    , m_rng(0)
{
    m_driftLFOValue1 = k_bipolar(m_rng);
    m_driftLFOValue2 = k_bipolar(m_rng);
}

float Pitch::process()
{
    if (m_state == PitchState::Silent) {
        return 0;
    }

    // We are using if statements instead of a switch statement because logic
    // involving state transitions should fall through to the next statement.

    if (m_state == PitchState::Static) {
        m_baseFrequency = m_targetFrequency;
    }

    if (m_state == PitchState::Preparation) {
        if (m_preparationTimeRemaining <= 0) {
            startPortamento();
        } else {
            float t = 1 - m_preparationTimeRemaining / m_preparationTime;
            float x = 1 - sine((1 - t) / 4);
            m_baseFrequency = (
                m_previousFrequency * (1 - x)
                + m_preparationFrequency * x
            );
            m_preparationTimeRemaining -= 1 / m_sampleRate;
        }
    }

    if (m_state == PitchState::Portamento) {
        if (m_portamentoTimeRemaining <= 0) {
            startOvershoot();
        } else {
            float t = 1 - m_portamentoTimeRemaining / m_portamentoTime;
            float x = m_ascending ? sine(t / 4) : 1 - sine((1 - t) / 4);
            m_baseFrequency = (
                m_preparationFrequency * (1 - x)
                + m_overshootFrequency * x
            );
            m_portamentoTimeRemaining -= 1 / m_sampleRate;
        }
    }

    if (m_state == PitchState::Overshoot) {
        if (m_overshootTimeRemaining <= 0) {
            m_state = PitchState::Static;
            m_baseFrequency = m_targetFrequency;
        } else {
            float t = 1 - m_overshootTimeRemaining / m_overshootTime;
            float x = sine(t / 4);
            m_baseFrequency = (
                m_overshootFrequency * (1 - x)
                + m_targetFrequency * x
            );
            m_overshootTimeRemaining -= 1 / m_sampleRate;
        }
    }

    float result = m_baseFrequency;

    int sineTableIndex = k_sineTableSize * m_vibratoPhase;
    float frac = k_sineTableSize * m_vibratoPhase - sineTableIndex;
    float vibrato = (
        k_sineTable[sineTableIndex] * (1 - frac)
        + k_sineTable[(sineTableIndex + 1) % k_sineTableSize] * frac
    ) * m_vibratoAmplitude;
    result *= 1 + vibrato;
    m_vibratoAmplitude += m_vibratoMaxAmplitude / (m_vibratoAttack * m_sampleRate);
    if (m_vibratoAmplitude >= m_vibratoMaxAmplitude) {
        m_vibratoAmplitude = m_vibratoMaxAmplitude;
    }
    m_vibratoPhase += m_vibratoFrequency / m_sampleRate;
    if (m_vibratoPhase >= 1) {
        m_vibratoPhase -= 1;
    }

    float t = smoothstep(m_driftLFOPhase);
    float drift = (
        m_driftLFOValue1 * (1 - t)
        + m_driftLFOValue2 * t
    ) * m_driftLFOAmplitude;
    m_driftLFOPhase += m_driftLFOFrequency / m_sampleRate;
    if (m_driftLFOPhase >= 1) {
        m_driftLFOPhase -= 1;
        m_driftLFOValue1 = m_driftLFOValue2;
        m_driftLFOValue2 = k_bipolar(m_rng);
    }
    result *= 1 + drift;

    m_jitterValue += k_bipolar(m_rng) / m_sampleRate;
    m_jitterValue = std::max(std::min(m_jitterValue, 1.f), -1.f);
    float jitter = m_jitterValue * m_jitterAmplitude;
    result *= 1 + jitter;

    return result;
}

void Pitch::setFrequencyImmediate(float frequency)
{
    if (frequency == 0) {
        m_state = PitchState::Silent;
    } else {
        m_state = PitchState::Static;
    }
    m_previousFrequency = frequency;
    m_preparationFrequency = frequency;
    m_overshootFrequency = frequency;
    m_targetFrequency = frequency;
}

void Pitch::setTargetFrequency(float frequency)
{
    if (m_state == PitchState::Silent) {
        setFrequencyImmediate(frequency);
        return;
    }
    if (frequency == m_targetFrequency) {
        return;
    }
    m_previousFrequency = m_baseFrequency;
    m_targetFrequency = frequency;
    m_ascending = m_targetFrequency > m_previousFrequency;

    // Portamento is scaled by the number of octaves in the interval.
    auto portamentoScale = 1 + std::abs(
        std::log2(m_targetFrequency / m_previousFrequency)
    ) / 12;
    m_portamentoTime = m_basePortamentoTime * portamentoScale;
    m_preparationTime = m_portamentoTime * m_preparationTimeRatio;
    m_overshootTime = m_portamentoTime * m_overshootTimeRatio;

    // The preparation and overshoot frequency ratios can be no more
    // than half the frequency ratio between the target and previous
    // frequencies. This prevents ridiculous prep/overshoot for
    // small intervals. This is a heuristic derived intuitively and
    // not based on any real data.
    float adjustedPreparationAmount = std::min(
        (m_targetFrequency / m_previousFrequency - 1) * 0.5f,
        m_preparationAmount
    );
    float adjustedOvershootAmount = std::min(
        (m_previousFrequency / m_targetFrequency - 1) * 0.5f,
        m_overshootAmount
    );

    // Based on real observations of a singer's pitch data, preparation
    // only happens for ascending intervals and overshoot only happens
    // for descending intervals.
    m_preparationFrequency = m_ascending
        ? m_baseFrequency * (1 - adjustedPreparationAmount)
        : m_baseFrequency;
    m_overshootFrequency = m_ascending
        ? frequency
        : frequency * (1 - adjustedOvershootAmount);

    startPreparation();
}

void Pitch::startPreparation()
{
    if (!m_ascending) {
        startPortamento();
        return;
    }
    m_state = PitchState::Preparation;
    m_preparationTimeRemaining = m_preparationTime;
}

void Pitch::startPortamento()
{
    m_state = PitchState::Portamento;
    m_portamentoTimeRemaining = m_portamentoTime;
}

void Pitch::startOvershoot()
{
    if (m_ascending) {
        m_state = PitchState::Static;
        m_baseFrequency = m_targetFrequency;
        return;
    }
    m_state = PitchState::Overshoot;
    m_overshootTimeRemaining = m_overshootTime;
}

} // namespace oddvoices
