use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound};
use pvp_sim::sim::CombatSound::{self, *};
use std::{collections::HashMap, io::Cursor};

const SOUND_KINDS: [CombatSound; 8] = [
    Weak, Strong, Critical, Sweep, Knockback, NoDamage, Hurt, Death,
];

/// Synthesizes an original combat cue as a mono PCM WAV.
fn synthesized_wav(kind: CombatSound) -> Vec<u8> {
    // Duration, start/end frequency, tonal/noise balance, decay.
    let (seconds, high, low, noise, decay): (f32, f32, f32, f32, f32) = match kind {
        Weak => (0.075, 440., 130., 0.60, 5.),
        Strong => (0.15, 220., 54., 0.38, 5.),
        Critical => (0.20, 1450., 380., 0.30, 5.),
        Sweep => (0.23, 650., 110., 0.91, 2.8),
        Knockback => (0.18, 138., 42., 0.22, 4.),
        NoDamage => (0.065, 730., 440., 0.70, 6.),
        Hurt => (0.16, 340., 102., 0.48, 4.),
        Death => (0.28, 290., 58., 0.45, 4.),
    };
    let rate = 44100;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut data = Cursor::new(Vec::new());
    let mut wav = hound::WavWriter::new(&mut data, spec).expect("in-memory WAV header");
    let mut seed = 1u32;
    let mut filtered = 0.0;
    let mut phase = 0.0f32;
    for i in 0..(seconds * rate as f32) as usize {
        let t = i as f32 / rate as f32;
        let x = t / seconds;
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        filtered += 0.32 * ((seed >> 8) as f32 / 8388608.0 - 1.0 - filtered);
        phase += std::f32::consts::TAU * (high + (low - high) * x) / rate as f32;
        let env = (t / 0.003).min(1.) * (1. - x).powf(decay);
        let sample = env * (noise * filtered + (1. - noise) * phase.sin());
        wav.write_sample((sample * 24000.) as i16)
            .expect("in-memory PCM");
    }
    wav.finalize().expect("in-memory WAV lengths");
    data.into_inner()
}

pub struct CombatAudio {
    bank: HashMap<CombatSound, Sound>,
}
impl CombatAudio {
    pub async fn load() -> Self {
        let mut bank = HashMap::new();
        for kind in SOUND_KINDS {
            if let Ok(sound) = load_sound_from_bytes(&synthesized_wav(kind)).await {
                bank.insert(kind, sound);
            }
        }
        Self { bank }
    }

    pub fn play(&self, kind: CombatSound, distance: f32) {
        if let Some(clip) = self.bank.get(&kind) {
            let volume = 0.7 * 0.75 * (1.0 - distance / 16.0).clamp(0.0, 1.0);
            play_sound(
                clip,
                PlaySoundParams {
                    looped: false,
                    volume,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesized_effects_are_distinct_audible_pcm() {
        let mut clips = std::collections::HashSet::new();
        for kind in SOUND_KINDS {
            let data = synthesized_wav(kind);
            let mut wav = hound::WavReader::new(Cursor::new(&data)).unwrap();
            assert_eq!(wav.spec().sample_rate, 44100);
            assert!(
                wav.samples::<i16>()
                    .any(|s| s.unwrap().unsigned_abs() > 800)
            );
            assert!(clips.insert(data));
        }
    }
}
