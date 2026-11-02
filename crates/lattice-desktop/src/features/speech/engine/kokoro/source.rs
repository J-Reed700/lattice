//! The vocoder's signal processing in plain Rust: the harmonic-plus-noise
//! excitation built from the pitch curve (`SineGen` and its merge), and the
//! STFT (n_fft 20, hop 5, periodic Hann, centred) on either side of the
//! network. Interpolation follows `F.interpolate`'s linear, align-corners-off
//! rule so the phase matches the reference.

use std::f32::consts::PI;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const SAMPLE_RATE: f32 = 24_000.0;
pub(super) const HARMONICS: usize = 9;
const N_FFT: usize = 20;
const HOP: usize = 5;
pub(super) const BINS: usize = N_FFT / 2 + 1;
/// Samples per pitch frame: the vocoder's upsampling (10 × 6) times the hop.
const UPSAMPLE: usize = 300;
const SINE_AMP: f32 = 0.1;
const VOICED_NOISE: f32 = 0.003;
const VOICED_THRESHOLD: f32 = 10.0;

/// Where the excitation's noise comes from.
pub trait Noise {
    fn gaussian(&mut self) -> f32;
}

/// Standard normal noise from a seeded generator, so one sentence always
/// sounds the same.
pub struct SeededNoise {
    rng: StdRng,
    spare: Option<f32>,
}

impl SeededNoise {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            spare: None,
        }
    }
}

impl Noise for SeededNoise {
    fn gaussian(&mut self) -> f32 {
        if let Some(spare) = self.spare.take() {
            return spare;
        }
        let u1: f32 = 1.0 - self.rng.gen::<f32>();
        let u2: f32 = self.rng.gen::<f32>();
        let radius = (-2.0 * u1.ln()).sqrt();
        self.spare = Some(radius * (2.0 * PI * u2).sin());
        radius * (2.0 * PI * u2).cos()
    }
}

/// No noise: the reference's output with its random source switched off.
#[cfg(test)]
pub struct Silence;

#[cfg(test)]
impl Noise for Silence {
    fn gaussian(&mut self) -> f32 {
        0.0
    }
}

/// The merged excitation, one sample per output sample, from the pitch
/// curve at 80 frames a second.
pub(super) fn excitation(
    f0: &[f32],
    weights: &[f32],
    bias: f32,
    noise: &mut dyn Noise,
) -> Vec<f32> {
    // Each frame's phase advance per harmonic, accumulated in double as
    // torch's CPU cumsum does, then scaled back up to samples.
    let mut totals = [0f64; HARMONICS];
    let phases: Vec<[f32; HARMONICS]> = f0
        .iter()
        .map(|&f| {
            let mut frame = [0f32; HARMONICS];
            for (harmonic, (total, phase)) in totals.iter_mut().zip(frame.iter_mut()).enumerate() {
                let cycles = f * (harmonic + 1) as f32 / SAMPLE_RATE;
                *total += f64::from(cycles - cycles.floor());
                *phase = (*total as f32) * 2.0 * PI * UPSAMPLE as f32;
            }
            frame
        })
        .collect();

    let frames = phases.len();
    let scale = 1.0f32 / UPSAMPLE as f32;
    let mut out = Vec::with_capacity(frames * UPSAMPLE);
    for (frame, &pitch) in f0.iter().enumerate() {
        let voiced = pitch > VOICED_THRESHOLD;
        let noise_amp = if voiced { VOICED_NOISE } else { SINE_AMP / 3.0 };
        for offset in 0..UPSAMPLE {
            let sample = frame * UPSAMPLE + offset;
            let src = (scale * (sample as f32 + 0.5) - 0.5).max(0.0);
            let lower = (src as usize).min(frames.saturating_sub(1));
            let upper = (lower + 1).min(frames.saturating_sub(1));
            let weight = src - lower as f32;
            let (Some(a), Some(b)) = (phases.get(lower), phases.get(upper)) else {
                continue;
            };
            let mut merged = bias;
            for ((w, a), b) in weights.iter().zip(a).zip(b) {
                let phase = (1.0 - weight) * a + weight * b;
                let sine = if voiced {
                    (phase).sin() * SINE_AMP
                } else {
                    0.0
                };
                merged += w * (sine + noise_amp * noise.gaussian());
            }
            out.push(merged.tanh());
        }
    }
    out
}

fn hann() -> [f32; N_FFT] {
    let mut window = [0f32; N_FFT];
    for (n, w) in window.iter_mut().enumerate() {
        *w = 0.5 - 0.5 * (2.0 * PI * n as f32 / N_FFT as f32).cos();
    }
    window
}

/// `cos` and `sin` of `2πkn/N` for every bin `k` and sample `n`, exact at
/// the quarter turns so the DC and Nyquist bins stay real, as in an FFT.
fn twiddles() -> Vec<([f32; N_FFT], [f32; N_FFT])> {
    (0..BINS)
        .map(|k| {
            let mut cos = [0f32; N_FFT];
            let mut sin = [0f32; N_FFT];
            for (n, (c, s)) in cos.iter_mut().zip(sin.iter_mut()).enumerate() {
                let turn = (k * n) % N_FFT;
                (*c, *s) = if turn == 0 {
                    (1.0, 0.0)
                } else if 4 * turn == N_FFT {
                    (0.0, 1.0)
                } else if 2 * turn == N_FFT {
                    (-1.0, 0.0)
                } else if 4 * turn == 3 * N_FFT {
                    (0.0, -1.0)
                } else {
                    let angle = 2.0 * PI * turn as f32 / N_FFT as f32;
                    (angle.cos(), angle.sin())
                };
            }
            (cos, sin)
        })
        .collect()
}

/// Centred STFT of `signal` with reflect padding: the frame count, and the
/// magnitudes then phases, each laid out bin by bin across the frames.
pub(super) fn stft(signal: &[f32]) -> (usize, Vec<f32>) {
    let pad = N_FFT / 2;
    let padded: Vec<f32> = signal
        .iter()
        .skip(1)
        .take(pad)
        .rev()
        .chain(signal)
        .chain(signal.iter().rev().skip(1).take(pad))
        .copied()
        .collect();
    let frames = 1 + padded.len().saturating_sub(N_FFT) / HOP;
    let window = hann();
    let twiddles = twiddles();
    let mut magnitude = vec![0f32; BINS * frames];
    let mut phase = vec![0f32; BINS * frames];
    let mut segment = [0f32; N_FFT];
    for t in 0..frames {
        for ((s, x), w) in segment
            .iter_mut()
            .zip(padded.iter().skip(t * HOP))
            .zip(&window)
        {
            *s = x * w;
        }
        for (k, (cos, sin)) in twiddles.iter().enumerate() {
            // `+ 0.0` turns a negative zero positive, so a silent bin's phase
            // is 0 as torch's is, not ±π: the vocoder reads the phase.
            let re: f32 = segment.iter().zip(cos).map(|(x, c)| x * c).sum::<f32>() + 0.0;
            let im: f32 = -segment.iter().zip(sin).map(|(x, s)| x * s).sum::<f32>() + 0.0;
            if let Some(m) = magnitude.get_mut(k * frames + t) {
                *m = re.hypot(im);
            }
            if let Some(p) = phase.get_mut(k * frames + t) {
                *p = im.atan2(re);
            }
        }
    }
    magnitude.extend(phase);
    (frames, magnitude)
}

/// Inverse of a centred STFT from magnitude and phase rows (one per bin),
/// overlap-added and divided by the window's squared envelope.
pub(super) fn inverse_stft(magnitude: &[Vec<f32>], phase: &[Vec<f32>]) -> Vec<f32> {
    let frames = magnitude.first().map_or(0, Vec::len);
    if frames == 0 {
        return Vec::new();
    }
    let window = hann();
    let twiddles = twiddles();
    let full = N_FFT + HOP * (frames - 1);
    let mut signal = vec![0f32; full];
    let mut envelope = vec![0f32; full];
    let mut segment = [0f32; N_FFT];
    for t in 0..frames {
        segment.fill(0.0);
        for (k, ((magnitudes, phases), (cos, sin))) in
            magnitude.iter().zip(phase).zip(&twiddles).enumerate()
        {
            let (Some(m), Some(p)) = (magnitudes.get(t), phases.get(t)) else {
                continue;
            };
            // The DC and Nyquist bins are real in a one-sided inverse.
            let edge = k == 0 || k == BINS - 1;
            let re = m * p.cos();
            let im = if edge { 0.0 } else { m * p.sin() };
            let weight = if edge { 1.0 } else { 2.0 };
            for ((s, c), sn) in segment.iter_mut().zip(cos).zip(sin) {
                *s += weight * (re * c - im * sn);
            }
        }
        for (((out, env), s), w) in signal
            .iter_mut()
            .zip(envelope.iter_mut())
            .skip(t * HOP)
            .zip(&segment)
            .zip(&window)
        {
            *out += s / N_FFT as f32 * w;
            *env += w * w;
        }
    }
    let pad = N_FFT / 2;
    signal
        .into_iter()
        .zip(envelope)
        .skip(pad)
        .take(HOP * (frames - 1))
        .map(|(s, env)| if env > 1e-11 { s / env } else { s })
        .collect()
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn inverse_stft_undoes_stft() {
        let signal: Vec<f32> = (0..600)
            .map(|n| (n as f32 * 0.05).sin() * 0.3 + (n as f32 * 0.31).cos() * 0.1)
            .collect();
        let (frames, spectrum) = stft(&signal);
        assert_eq!(frames, 121);
        let rows: Vec<Vec<f32>> = spectrum.chunks(frames).map(<[f32]>::to_vec).collect();
        let (magnitude, phase) = rows.split_at(BINS);
        let restored = inverse_stft(magnitude, phase);
        assert_eq!(restored.len(), signal.len());
        for (a, b) in restored.iter().zip(&signal) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
    }

    #[test]
    fn unvoiced_frames_carry_only_noise() {
        let f0 = [0.0f32, 0.0];
        let silent = excitation(&f0, &[1.0; HARMONICS], 0.0, &mut Silence);
        assert_eq!(silent.len(), 2 * UPSAMPLE);
        assert!(silent.iter().all(|s| *s == 0.0));

        let noisy = excitation(&f0, &[1.0; HARMONICS], 0.0, &mut SeededNoise::new(7));
        assert!(noisy.iter().any(|s| s.abs() > 0.0));
    }

    #[test]
    fn a_voiced_frame_rings_at_its_pitch() {
        let f0 = [240.0f32; 8];
        let mut weights = [0.0f32; HARMONICS];
        weights[0] = 1.0;
        let wave = excitation(&f0, &weights, 0.0, &mut Silence);
        // 240 Hz at 24 kHz is a cycle every 100 samples; like torch's
        // interpolation, the phase holds still for the first and last half
        // frame, so 8 frames ring for about 7.
        let crossings = wave
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        assert!(
            (20..=22).contains(&crossings),
            "{crossings} upward crossings"
        );
    }
}
