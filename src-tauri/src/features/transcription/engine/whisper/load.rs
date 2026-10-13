//! Loading a downloaded whisper model: its config, mel filterbank, tokenizer
//! and quantized weights, on the best available device.

use std::path::Path;

use candle_core::{Device, Tensor};
use candle_transformers::models::whisper::{self as whisper, quantized_model, Config};
use candle_transformers::quantized_var_builder::VarBuilder;
use tokenizers::Tokenizer;
use tracing::info;

use super::{
    LoadedWhisper, ResolvedModel, BUNDLED_MEL_FILTERS_80, MEL_FILTER_WIDTH, SUPPORTED_MEL_BINS,
};
use crate::shared::error::AppError;

fn read_json_config(dir: &Path, model_id: &str) -> Result<Config, AppError> {
    let path = dir.join("config.json");
    let bytes = std::fs::read(&path).map_err(|e| {
        AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' is incomplete: config.json is missing ({e})"
        ))
    })?;
    serde_json::from_slice::<Config>(&bytes).map_err(|e| {
        AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' has an unreadable config.json: {e}"
        ))
    })
}

fn read_mel_filters(dir: &Path, model_id: &str, num_mel_bins: usize) -> Result<Vec<f32>, AppError> {
    // A model directory may ship its own table (a future 128-mel model must);
    // otherwise fall back to the bundled 80-bin filterbank.
    let path = dir.join("melfilters.bytes");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) if num_mel_bins == SUPPORTED_MEL_BINS => BUNDLED_MEL_FILTERS_80.to_vec(),
        Err(e) => {
            return Err(AppError::ModelLoadFailed(format!(
                "Transcription model '{model_id}' is incomplete: melfilters.bytes is missing ({e})"
            )))
        }
    };

    let filters: Vec<f32> = bytes
        .chunks_exact(4)
        .filter_map(|chunk| <[u8; 4]>::try_from(chunk).ok().map(f32::from_le_bytes))
        .collect();

    let expected = num_mel_bins * MEL_FILTER_WIDTH;
    if filters.len() != expected {
        return Err(AppError::InvalidData(format!(
            "Transcription model '{model_id}' has {} mel filter values, expected {expected}",
            filters.len()
        )));
    }

    Ok(filters)
}

fn token_id(tokenizer: &Tokenizer, token: &str, model_id: &str) -> Result<u32, AppError> {
    tokenizer.token_to_id(token).ok_or_else(|| {
        AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' has no '{token}' token in its tokenizer"
        ))
    })
}

/// Collect the `<|xx|>` language tokens from the tokenizer vocabulary.
///
/// Deriving them from the vocab avoids embedding a 99-language table that would
/// drift from whatever model the user downloaded. Task/control tokens
/// (`<|translate|>`, `<|nospeech|>`, …) are longer than three characters and
/// timestamp tokens contain digits, so a 2–3 lowercase-letter tag selects
/// exactly the language set.
fn collect_language_tokens(tokenizer: &Tokenizer) -> Vec<(u32, String)> {
    let mut tokens: Vec<(u32, String)> = tokenizer
        .get_vocab(true)
        .into_iter()
        .filter_map(|(token, id)| {
            let tag = token.strip_prefix("<|")?.strip_suffix("|>")?;
            let is_language_tag =
                (2..=3).contains(&tag.len()) && tag.chars().all(|c| c.is_ascii_lowercase());
            is_language_tag.then(|| (id, tag.to_string()))
        })
        .collect();
    tokens.sort_by_key(|(id, _)| *id);
    tokens
}

/// Use the same platform policy as embeddings: Metal on Apple Silicon,
/// CPU on Intel Macs, with recoverable accelerator initialization.
fn best_device() -> Device {
    crate::infrastructure::ml::compute_device::best_available_compute_device("transcription")
}

pub(super) fn load_whisper(resolved: &ResolvedModel) -> Result<LoadedWhisper, AppError> {
    let model_id = resolved.model_id.as_str();
    let dir = resolved.dir.as_path();

    let config = read_json_config(dir, model_id)?;
    if config.num_mel_bins != SUPPORTED_MEL_BINS {
        return Err(AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' uses {} mel bins; Lattice supports {SUPPORTED_MEL_BINS}-bin whisper models (base, small).",
            config.num_mel_bins
        )));
    }

    let mel_filters = read_mel_filters(dir, model_id, config.num_mel_bins)?;

    let tokenizer_path = dir.join("tokenizer.json");
    let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
        AppError::TokenizationError {
            reason: format!(
                "Transcription model '{model_id}' is incomplete: tokenizer.json could not be read ({e})"
            ),
        }
    })?;

    let weights_path = dir.join("model.gguf");
    if !weights_path.is_file() {
        return Err(AppError::ModelLoadFailed(format!(
            "Transcription model '{model_id}' is incomplete: model.gguf is missing"
        )));
    }

    let device = best_device();
    let var_builder = VarBuilder::from_gguf(&weights_path, &device)
        .map_err(|e| AppError::ModelLoadFailed(format!("whisper weights failed to load: {e}")))?;
    let model = quantized_model::Whisper::load(&var_builder, config.clone())
        .map_err(|e| AppError::ModelLoadFailed(format!("whisper weights failed to load: {e}")))?;

    let sot = token_id(&tokenizer, whisper::SOT_TOKEN, model_id)?;
    let eot = token_id(&tokenizer, whisper::EOT_TOKEN, model_id)?;
    let transcribe = token_id(&tokenizer, whisper::TRANSCRIBE_TOKEN, model_id)?;
    let no_timestamps = token_id(&tokenizer, whisper::NO_TIMESTAMPS_TOKEN, model_id)?;
    let no_speech = whisper::NO_SPEECH_TOKENS
        .iter()
        .find_map(|token| tokenizer.token_to_id(token))
        .ok_or_else(|| {
            AppError::ModelLoadFailed(format!(
                "Transcription model '{model_id}' has no no-speech token in its tokenizer"
            ))
        })?;

    // Suppressed logits, built once. `no_timestamps` is suppressed because we
    // always decode *with* timestamps — the timestamps are the whole point.
    let suppress: Vec<f32> = (0..config.vocab_size as u32)
        .map(|token| {
            if config.suppress_tokens.contains(&token) || token == no_timestamps {
                f32::NEG_INFINITY
            } else {
                0.0
            }
        })
        .collect();
    let suppress = Tensor::new(suppress.as_slice(), &device)
        .map_err(|e| AppError::ModelLoadFailed(format!("whisper weights failed to load: {e}")))?;

    let language_tokens = collect_language_tokens(&tokenizer);

    info!(
        model_id,
        device = ?device,
        multilingual = !language_tokens.is_empty(),
        "transcription model loaded"
    );

    Ok(LoadedWhisper {
        model_id: resolved.model_id.clone(),
        model,
        tokenizer,
        config,
        mel_filters,
        device,
        sot,
        eot,
        transcribe,
        no_timestamps,
        no_speech,
        suppress,
        language_tokens,
    })
}
