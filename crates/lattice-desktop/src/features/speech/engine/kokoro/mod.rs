//! Kokoro-82M on Candle: StyleTTS 2's text and prosody networks with an
//! iSTFTNet vocoder, ported from hexgrad's reference (`kokoro/model.py`).

mod albert;
mod conv;
mod decoder;
mod generator;
mod prosody;
mod source;
mod weights;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::Path;

use candle_core::{Device, Module, Result, Tensor};
use candle_nn::Linear;

pub use source::{Noise, SeededNoise};

/// Samples per second of Kokoro's output.
pub const SAMPLE_RATE: u32 = 24_000;

/// Phonemes one forward pass reads: ALBERT's 512 positions less the two
/// boundary pads.
pub const MAX_PHONEMES: usize = 510;

const STYLE_WIDTH: usize = 256;

/// A voice pack: one style vector per utterance length, `[510, 1, 256]`.
pub struct Voice {
    styles: Vec<f32>,
}

impl Voice {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || !bytes.len().is_multiple_of(4 * STYLE_WIDTH) {
            return Err(candle_core::Error::Msg(format!(
                "a Kokoro voice is whole rows of {STYLE_WIDTH} floats; this one is {} bytes",
                bytes.len()
            )));
        }
        Ok(Self {
            styles: bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect(),
        })
    }

    /// The style the voice uses for an utterance of `phonemes` phonemes.
    fn style(&self, phonemes: usize) -> Option<&[f32]> {
        let rows = self.styles.len() / STYLE_WIDTH;
        let row = phonemes.saturating_sub(1).min(rows.saturating_sub(1));
        self.styles.get(row * STYLE_WIDTH..(row + 1) * STYLE_WIDTH)
    }
}

#[derive(serde::Deserialize)]
struct Config {
    vocab: HashMap<String, u32>,
}

pub struct Kokoro {
    vocab: HashMap<char, u32>,
    bert: albert::Albert,
    bert_encoder: Linear,
    predictor: prosody::ProsodyPredictor,
    text_encoder: prosody::TextEncoder,
    decoder: decoder::Decoder,
    device: Device,
}

impl Kokoro {
    /// `config` is the model's `config.json`; `weights` its `.pth`.
    pub fn load(config: &Path, weights: &Path, device: &Device) -> Result<Self> {
        let config: Config = serde_json::from_slice(&std::fs::read(config)?)
            .map_err(|e| candle_core::Error::Msg(format!("Kokoro config: {e}")))?;
        let vocab = config
            .vocab
            .into_iter()
            .filter_map(|(symbol, id)| {
                let mut chars = symbol.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Some((c, id)),
                    _ => None,
                }
            })
            .collect();
        let part = |name: &str| weights::Weights::load(weights, name, device);
        let bert = part("bert")?;
        let predictor = part("predictor")?;
        let text_encoder = part("text_encoder")?;
        let decoder = part("decoder")?;
        Ok(Self {
            vocab,
            bert: albert::Albert::load(&bert)?,
            bert_encoder: part("bert_encoder")?.linear("")?,
            predictor: prosody::ProsodyPredictor::load(&predictor)?,
            text_encoder: prosody::TextEncoder::load(&text_encoder)?,
            decoder: decoder::Decoder::load(&decoder)?,
            device: device.clone(),
        })
    }

    /// The token ids of the phonemes the model knows, in order.
    pub fn phoneme_ids(&self, phonemes: &str) -> Vec<u32> {
        phonemes
            .chars()
            .filter_map(|c| self.vocab.get(&c).copied())
            .collect()
    }

    /// Speak `phonemes` (at most [`MAX_PHONEMES`]) in `voice`; `speed` 1.0 is
    /// the voice's natural pace.
    pub fn synthesize(
        &self,
        phonemes: &str,
        voice: &Voice,
        speed: f32,
        noise: &mut dyn Noise,
    ) -> Result<Vec<f32>> {
        let ids = self.phoneme_ids(phonemes);
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        if ids.len() > MAX_PHONEMES {
            return Err(candle_core::Error::Msg(format!(
                "{} phonemes is more than Kokoro reads at once ({MAX_PHONEMES})",
                ids.len()
            )));
        }
        let style = voice
            .style(ids.len())
            .ok_or_else(|| candle_core::Error::Msg("the voice has no styles".into()))?;
        let padded: Vec<u32> = std::iter::once(0)
            .chain(ids)
            .chain(std::iter::once(0))
            .collect();
        self.forward(&padded, style, speed, noise)
    }

    fn forward(
        &self,
        ids: &[u32],
        style: &[f32],
        speed: f32,
        noise: &mut dyn Noise,
    ) -> Result<Vec<f32>> {
        let input = Tensor::new(ids, &self.device)?.unsqueeze(0)?;
        let style = Tensor::new(style, &self.device)?.unsqueeze(0)?;
        let decoder_style = style.narrow(1, 0, STYLE_WIDTH / 2)?;
        let prosody_style = style.narrow(1, STYLE_WIDTH / 2, STYLE_WIDTH / 2)?;

        let d_en = self.bert_encoder.forward(&self.bert.forward(&input)?)?;
        let encoded = self.predictor.encode(&d_en, &prosody_style)?;
        let durations = self.predictor.durations(&encoded, speed)?;
        let frames: Vec<u32> = durations
            .iter()
            .enumerate()
            .flat_map(|(token, &n)| std::iter::repeat_n(token as u32, n))
            .collect();
        let frames = Tensor::new(frames.as_slice(), &self.device)?;

        let aligned = encoded
            .index_select(&frames, 1)?
            .transpose(1, 2)?
            .contiguous()?;
        let (f0, n) = self.predictor.f0_n(&aligned, &prosody_style)?;
        let asr = self
            .text_encoder
            .forward(&input)?
            .index_select(&frames, 2)?;
        self.decoder.forward(&asr, &f0, &n, &decoder_style, noise)
    }
}
