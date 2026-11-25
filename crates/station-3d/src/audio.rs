use bevy::prelude::*;
use std::collections::HashMap;
use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 22_050;

pub const ALL_SFX_NAMES: [&str; 27] = [
    "perm", "turn", "key", "print", "ding", "deploy", "alarm", "siren", "meow", "pop", "chat",
    "bloop", "brew", "rustle", "click", "pock", "shutter", "coin", "beep", "micro", "buzz",
    "thunder", "bell", "water", "clap", "thud", "mixue",
];

#[derive(Debug, Clone, Copy)]
enum Wave {
    Sine,
    Triangle,
    Square,
    Saw,
}

fn add_tone(
    buf: &mut Vec<f32>,
    f0: f32,
    t0: f32,
    dur: f32,
    wave: Wave,
    vol: f32,
    f1: Option<f32>,
) {
    let start = (t0 * SAMPLE_RATE as f32).round() as usize;
    let len = (dur * SAMPLE_RATE as f32).round().max(1.0) as usize;
    if buf.len() < start + len {
        buf.resize(start + len, 0.0);
    }
    let f_end = f1.unwrap_or(f0);
    let mut phase = 0.0_f32;
    for i in 0..len {
        let u = i as f32 / len as f32;
        let freq = f0 * (f_end / f0).powf(u);
        phase += TAU * freq / SAMPLE_RATE as f32;
        let s = match wave {
            Wave::Sine => phase.sin(),
            Wave::Triangle => (2.0 / std::f32::consts::PI) * phase.sin().asin(),
            Wave::Square => if phase.sin() >= 0.0 { 1.0 } else { -1.0 },
            Wave::Saw => 2.0 * ((phase / TAU).fract()) - 1.0,
        };
        let attack = (u / 0.04).min(1.0);
        let env = attack * (1.0 - u).powf(1.2);
        buf[start + i] += s * vol * env;
    }
}

fn add_noise(buf: &mut Vec<f32>, t0: f32, dur: f32, vol: f32) {
    let start = (t0 * SAMPLE_RATE as f32).round() as usize;
    let len = (dur * SAMPLE_RATE as f32).round().max(1.0) as usize;
    if buf.len() < start + len {
        buf.resize(start + len, 0.0);
    }
    let mut seed: u32 = 0x1234_5678 ^ (start as u32);
    for i in 0..len {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let n = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let env = 1.0 - (i as f32 / len as f32);
        buf[start + i] += n * vol * env;
    }
}

/// Synthesizes any of the 27 Oracle `src/office/audio/sfx.js` sounds into a 16-bit PCM WAV byte buffer.
pub fn synthesize_sfx_wav(name: &str) -> Option<Vec<u8>> {
    let mut buf = Vec::<f32>::new();
    match name {
        "perm" => {
            add_tone(&mut buf, 880.0, 0.0, 0.16, Wave::Triangle, 0.25, None);
            add_tone(&mut buf, 1175.0, 0.13, 0.22, Wave::Triangle, 0.22, None);
        }
        "turn" => {
            add_tone(&mut buf, 659.0, 0.0, 0.18, Wave::Sine, 0.2, None);
            add_tone(&mut buf, 988.0, 0.12, 0.30, Wave::Sine, 0.18, None);
        }
        "key" => add_noise(&mut buf, 0.0, 0.03, 0.15),
        "print" => {
            for i in 0..6 {
                add_noise(&mut buf, i as f32 * 0.09, 0.06, 0.12);
            }
        }
        "ding" => {
            add_tone(&mut buf, 1318.5, 0.0, 0.55, Wave::Sine, 0.18, None);
            add_tone(&mut buf, 1046.5, 0.26, 0.80, Wave::Sine, 0.18, None);
        }
        "deploy" => {
            add_tone(&mut buf, 220.0, 0.0, 1.0, Wave::Saw, 0.12, Some(880.0));
            add_noise(&mut buf, 0.0, 1.0, 0.08);
        }
        "alarm" => {
            for i in 0..15 {
                add_tone(&mut buf, 2380.0, i as f32 * 0.05, 0.045, Wave::Square, 0.1, None);
            }
        }
        "siren" => {
            for i in 0..2 {
                let t = i as f32 * 0.6;
                add_tone(&mut buf, 620.0, t, 0.3, Wave::Square, 0.1, Some(880.0));
                add_tone(&mut buf, 880.0, t + 0.3, 0.3, Wave::Square, 0.1, Some(620.0));
            }
        }
        "meow" => add_tone(&mut buf, 740.0, 0.0, 0.35, Wave::Sine, 0.22, Some(520.0)),
        "pop" => add_tone(&mut buf, 520.0, 0.0, 0.08, Wave::Sine, 0.2, Some(900.0)),
        "chat" => {
            add_tone(&mut buf, 1568.0, 0.0, 0.11, Wave::Sine, 0.16, None);
            add_tone(&mut buf, 2093.0, 0.08, 0.20, Wave::Sine, 0.14, None);
        }
        "bloop" => {
            add_tone(&mut buf, 320.0, 0.0, 0.12, Wave::Sine, 0.2, Some(760.0));
            add_tone(&mut buf, 420.0, 0.14, 0.12, Wave::Sine, 0.16, Some(900.0));
        }
        "brew" => {
            add_noise(&mut buf, 0.0, 0.8, 0.1);
            add_tone(&mut buf, 110.0, 0.0, 0.8, Wave::Sine, 0.12, Some(90.0));
        }
        "rustle" => {
            add_noise(&mut buf, 0.0, 0.12, 0.12);
            add_noise(&mut buf, 0.1, 0.16, 0.1);
        }
        "click" => add_tone(&mut buf, 1800.0, 0.0, 0.03, Wave::Square, 0.14, None),
        "pock" => {
            add_tone(&mut buf, 1250.0, 0.0, 0.045, Wave::Triangle, 0.2, Some(760.0));
            add_noise(&mut buf, 0.0, 0.02, 0.08);
        }
        "shutter" => {
            add_noise(&mut buf, 0.0, 0.04, 0.16);
            add_noise(&mut buf, 0.09, 0.05, 0.14);
        }
        "coin" => {
            add_tone(&mut buf, 988.0, 0.0, 0.09, Wave::Square, 0.14, None);
            add_tone(&mut buf, 1319.0, 0.08, 0.28, Wave::Square, 0.14, None);
        }
        "beep" => {
            add_tone(&mut buf, 1046.0, 0.0, 0.09, Wave::Square, 0.12, None);
            add_tone(&mut buf, 1318.0, 0.12, 0.09, Wave::Square, 0.12, None);
        }
        "micro" => {
            for i in 0..3 {
                add_tone(&mut buf, 2093.0, i as f32 * 0.32, 0.14, Wave::Square, 0.12, None);
            }
        }
        "buzz" => add_tone(&mut buf, 120.0, 0.0, 0.45, Wave::Saw, 0.12, Some(110.0)),
        "thunder" => {
            add_noise(&mut buf, 0.0, 1.4, 0.22);
            add_tone(&mut buf, 48.0, 0.0, 1.2, Wave::Sine, 0.28, Some(30.0));
        }
        "bell" => {
            add_tone(&mut buf, 2093.0, 0.0, 0.9, Wave::Sine, 0.2, None);
            add_tone(&mut buf, 2637.0, 0.0, 0.6, Wave::Sine, 0.1, None);
        }
        "water" => {
            for i in 0..4 {
                add_noise(&mut buf, i as f32 * 0.2, 0.24, 0.1);
            }
        }
        "clap" => {
            for i in 0..12 {
                add_noise(&mut buf, i as f32 * 0.11, 0.04, 0.14);
            }
        }
        "thud" => {
            for i in 0..3 {
                add_tone(&mut buf, 95.0, i as f32 * 0.22, 0.12, Wave::Sine, 0.28, Some(60.0));
            }
        }
        "mixue" => {
            // "Oh! Susanna" public-domain Mixue melody
            let notes: [(f32, f32); 13] = [
                (659.25, 1.0), (783.99, 1.0), (783.99, 2.0), (880.0, 1.0), (783.99, 1.0),
                (659.25, 2.0), (523.25, 1.0), (587.33, 1.0), (659.25, 1.0), (659.25, 1.0),
                (587.33, 1.0), (523.25, 1.0), (587.33, 2.0),
            ];
            let mut at = 0.0_f32;
            for (hz, beats) in notes {
                let d = beats * 0.19;
                add_tone(&mut buf, hz, at, d * 0.95, Wave::Triangle, 0.16, None);
                at += d;
            }
        }
        _ => return None,
    }
    Some(encode_wav_pcm16(&buf, SAMPLE_RATE))
}

fn encode_wav_pcm16(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

pub fn gap_secs(name: &str) -> f64 {
    match name {
        "mixue" => 8.0,
        "thunder" => 5.0,
        "siren" => 4.0,
        "deploy" => 3.0,
        "thud" => 2.0,
        "turn" | "alarm" | "brew" | "water" | "clap" => 1.5,
        "micro" => 1.2,
        "ding" | "perm" | "print" => 1.0,
        "meow" => 0.8,
        "bell" | "buzz" => 0.6,
        "bloop" | "beep" => 0.5,
        "shutter" | "coin" => 0.4,
        "pock" | "rustle" => 0.25,
        "key" => 0.18,
        "pop" => 0.15,
        "click" => 0.08,
        _ => 0.2,
    }
}

#[derive(Resource, Debug, Default)]
pub struct SfxEngine {
    pub enabled: bool,
    last_played: HashMap<String, f64>,
}

impl SfxEngine {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last_played: HashMap::new(),
        }
    }

    pub fn should_play(&mut self, name: &str, now_secs: f64) -> bool {
        if !self.enabled {
            return false;
        }
        let gap = gap_secs(name);
        if let Some(&prev) = self.last_played.get(name)
            && now_secs - prev < gap
        {
            return false;
        }
        self.last_played.insert(name.to_owned(), now_secs);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesizes_all_27_oracle_sfx_and_enforces_cooldown_and_mute() {
        for name in ALL_SFX_NAMES {
            let wav = synthesize_sfx_wav(name).expect("known sfx");
            assert!(wav.len() > 44, "{name} wav too small");
            assert_eq!(&wav[0..4], b"RIFF");
            assert_eq!(&wav[8..12], b"WAVE");
        }
        assert!(synthesize_sfx_wav("nonexistent").is_none());

        let mut eng = SfxEngine::new(true);
        assert!(eng.should_play("perm", 1.0));
        assert!(!eng.should_play("perm", 1.5));
        assert!(eng.should_play("perm", 2.2));
        eng.enabled = false;
        assert!(!eng.should_play("perm", 10.0));
    }
}
