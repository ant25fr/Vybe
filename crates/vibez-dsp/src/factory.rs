use vibez_core::effect::EffectType;

use crate::auto_pan::AutoPanEffect;
use crate::bitcrush::BitcrushEffect;
use crate::chorus::ChorusEffect;
use crate::compressor::CompressorEffect;
use crate::delay::DelayEffect;
use crate::drive::DriveEffect;
use crate::echo::EchoEffect;
use crate::effect::AudioEffect;
use crate::eq::EqEffect;
use crate::filter::FilterEffect;
use crate::flanger::FlangerEffect;
use crate::frequency_shifter::FrequencyShifterEffect;
use crate::gain::GainEffect;
use crate::gate::GateEffect;
use crate::limiter::LimiterEffect;
use crate::multiband_dynamics::MultibandDynamicsEffect;
use crate::phaser::PhaserEffect;
use crate::pitch_shifter::PitchShifterEffect;
use crate::reverb::ReverbEffect;
use crate::ring_modulator::RingModulatorEffect;
use crate::saturator::SaturatorEffect;
use crate::stereo_widener::StereoWidenerEffect;

/// Create an effect instance from its type.
pub fn create_effect(effect_type: EffectType, sample_rate: f32) -> Box<dyn AudioEffect> {
    match effect_type {
        EffectType::Gain => Box::new(GainEffect::new()),
        EffectType::Filter => Box::new(FilterEffect::new(sample_rate)),
        EffectType::Delay => Box::new(DelayEffect::new(sample_rate)),
        EffectType::Reverb => Box::new(ReverbEffect::new(sample_rate)),
        EffectType::Drive => Box::new(DriveEffect::new(sample_rate)),
        EffectType::Bitcrush => Box::new(BitcrushEffect::with_sample_rate(sample_rate)),
        EffectType::Compressor => Box::new(CompressorEffect::new(sample_rate)),
        EffectType::AutoPan => Box::new(AutoPanEffect::new(sample_rate)),
        EffectType::Gate => Box::new(GateEffect::new(sample_rate)),
        EffectType::Phaser => Box::new(PhaserEffect::new(sample_rate)),
        EffectType::Eq => Box::new(EqEffect::new(sample_rate)),
        EffectType::Echo => Box::new(EchoEffect::new(sample_rate)),
        EffectType::Limiter => Box::new(LimiterEffect::new(sample_rate)),
        EffectType::Saturator => Box::new(SaturatorEffect::new(sample_rate)),
        EffectType::MultibandDynamics => Box::new(MultibandDynamicsEffect::new(sample_rate)),
        EffectType::Chorus => Box::new(ChorusEffect::new(sample_rate)),
        EffectType::Flanger => Box::new(FlangerEffect::new(sample_rate)),
        EffectType::StereoWidener => Box::new(StereoWidenerEffect::new(sample_rate)),
        EffectType::PitchShifter => Box::new(PitchShifterEffect::new(sample_rate)),
        EffectType::RingModulator => Box::new(RingModulatorEffect::new(sample_rate)),
        EffectType::FrequencyShifter => Box::new(FrequencyShifterEffect::new(sample_rate)),
    }
}

/// Create an effect and restore saved parameters.
pub fn create_effect_with_params(
    effect_type: EffectType,
    sample_rate: f32,
    params: &[f32],
) -> Box<dyn AudioEffect> {
    let mut fx = create_effect(effect_type, sample_rate);
    for (i, &val) in params.iter().enumerate() {
        fx.set_param(i, val);
    }
    fx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_creates_all_types() {
        for &t in EffectType::all() {
            let fx = create_effect(t, 44100.0);
            assert_eq!(fx.effect_type(), t);
        }
    }

    #[test]
    fn factory_with_params() {
        let fx = create_effect_with_params(EffectType::Gain, 44100.0, &[0.75]);
        assert!((fx.get_param(0) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn factory_with_empty_params() {
        let fx = create_effect_with_params(EffectType::Delay, 44100.0, &[]);
        // Should use defaults
        assert!((fx.get_param(0) - 500.0).abs() < 1e-3);
    }

    #[test]
    fn factory_all_effects_process_without_panic() {
        for &t in EffectType::all() {
            let mut fx = create_effect(t, 44100.0);
            let mut buf = vec![0.5, -0.3, 0.8, -0.1];
            fx.process(&mut buf, 2);
            // Just verify no panic
        }
    }

    #[test]
    fn factory_all_effects_reset_without_panic() {
        for &t in EffectType::all() {
            let mut fx = create_effect(t, 44100.0);
            fx.reset();
        }
    }
}
