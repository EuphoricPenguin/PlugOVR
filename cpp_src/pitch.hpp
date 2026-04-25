#pragma once

#include <array>
#include <random>


namespace oddvoices {

constexpr int k_sineTableSize = 256;
extern std::array<float, k_sineTableSize> k_sineTable;

enum class PitchState {
    Silent,
    Static,
    Preparation,
    Portamento,
    Overshoot
};

class Pitch {
public:
    Pitch(float sampleRate);

    float process();

    void setFrequencyImmediate(float frequency);
    void setTargetFrequency(float frequency);

    void setBasePortamentoTime(float time) { m_basePortamentoTime = time; }
    void setPreparationTimeRatio(float ratio) { m_preparationTimeRatio = ratio; }
    void setPreparationAmount(float amount) { m_preparationAmount = amount; }
    void setOvershootTimeRatio(float ratio) { m_overshootTimeRatio = ratio; }
    void setOvershootAmount(float amount) { m_overshootAmount = amount; }

    void setVibratoFrequency(float frequency) { m_vibratoFrequency = frequency; };
    void setVibratoMaxAmplitude(float amplitude) { m_vibratoMaxAmplitude = amplitude; };
    void setVibratoAttack(float time) { m_vibratoAttack = time; };

    void setDriftLFOFrequency(float frequency) { m_driftLFOFrequency = frequency; };
    void setDriftLFOAmplitude(float amplitude) { m_driftLFOAmplitude = amplitude; };
    void setJitterAmplitude(float amplitude) { m_jitterAmplitude = amplitude; };

private:
    const float m_sampleRate;
    std::mt19937 m_rng;

    // Parameters
    float m_basePortamentoTime = 0.1;
    float m_preparationTimeRatio = 0.5;
    float m_preparationAmount = 0.03;  // Roughly 0.5 semitones.
    float m_overshootTimeRatio = 0.5;
    float m_overshootAmount = 0.06;
    float m_vibratoFrequency = 5;
    float m_vibratoMaxAmplitude = 0.02;
    float m_vibratoAttack = 0.5;
    float m_driftLFOFrequency = 6;
    float m_driftLFOAmplitude = 0.005;
    float m_jitterAmplitude = 0.005;

    // State variables
    PitchState m_state = PitchState::Silent;
    bool m_ascending = true;
    float m_baseFrequency = 0;
    float m_previousFrequency = 0;
    float m_portamentoTime = 0;
    float m_preparationFrequency = 0;
    float m_preparationTime = 0;
    float m_overshootFrequency = 0;
    float m_overshootTime = 0;
    float m_targetFrequency = 0;
    float m_portamentoTimeRemaining = 0;
    float m_preparationTimeRemaining = 0;
    float m_overshootTimeRemaining = 0;
    float m_vibratoAmplitude = 0;
    float m_vibratoPhase = 0;
    float m_driftLFOValue1 = 0;
    float m_driftLFOValue2 = 0;
    float m_driftLFOPhase = 0;
    float m_jitterValue = 0;

    void startPreparation();
    void startPortamento();
    void startOvershoot();
};

} // namespace oddvoices
