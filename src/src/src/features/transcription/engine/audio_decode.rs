//! Audio decoding for the transcription engine.
//!
//! One pass turns any container symphonia understands into the mono 16 kHz f32
//! buffer whisper expects. Decode ▸ downmix ▸ resample happen together so only
//! the 16 kHz buffer is ever held; a second source-rate buffer would double peak
//! memory on long files.

use crate::shared::error::AppError;
use rubato::{FftFixedIn, Resampler};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tracing::debug;

/// Sample rate whisper's mel front-end expects.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Frames handed to the resampler per call. Small enough to keep the staging
/// buffer tiny, large enough that the FFT is not call-dominated.
const RESAMPLER_CHUNK: usize = 1024;

/// Fully decoded audio, ready for the mel front-end.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// Mono f32 at exactly 16 000 Hz, in [-1.0, 1.0].
    pub samples: Vec<f32>,
    /// Duration of `samples`.
    pub duration_ms: u64,
    /// Sample rate of the source file, before resampling.
    pub source_sample_rate: u32,
    /// Channel count of the source file, before downmixing.
    pub channels: u16,
}

fn too_long(max_secs: u64) -> AppError {
    let hours = max_secs / 3600;
    let minutes = (max_secs % 3600) / 60;
    AppError::InvalidInput(format!(
        "Audio is longer than {hours}h {minutes}m; Lattice transcribes files up to 2 hours."
    ))
}

/// Decode any container symphonia supports to mono 16 kHz f32 in one pass.
///
/// `max_secs` caps the *decoded* length. The cap is checked twice: from the
/// track's declared frame count before decoding when the container states it,
/// and by counting resampled samples during the packet loop.
pub fn decode_to_mono_16k(path: &Path, max_secs: u64) -> Result<DecodedAudio, AppError> {
    let display = path.display().to_string();
    let file = std::fs::File::open(path).map_err(|e| AppError::ContentExtraction {
        path: display.clone(),
        reason: format!("Could not open the audio file: {e}"),
    })?;

    let stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };

    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &format_opts, &MetadataOptions::default())
        .map_err(|e| AppError::ContentExtraction {
            path: display.clone(),
            reason: format!("Lattice could not read this audio file: {e}"),
        })?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| AppError::ContentExtraction {
            path: display.clone(),
            reason: "This file has no decodable audio track.".to_string(),
        })?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    let source_sample_rate = codec_params.sample_rate.ok_or_else(|| {
        AppError::InvalidData(format!("audio track has no sample rate ({display})"))
    })?;
    let channels = codec_params
        .channels
        .map(|value| value.count())
        .unwrap_or(1)
        .max(1);

    // Pre-flight the cap when the container declares a duration.
    if let (Some(n_frames), Some(time_base)) = (codec_params.n_frames, codec_params.time_base) {
        let time = time_base.calc_time(n_frames);
        let seconds = time.seconds as f64 + time.frac;
        if seconds > max_secs as f64 {
            return Err(too_long(max_secs));
        }
    }

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| AppError::ContentExtraction {
            path: display.clone(),
            reason: match path
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("wma") => "Lattice cannot decode .wma audio yet.".to_string(),
                _ => format!("Lattice cannot decode this audio codec yet: {e}"),
            },
        })?;

    let mut resampler = if source_sample_rate == TARGET_SAMPLE_RATE {
        None
    } else {
        Some(
            FftFixedIn::<f32>::new(
                source_sample_rate as usize,
                TARGET_SAMPLE_RATE as usize,
                RESAMPLER_CHUNK,
                2,
                1,
            )
            .map_err(|e| {
                AppError::InvalidData(format!("could not build the audio resampler: {e}"))
            })?,
        )
    };

    let max_samples = max_secs.saturating_mul(u64::from(TARGET_SAMPLE_RATE));
    let mut out: Vec<f32> = Vec::new();
    let mut staging: Vec<f32> = Vec::new();
    let mut interleaved: Option<SampleBuffer<f32>> = None;
    let mut decoded_any = false;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(err) => {
                return Err(AppError::ContentExtraction {
                    path: display.clone(),
                    reason: format!("Lattice could not read this audio file: {err}"),
                })
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        let audio_buf = match decoder.decode(&packet) {
            Ok(buf) => buf,
            // Recoverable: a corrupt frame in the middle of an otherwise fine file.
            Err(SymphoniaError::DecodeError(reason)) => {
                debug!(reason, "skipping malformed audio packet");
                continue;
            }
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(err) => {
                return Err(AppError::ContentExtraction {
                    path: display.clone(),
                    reason: format!("Lattice could not decode this audio file: {err}"),
                })
            }
        };

        decoded_any = true;

        let spec = *audio_buf.spec();
        let capacity = audio_buf.capacity() as u64;
        let buffer = interleaved.get_or_insert_with(|| SampleBuffer::<f32>::new(capacity, spec));
        if buffer.capacity() < audio_buf.frames() * spec.channels.count() {
            *buffer = SampleBuffer::<f32>::new(capacity, spec);
        }
        buffer.copy_interleaved_ref(audio_buf);

        // Downmix interleaved frames to mono by averaging channels.
        let frame_channels = spec.channels.count().max(1);
        let inv = 1.0 / frame_channels as f32;
        for frame in buffer.samples().chunks_exact(frame_channels) {
            staging.push(frame.iter().sum::<f32>() * inv);
        }

        match resampler.as_mut() {
            None => {
                out.append(&mut staging);
            }
            Some(resampler) => {
                let needed = resampler.input_frames_next();
                while staging.len() >= needed {
                    let chunk: Vec<f32> = staging.drain(..needed).collect();
                    let resampled = resampler.process(&[chunk], None).map_err(|e| {
                        AppError::InvalidData(format!("audio resampling failed: {e}"))
                    })?;
                    if let Some(channel) = resampled.into_iter().next() {
                        out.extend(channel);
                    }
                }
            }
        }

        if out.len() as u64 > max_samples {
            return Err(too_long(max_secs));
        }
    }

    if !decoded_any {
        return Err(AppError::ContentExtraction {
            path: display,
            reason: "This file contains no decodable audio.".to_string(),
        });
    }

    // Flush the resampler: the partial tail, then the internal delay line.
    if let Some(resampler) = resampler.as_mut() {
        if !staging.is_empty() {
            let tail: Vec<f32> = std::mem::take(&mut staging);
            let resampled = resampler
                .process_partial(Some(&[tail]), None)
                .map_err(|e| AppError::InvalidData(format!("audio resampling failed: {e}")))?;
            if let Some(channel) = resampled.into_iter().next() {
                out.extend(channel);
            }
        }
        let flushed = resampler
            .process_partial::<Vec<f32>>(None, None)
            .map_err(|e| AppError::InvalidData(format!("audio resampling failed: {e}")))?;
        if let Some(channel) = flushed.into_iter().next() {
            out.extend(channel);
        }
    }

    if out.len() as u64 > max_samples {
        return Err(too_long(max_secs));
    }

    let duration_ms = out.len() as u64 * 1000 / u64::from(TARGET_SAMPLE_RATE);

    Ok(DecodedAudio {
        samples: out,
        duration_ms,
        source_sample_rate,
        channels: u16::try_from(channels).unwrap_or(u16::MAX),
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn write_sine(path: &Path, channels: u16, sample_rate: u32, seconds: f32) {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        let frames = (sample_rate as f32 * seconds) as usize;
        for frame in 0..frames {
            let value = (2.0 * PI * 440.0 * frame as f32 / sample_rate as f32).sin();
            let sample = (value * i16::MAX as f32 * 0.5) as i16;
            for _ in 0..channels {
                writer.write_sample(sample).unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    fn fixture(name: &str, channels: u16, sample_rate: u32, seconds: f32) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write_sine(&dir.path().join(name), channels, sample_rate, seconds);
        dir
    }

    #[test]
    fn decodes_generated_wav_to_16k_mono() {
        let dir = fixture("tone.wav", 2, 44_100, 3.0);
        let decoded = decode_to_mono_16k(&dir.path().join("tone.wav"), 7200).unwrap();

        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.source_sample_rate, 44_100);
        assert!(
            (decoded.samples.len() as i64 - 48_000).abs() < 1_600,
            "expected ~48000 samples, got {}",
            decoded.samples.len()
        );
        assert!(
            (decoded.duration_ms as i64 - 3000).abs() < 100,
            "expected ~3000 ms, got {}",
            decoded.duration_ms
        );
    }

    #[test]
    fn passes_through_16k_mono_unresampled() {
        let dir = fixture("tone.wav", 1, 16_000, 1.0);
        let decoded = decode_to_mono_16k(&dir.path().join("tone.wav"), 7200).unwrap();

        assert_eq!(decoded.source_sample_rate, 16_000);
        assert_eq!(decoded.channels, 1);
        assert!(
            (decoded.samples.len() as i64 - 16_000).abs() <= 16,
            "expected ~16000 samples, got {}",
            decoded.samples.len()
        );
    }

    #[test]
    fn rejects_audio_over_the_cap() {
        let dir = fixture("tone.wav", 1, 44_100, 3.0);
        let error = decode_to_mono_16k(&dir.path().join("tone.wav"), 1).unwrap_err();
        assert!(
            matches!(error, AppError::InvalidInput(_)),
            "expected InvalidInput, got {error:?}"
        );
    }

    #[test]
    fn rejects_a_non_audio_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-really.mp3");
        std::fs::write(&path, b"this is plain text, not audio at all\n").unwrap();
        assert!(decode_to_mono_16k(&path, 7200).is_err());
    }
}
