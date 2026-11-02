#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use candle_core::{Device, Module, Tensor, D};

use super::source::Silence;
use super::*;

const PHONEMES: &str = "həlˈQ, ðɪs ɪz ɐ tˈɛst ɒv ðə bɹˈɪtɪʃ vˈYs.";

/// hexgrad's PyTorch Kokoro (`kokoro` at dfb907a, torch 2.1) on `PHONEMES` in
/// bf_emma with its source noise off: frames per token, the mean pitch and
/// the loudness of the 3.2 s it says. The port's waveform correlates 0.997
/// with that run and its log-mel spectrogram is 0.5 dB from it on average.
const DURATIONS: [usize; 43] = [
    13, 2, 3, 2, 6, 4, 1, 2, 2, 2, 2, 2, 2, 1, 1, 2, 3, 2, 3, 3, 3, 2, 1, 1, 1, 1, 1, 1, 2, 1, 1,
    1, 2, 3, 2, 2, 2, 2, 5, 4, 23, 8, 1,
];
const F0_MEAN: f32 = 119.19;
const SAMPLES: usize = 76_800;
const RMS: f32 = 0.065_21;

#[test]
#[ignore = "needs the downloaded speech bundle in LATTICE_KOKORO_DIR"]
fn speaks_as_the_pytorch_reference_does() {
    let dir = PathBuf::from(std::env::var_os("LATTICE_KOKORO_DIR").expect("LATTICE_KOKORO_DIR"));
    let device = Device::Cpu;
    let model = Kokoro::load(
        &dir.join("kokoro-v1_0.json"),
        &dir.join("kokoro-v1_0.pth"),
        &device,
    )
    .unwrap();
    let voice = Voice::from_bytes(&std::fs::read(dir.join("voice-bf_emma.bin")).unwrap()).unwrap();

    let ids = model.phoneme_ids(PHONEMES);
    let style = Tensor::new(voice.style(ids.len()).unwrap(), &device)
        .unwrap()
        .unsqueeze(0)
        .unwrap();
    let prosody_style = style.narrow(1, 128, 128).unwrap();
    let padded: Vec<u32> = std::iter::once(0)
        .chain(ids)
        .chain(std::iter::once(0))
        .collect();
    let input = Tensor::new(padded.as_slice(), &device)
        .unwrap()
        .unsqueeze(0)
        .unwrap();
    let d_en = model
        .bert_encoder
        .forward(&model.bert.forward(&input).unwrap())
        .unwrap();
    let encoded = model.predictor.encode(&d_en, &prosody_style).unwrap();
    let durations = model.predictor.durations(&encoded, 1.0).unwrap();
    assert_eq!(durations, DURATIONS);

    let frames: Vec<u32> = durations
        .iter()
        .enumerate()
        .flat_map(|(token, &n)| std::iter::repeat_n(token as u32, n))
        .collect();
    let frames = Tensor::new(frames.as_slice(), &device).unwrap();
    let aligned = encoded
        .index_select(&frames, 1)
        .unwrap()
        .transpose(1, 2)
        .unwrap()
        .contiguous()
        .unwrap();
    let (f0, _) = model.predictor.f0_n(&aligned, &prosody_style).unwrap();
    let f0_mean = f0
        .mean(D::Minus1)
        .unwrap()
        .squeeze(0)
        .unwrap()
        .to_scalar::<f32>()
        .unwrap();
    assert!((f0_mean - F0_MEAN).abs() < 0.05, "mean pitch {f0_mean}");

    let audio = model
        .synthesize(PHONEMES, &voice, 1.0, &mut Silence)
        .unwrap();
    assert_eq!(audio.len(), SAMPLES);
    let rms = (audio.iter().map(|s| s * s).sum::<f32>() / audio.len() as f32).sqrt();
    assert!((rms / RMS - 1.0).abs() < 0.02, "loudness {rms}");
}

#[test]
#[ignore = "needs the downloaded speech bundle in LATTICE_KOKORO_DIR"]
fn a_faster_pace_is_shorter_and_noise_is_seeded() {
    let dir = PathBuf::from(std::env::var_os("LATTICE_KOKORO_DIR").expect("LATTICE_KOKORO_DIR"));
    let model = Kokoro::load(
        &dir.join("kokoro-v1_0.json"),
        &dir.join("kokoro-v1_0.pth"),
        &Device::Cpu,
    )
    .unwrap();
    let voice = Voice::from_bytes(&std::fs::read(dir.join("voice-bf_lily.bin")).unwrap()).unwrap();
    let natural = model
        .synthesize(PHONEMES, &voice, 1.0, &mut SeededNoise::new(1))
        .unwrap();
    let again = model
        .synthesize(PHONEMES, &voice, 1.0, &mut SeededNoise::new(1))
        .unwrap();
    let faster = model
        .synthesize(PHONEMES, &voice, 1.5, &mut SeededNoise::new(1))
        .unwrap();
    assert_eq!(natural, again);
    assert!(
        faster.len() * 5 < natural.len() * 4,
        "{} vs {}",
        faster.len(),
        natural.len()
    );
}

#[test]
fn a_voice_is_whole_rows_of_styles() {
    assert!(Voice::from_bytes(&[0u8; 4 * STYLE_WIDTH * 2]).is_ok());
    assert!(Voice::from_bytes(&[0u8; 12]).is_err());
    assert!(Voice::from_bytes(&[]).is_err());
}
