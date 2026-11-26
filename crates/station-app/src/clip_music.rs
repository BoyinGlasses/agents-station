//! Procedural 15s lo-fi stereo synth matching `src/office/ui/clip-music.js`.
//! Pure Rust, zero external audio crates or sample files: FM electric piano on
//! Fmaj9 – Em7 – Dm9 – G13, round bass, swung boom-bap kit, pentatonic lead,
//! vinyl crackle, Schroeder reverb, and RIFF/WAVE 16-bit PCM encoder.

use std::f32::consts::PI;

const BPM: f32 = 96.0;
const BEAT: f32 = 60.0 / BPM;
const BAR: f32 = BEAT * 4.0;

const CHORDS: [(i32, [i32; 4]); 4] = [
    (41, [57, 60, 64, 67]), // Fmaj9
    (40, [55, 59, 62, 64]), // Em7
    (38, [53, 57, 60, 64]), // Dm9
    (43, [53, 59, 64, 69]), // G13
];

const PHRASE_A: [&[(f32, i32, f32)]; 4] = [
    &[(0.5, 76, 0.5), (1.0, 79, 1.0), (2.5, 81, 0.5), (3.0, 79, 1.0)],
    &[(0.5, 76, 0.5), (1.0, 74, 1.5), (3.0, 71, 1.0)],
    &[(0.5, 72, 0.5), (1.0, 74, 0.5), (1.5, 76, 1.5), (3.5, 74, 0.5)],
    &[(0.0, 71, 1.0), (1.5, 74, 0.5), (2.0, 74, 1.5)],
];

const PHRASE_B: [&[(f32, i32, f32)]; 4] = [
    &[(1.0, 81, 0.5), (1.5, 79, 0.5), (2.0, 76, 1.5)],
    &[(0.0, 74, 0.5), (0.5, 76, 0.5), (1.0, 79, 2.0)],
    &[(0.5, 81, 1.0), (2.0, 79, 0.5), (2.5, 76, 1.0)],
    &[(0.0, 74, 2.0), (3.0, 79, 0.5), (3.5, 76, 0.5)],
];

#[inline]
fn mtof(midi: i32) -> f32 {
    440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

#[inline]
fn swing_at(bar: usize, beat: f32) -> f32 {
    let q = (beat * 4.0 + 1e-6).floor();
    let frac = beat * 4.0 - q;
    let swing = if (q as i32) % 2 != 0 { 0.16 } else { 0.0 };
    bar as f32 * BAR + (q + swing + frac) * BEAT * 0.25
}

struct Lcg(u64);

impl Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = (self.0.wrapping_mul(16807)) % 2_147_483_647;
        self.0 as f32 / 2_147_483_647.0
    }

    fn noise(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

/// Synthesizes stereo `(left, right)` samples normalized to peak ~0.89.
pub fn synthesize_lofi(dur_secs: f32, sample_rate: u32) -> (Vec<f32>, Vec<f32>) {
    let sr = sample_rate.max(8_000) as f32;
    let n = (dur_secs.max(0.1) * sr).ceil() as usize;
    let bars = (dur_secs.max(0.1) / BAR).ceil() as usize;

    let mut left = vec![0.0_f32; n];
    let mut right = vec![0.0_f32; n];
    let mut wet_l = vec![0.0_f32; n];
    let mut wet_r = vec![0.0_f32; n];
    let mut rng = Lcg(7);

    let add = |left: &mut [f32],
               right: &mut [f32],
               wet_l: &mut [f32],
               wet_r: &mut [f32],
               t0: f32,
               len: f32,
               pan: f32,
               wet: f32,
               mut sample: Box<dyn FnMut(f32) -> f32>| {
        let i0 = (t0 * sr).round() as isize;
        let count = (len * sr).round() as isize;
        let angle = (pan + 1.0) * PI * 0.25;
        let gl = angle.cos();
        let gr = angle.sin();
        for k in 0..count {
            let idx = i0 + k;
            if idx < 0 {
                continue;
            }
            let u = idx as usize;
            if u >= left.len() {
                break;
            }
            let s = sample(k as f32 / sr);
            left[u] += s * gl;
            right[u] += s * gr;
            if wet > 0.0 {
                wet_l[u] += s * gl * wet;
                wet_r[u] += s * gr * wet;
            }
        }
    };

    for b in 0..bars {
        let (root, keys) = CHORDS[b % 4];
        for (j, &m) in keys.iter().enumerate() {
            let vel = 0.95 - j as f32 * 0.05 + rng.next_f32() * 0.08;
            let pan = -0.25 + j as f32 * 0.17;
            let f = mtof(m);
            let dur1 = BEAT * 2.2;
            add(
                &mut left,
                &mut right,
                &mut wet_l,
                &mut wet_r,
                swing_at(b, 0.0) + j as f32 * 0.012,
                dur1 + 1.2,
                pan,
                0.28,
                Box::new(move |t| {
                    let release = if t > dur1 { (-(t - dur1) / 0.18).exp() } else { 1.0 };
                    let env = (t / 0.006).min(1.0) * (-t / 1.7).exp() * release;
                    let idx_fm = 1.6 * (-t * 7.0).exp() + 0.18;
                    let w = 2.0 * PI * f * t;
                    let a = (w + idx_fm * w.sin()).sin();
                    let c = (w * 1.0035 + idx_fm * 0.8 * w.sin()).sin();
                    (a + c * 0.6) * env * vel * 0.055
                }),
            );
        }

        if b >= 1 {
            for (j, &m) in keys.iter().enumerate() {
                let vel = 0.55 + rng.next_f32() * 0.1;
                let pan = -0.2 + j as f32 * 0.13;
                let f = mtof(m);
                let dur1 = BEAT * 1.1;
                add(
                    &mut left,
                    &mut right,
                    &mut wet_l,
                    &mut wet_r,
                    swing_at(b, 2.5) + j as f32 * 0.01,
                    dur1 + 1.2,
                    pan,
                    0.28,
                    Box::new(move |t| {
                        let release = if t > dur1 { (-(t - dur1) / 0.18).exp() } else { 1.0 };
                        let env = (t / 0.006).min(1.0) * (-t / 1.7).exp() * release;
                        let idx_fm = 1.6 * (-t * 7.0).exp() + 0.18;
                        let w = 2.0 * PI * f * t;
                        let a = (w + idx_fm * w.sin()).sin();
                        let c = (w * 1.0035 + idx_fm * 0.8 * w.sin()).sin();
                        (a + c * 0.6) * env * vel * 0.055
                    }),
                );
            }

            // Round bass
            for (beat, midi, dur_beats, vel) in [
                (0.0, root, 1.4, 1.0),
                (2.5, root, 0.4, 0.8),
                (3.25, root + 7, 0.55, 0.75),
            ] {
                let f = mtof(midi);
                let dur1 = BEAT * dur_beats;
                add(
                    &mut left,
                    &mut right,
                    &mut wet_l,
                    &mut wet_r,
                    swing_at(b, beat),
                    dur1 + 0.3,
                    0.0,
                    0.0,
                    Box::new(move |t| {
                        let release = if t > dur1 { (-(t - dur1) / 0.06).exp() } else { 1.0 };
                        let env = (t / 0.01).min(1.0) * (0.65 + 0.35 * (-t / 0.25).exp()) * release;
                        let s = (2.0 * PI * f * t).sin() + 0.25 * (4.0 * PI * f * t).sin();
                        (1.6 * s).tanh() * env * vel * 0.2
                    }),
                );
            }

            // Swung boom-bap kick
            let kicks: &[(f32, f32)] = if b % 2 == 1 {
                &[(0.0, 1.0), (2.5, 0.85), (3.75, 0.5)]
            } else {
                &[(0.0, 1.0), (2.5, 0.85)]
            };
            for &(beat, vel) in kicks {
                let mut ph = 0.0_f32;
                let mut click_rng = Lcg(rng.0 ^ 0x5A17);
                add(
                    &mut left,
                    &mut right,
                    &mut wet_l,
                    &mut wet_r,
                    swing_at(b, beat),
                    0.6,
                    0.0,
                    0.0,
                    Box::new(move |t| {
                        let f = 48.0 + 75.0 * (-t / 0.03).exp();
                        ph += 2.0 * PI * f / sr;
                        let click = if t < 0.004 { click_rng.noise() * 0.3 } else { 0.0 };
                        (ph.sin() * (-t / 0.28).exp() + click) * vel * 0.5
                    }),
                );
            }

            // Snare on 2 and 4
            for beat in [1.0, 3.0] {
                let mut lp = 0.0_f32;
                let mut hp = 0.0_f32;
                let mut prev = 0.0_f32;
                let mut sn_rng = Lcg(rng.0 ^ 0x9E37);
                add(
                    &mut left,
                    &mut right,
                    &mut wet_l,
                    &mut wet_r,
                    swing_at(b, beat),
                    0.4,
                    0.05,
                    0.25,
                    Box::new(move |t| {
                        let n0 = sn_rng.noise();
                        hp = 0.75 * (hp + n0 - prev);
                        prev = n0;
                        lp += (hp - lp) * 0.45;
                        (lp * (-t / 0.13).exp() * 0.9
                            + (2.0 * PI * 185.0 * t).sin() * (-t / 0.05).exp() * 0.5)
                            * 0.22
                    }),
                );
            }

            // Hats on 8ths
            for e in 0..8 {
                let open = e == 7 && b % 4 == 3;
                let vel = (if e % 2 == 1 { 0.45 } else { 0.8 }) * (0.85 + rng.next_f32() * 0.3);
                let mut hp = 0.0_f32;
                let mut prev = 0.0_f32;
                let mut h_rng = Lcg(rng.0.wrapping_add(e as u64 + 13));
                add(
                    &mut left,
                    &mut right,
                    &mut wet_l,
                    &mut wet_r,
                    swing_at(b, e as f32 * 0.5),
                    if open { 0.5 } else { 0.12 },
                    0.3,
                    0.0,
                    Box::new(move |t| {
                        let n0 = h_rng.noise();
                        hp = 0.92 * (hp + n0 - prev);
                        prev = n0;
                        hp * (-t / if open { 0.22 } else { 0.035 }).exp() * vel * 0.07
                    }),
                );
            }
        }

        if b >= 2 {
            let phrase = if b >= 4 {
                PHRASE_B[b % 4]
            } else {
                PHRASE_A[b % 4]
            };
            for &(beat, midi, dur_beats) in phrase {
                let vel = 0.85 + rng.next_f32() * 0.15;
                let f = mtof(midi);
                let dur1 = dur_beats * BEAT;
                add(
                    &mut left,
                    &mut right,
                    &mut wet_l,
                    &mut wet_r,
                    swing_at(b, beat),
                    dur1 + 0.6,
                    0.12,
                    0.35,
                    Box::new(move |t| {
                        let vib = 1.0 + 0.004 * (2.0 * PI * 5.0 * t).sin() * (t / 0.3).min(1.0);
                        let p = 2.0 * PI * f * vib * t;
                        let tri = (2.0 / PI) * p.sin().asin();
                        let sustain = if t > dur1 {
                            (-(t - dur1) / 0.12).exp()
                        } else {
                            0.85 + 0.15 * (-t / 0.3).exp()
                        };
                        let env = (t / 0.04).min(1.0) * sustain;
                        (p.sin() * 0.7 + tri * 0.3) * env * vel * 0.085
                    }),
                );
            }
        }
    }

    // Vinyl bed + crackle
    for i in 0..n {
        let crackle = if rng.next_f32() < 0.00012 {
            rng.noise() * 0.12
        } else {
            0.0
        };
        let hiss = rng.noise() * 0.0035;
        left[i] += crackle + hiss;
        right[i] += crackle * 0.8 + hiss;
    }

    // Schroeder reverb on wet send
    apply_reverb(&wet_l, &mut left, 0, sr);
    apply_reverb(&wet_r, &mut right, 23, sr);

    // Master filter sweep + soft clip + fade + normalize
    let mut l1 = 0.0_f32;
    let mut r1 = 0.0_f32;
    let mut peak = 0.0_f32;
    for i in 0..n {
        let t = i as f32 / sr;
        let open = (t / BAR).min(1.0);
        let cut = 600.0 + (9000.0 - 600.0) * open * open;
        let a = 1.0 - (-2.0 * PI * cut / sr).exp();
        l1 += (left[i] - l1) * a;
        r1 += (right[i] - r1) * a;
        let fade = (t / 0.05).min(1.0) * ((dur_secs - t) / 1.5).clamp(0.0, 1.0);
        left[i] = (l1 * 1.15).tanh() * fade;
        right[i] = (r1 * 1.15).tanh() * fade;
        peak = peak.max(left[i].abs()).max(right[i].abs());
    }
    let gain = 0.89 / peak.max(1e-6);
    for i in 0..n {
        left[i] *= gain;
        right[i] *= gain;
    }

    (left, right)
}

fn apply_reverb(inp: &[f32], out: &mut [f32], offset: usize, sr: f32) {
    let k = sr / 44_100.0;
    let mut combs: Vec<(Vec<f32>, usize, f32)> = [1116usize, 1188, 1277, 1356]
        .iter()
        .map(|&d| (vec![0.0; (((d + offset) as f32 * k).round() as usize).max(1)], 0, 0.0))
        .collect();
    let mut aps: Vec<(Vec<f32>, usize)> = [556usize, 441]
        .iter()
        .map(|&d| (vec![0.0; (((d + offset) as f32 * k).round() as usize).max(1)], 0))
        .collect();

    for (n, &x) in inp.iter().enumerate() {
        let mut s = 0.0_f32;
        for (buf, idx, lp) in &mut combs {
            let y = buf[*idx];
            *lp = y * 0.6 + *lp * 0.4;
            buf[*idx] = x + *lp * 0.8;
            *idx = (*idx + 1) % buf.len();
            s += y;
        }
        s *= 0.25;
        for (buf, idx) in &mut aps {
            let y = buf[*idx];
            let v = s + y * 0.5;
            buf[*idx] = v;
            *idx = (*idx + 1) % buf.len();
            s = y - v * 0.5;
        }
        out[n] += s;
    }
}

/// Encodes procedural lo-fi synth directly into a standard stereo 16-bit PCM WAV byte buffer.
pub fn generate_lofi_wav(dur_secs: f32, sample_rate: u32) -> Vec<u8> {
    let sr = sample_rate.max(8_000);
    let (left, right) = synthesize_lofi(dur_secs, sr);
    let num_samples = left.len();
    let data_bytes = (num_samples * 4) as u32;
    let riff_size = 36_u32.saturating_add(data_bytes);

    let mut out = Vec::with_capacity(44 + data_bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16_u32.to_le_bytes()); // PCM chunk size
    out.extend_from_slice(&1_u16.to_le_bytes()); // AudioFormat = PCM
    out.extend_from_slice(&2_u16.to_le_bytes()); // Stereo
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&(sr * 4).to_le_bytes()); // ByteRate
    out.extend_from_slice(&4_u16.to_le_bytes()); // BlockAlign
    out.extend_from_slice(&16_u16.to_le_bytes()); // BitsPerSample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());

    for i in 0..num_samples {
        let l = (left[i].clamp(-1.0, 1.0) * 32767.0).round() as i16;
        let r = (right[i].clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&l.to_le_bytes());
        out.extend_from_slice(&r.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_15s_stereo_lofi_wav_with_non_silent_normalized_audio() {
        let wav = generate_lofi_wav(15.0, 22_050);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        let expected_pcm = 15 * 22_050 * 4;
        assert_eq!(wav.len(), 44 + expected_pcm);

        let peak = wav[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(peak > 20_000, "normalized lo-fi audio must be audible, got peak {peak}");
        assert!(peak <= 30_000, "normalized peak (~0.89) must leave headroom, got {peak}");
    }
}
