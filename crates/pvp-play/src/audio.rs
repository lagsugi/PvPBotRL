use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound};
use pvp_sim::sim::CombatSound::{self, *};
use serde_json::Value;
use std::{collections::HashMap, env, fs, io::Cursor, path::PathBuf};

const EVENTS: [(CombatSound, &str); 8] = [
    (Weak, "entity.player.attack.weak"),
    (Strong, "entity.player.attack.strong"),
    (Critical, "entity.player.attack.crit"),
    (Sweep, "entity.player.attack.sweep"),
    (Knockback, "entity.player.attack.knockback"),
    (NoDamage, "entity.player.attack.nodamage"),
    (Hurt, "entity.player.hurt"),
    (Death, "entity.player.death"),
];

fn assets_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("PVP_MINECRAFT_ASSETS") {
        return Some(dir.into());
    }
    if cfg!(windows) {
        Some(PathBuf::from(env::var_os("APPDATA")?).join(".minecraft/assets"))
    } else {
        let home = PathBuf::from(env::var_os("HOME")?);
        Some(home.join(if cfg!(target_os = "macos") {
            "Library/Application Support/minecraft/assets"
        } else {
            ".minecraft/assets"
        }))
    }
}

struct LocalAssets {
    root: PathBuf,
    index: Value,
    events: Value,
}
impl LocalAssets {
    fn open(root: PathBuf) -> Option<Self> {
        // The Java 26.1.2 version manifest specifies asset index 30.
        let index = serde_json::from_slice(&fs::read(root.join("indexes/30.json")).ok()?).ok()?;
        let mut assets = Self {
            root,
            index,
            events: Value::Null,
        };
        assets.events = serde_json::from_slice(&assets.object("minecraft/sounds.json")?).ok()?;
        Some(assets)
    }
    fn object(&self, key: &str) -> Option<Vec<u8>> {
        let entry = &self.index["objects"][key];
        let hash = entry["hash"].as_str()?;
        if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let data = fs::read(self.root.join("objects").join(&hash[..2]).join(hash)).ok()?;
        (entry["size"].as_u64()? == data.len() as u64).then_some(data)
    }
    fn variants(&self, event: &str) -> Vec<(Vec<u8>, f32)> {
        self.events[event]["sounds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let name = item.as_str().or_else(|| item["name"].as_str())?;
                let bytes = self.object(&format!("minecraft/sounds/{name}.ogg"))?;
                // The native mixer unwraps Vorbis decoding errors. Validate local
                // cache entries so damaged files fall back without stopping a duel.
                let mut reader = audrey::Reader::new(Cursor::new(&bytes)).ok()?;
                let desc = reader.description();
                if !(1..=2).contains(&desc.channel_count())
                    || !(8000..=192000).contains(&desc.sample_rate())
                {
                    return None;
                }
                let mut samples = reader.samples::<f32>();
                samples.next()?.ok()?;
                if !samples.all(|s| s.is_ok()) {
                    return None;
                }
                let volume = item.get("volume").and_then(Value::as_f64).unwrap_or(1.0) as f32;
                Some((bytes, volume.clamp(0.0, 1.0)))
            })
            .collect()
    }
}

/// Short original effects for installations without the matching game assets.
fn fallback_wav(kind: CombatSound) -> Vec<u8> {
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

struct Variants {
    clips: Vec<(Sound, f32)>,
    next: usize,
}
pub struct CombatAudio {
    bank: HashMap<CombatSound, Variants>,
}
impl CombatAudio {
    pub async fn load() -> Self {
        let local = assets_dir().and_then(LocalAssets::open);
        let mut bank = HashMap::new();
        for (kind, event) in EVENTS {
            let mut clips = Vec::new();
            if let Some(local) = &local {
                for (data, volume) in local.variants(event) {
                    if let Ok(sound) = load_sound_from_bytes(&data).await {
                        clips.push((sound, volume));
                    }
                }
            }
            if clips.is_empty() {
                if let Ok(sound) = load_sound_from_bytes(&fallback_wav(kind)).await {
                    clips.push((sound, 0.7));
                }
            }
            bank.insert(kind, Variants { clips, next: 0 });
        }
        Self { bank }
    }
    pub fn play(&mut self, kind: CombatSound, distance: f32) {
        let variants = self.bank.get_mut(&kind).expect("all combat sounds loaded");
        if let Some((clip, gain)) = variants.clips.get(variants.next) {
            let volume = gain * 0.75 * (1.0 - distance / 16.0).clamp(0.0, 1.0);
            play_sound(
                clip,
                PlaySoundParams {
                    looped: false,
                    volume,
                },
            );
            variants.next = (variants.next + 1) % variants.clips.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standalone_effects_are_distinct_audible_pcm() {
        let mut clips = std::collections::HashSet::new();
        for (kind, _) in EVENTS {
            let data = fallback_wav(kind);
            let mut wav = hound::WavReader::new(Cursor::new(&data)).unwrap();
            assert_eq!(wav.spec().sample_rate, 44100);
            assert!(
                wav.samples::<i16>()
                    .any(|s| s.unwrap().unsigned_abs() > 800)
            );
            assert!(clips.insert(data));
        }
    }

    #[test]
    #[ignore = "requires an installed Minecraft 26.1.2 asset cache"]
    fn installed_combat_clips_decode() {
        let local = LocalAssets::open(assets_dir().unwrap()).expect("asset index 30");
        for (_, event) in EVENTS {
            let expected = local.events[event]["sounds"].as_array().unwrap().len();
            assert!(expected > 0);
            assert_eq!(local.variants(event).len(), expected, "{event}");
        }
    }
}
