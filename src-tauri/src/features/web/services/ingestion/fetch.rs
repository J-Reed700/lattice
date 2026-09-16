//! Network and process-backed fetching: yt-dlp metadata and subtitles, audio
//! downloads, ASR transcription, and the extraction strategy pipeline.

use super::extract::{ExtractionStrategy, YtDlpMetadata};
use super::WebIngestionService;
use crate::features::function_calling::dto::CleanArticle;
use crate::shared::error::{AppError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs as async_fs;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};
use uuid::Uuid;

#[derive(Debug)]
pub(super) struct TempDirectoryGuard {
    path: PathBuf,
}

impl TempDirectoryGuard {
    pub(super) fn new(prefix: &str) -> Result<Self> {
        let path = std::env::temp_dir().join(format!("{}-{}", prefix, Uuid::new_v4()));
        fs::create_dir_all(&path).map_err(|e| {
            AppError::FileSystem(format!(
                "Failed to create temporary directory '{}': {}",
                path.display(),
                e
            ))
        })?;

        Ok(Self { path })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectoryGuard {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.path) {
            debug!(
                path = %self.path.display(),
                error = %error,
                "Failed to cleanup temporary directory"
            );
        }
    }
}

impl WebIngestionService {
    fn extract_transcript_text_from_file(path: &Path) -> Option<String> {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)?;

        let contents = fs::read_to_string(path).ok()?;
        let transcript = match extension.as_str() {
            "vtt" => Self::parse_vtt_transcript_text(&contents),
            "srt" => Self::parse_srt_transcript_text(&contents),
            "json3" => Self::parse_json3_transcript_text(&contents)?,
            _ => return None,
        };

        if transcript.is_empty() {
            None
        } else {
            Some(transcript)
        }
    }

    fn subtitle_file_score(path: &Path) -> i32 {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        let mut score = match extension.as_str() {
            "vtt" => 50,
            "srt" => 40,
            "json3" => 30,
            _ => 0,
        };

        if file_name.contains(".en.")
            || file_name.contains(".en-")
            || file_name.ends_with(".en.vtt")
        {
            score += 100;
        } else if file_name.contains("english") {
            score += 80;
        }

        score
    }

    pub(super) fn load_transcript_from_directory(directory: &Path) -> Option<String> {
        let mut candidates: Vec<PathBuf> = fs::read_dir(directory)
            .ok()?
            .filter_map(|entry| entry.ok().map(|value| value.path()))
            .filter(|path| path.is_file())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "vtt" | "srt" | "json3"))
                    .unwrap_or(false)
            })
            .collect();

        candidates.sort_by(|a, b| Self::subtitle_file_score(b).cmp(&Self::subtitle_file_score(a)));

        for candidate in candidates {
            if let Some(transcript) = Self::extract_transcript_text_from_file(&candidate) {
                let word_count = transcript.split_whitespace().count();
                if word_count >= 20 {
                    return Some(transcript);
                }
            }
        }

        None
    }

    fn audio_file_score(path: &Path) -> i32 {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        match extension.as_str() {
            "wav" => 100,
            "m4a" => 80,
            "mp3" => 70,
            "opus" => 60,
            "webm" => 50,
            _ => 0,
        }
    }

    pub(super) fn find_best_audio_file(directory: &Path) -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = fs::read_dir(directory)
            .ok()?
            .filter_map(|entry| entry.ok().map(|value| value.path()))
            .filter(|path| path.is_file())
            .filter(|path| Self::audio_file_score(path) > 0)
            .collect();

        candidates.sort_by(|a, b| Self::audio_file_score(b).cmp(&Self::audio_file_score(a)));
        candidates.into_iter().next()
    }

    fn normalize_transcript_for_storage(transcript: &str) -> String {
        transcript
            .lines()
            .map(Self::normalize_whitespace)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn download_audio_for_asr(&self, url: &str, directory: &Path) -> Result<PathBuf> {
        let command_future = Command::new(&self.config.yt_dlp_binary)
            .arg("-f")
            .arg("bestaudio/best")
            .arg("--extract-audio")
            .arg("--audio-format")
            .arg("wav")
            .arg("--audio-quality")
            .arg("0")
            .arg("--paths")
            .arg(directory)
            .arg("-o")
            .arg("%(id)s.%(ext)s")
            .arg("--no-warnings")
            .arg("--")
            .arg(url)
            .kill_on_drop(true)
            .output();

        let output = timeout(
            Duration::from_secs(self.config.yt_dlp_timeout_secs),
            command_future,
        )
        .await
        .map_err(|_| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "yt-dlp audio download timed out after {} seconds",
                self.config.yt_dlp_timeout_secs
            ),
        })?
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "Failed to execute yt-dlp for audio download '{}': {}",
                self.config.yt_dlp_binary, e
            ),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = Self::normalize_whitespace(stderr.as_ref());
            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("yt-dlp audio download failed: {}", stderr),
            });
        }

        let best_audio = tokio::task::spawn_blocking({
            let directory = directory.to_path_buf();
            move || Self::find_best_audio_file(&directory)
        })
        .await
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!("Failed to inspect downloaded audio files: {}", e),
        })?;

        best_audio.ok_or_else(|| AppError::ContentExtraction {
            path: url.to_string(),
            reason: "yt-dlp completed but no audio file was produced".to_string(),
        })
    }

    fn find_best_asr_output_file(directory: &Path, audio_stem: &str) -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = fs::read_dir(directory)
            .ok()?
            .filter_map(|entry| entry.ok().map(|value| value.path()))
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("txt"))
                        .unwrap_or(false)
            })
            .collect();

        candidates.sort_by(|a, b| {
            let a_name = a
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let b_name = b
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();

            let a_score = if a_name.contains(&audio_stem.to_ascii_lowercase()) {
                1
            } else {
                0
            };
            let b_score = if b_name.contains(&audio_stem.to_ascii_lowercase()) {
                1
            } else {
                0
            };

            b_score.cmp(&a_score)
        });

        candidates.into_iter().next()
    }

    async fn transcribe_audio_with_asr(&self, url: &str, audio_path: &Path) -> Result<String> {
        let output_directory = audio_path.parent().ok_or_else(|| {
            AppError::FileSystem("Audio path missing parent directory".to_string())
        })?;

        let mut command = Command::new(&self.config.asr_binary);
        command.arg(audio_path);
        command.arg("--model").arg(&self.config.asr_model);
        command.arg("--output_format").arg("txt");
        command.arg("--output_dir").arg(output_directory);
        command.arg("--fp16").arg("False");
        if let Some(language) = &self.config.asr_language {
            if !language.trim().is_empty() {
                command.arg("--language").arg(language);
            }
        }
        command.kill_on_drop(true);

        let output = timeout(
            Duration::from_secs(self.config.asr_timeout_secs),
            command.output(),
        )
        .await
        .map_err(|_| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "ASR transcription timed out after {} seconds",
                self.config.asr_timeout_secs
            ),
        })?
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "Failed to execute ASR binary '{}': {}",
                self.config.asr_binary, e
            ),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = Self::normalize_whitespace(stderr.as_ref());
            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("ASR transcription failed: {}", stderr),
            });
        }

        let audio_stem = audio_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        let transcript_path = tokio::task::spawn_blocking({
            let output_directory = output_directory.to_path_buf();
            let audio_stem = audio_stem.to_string();
            move || Self::find_best_asr_output_file(&output_directory, &audio_stem)
        })
        .await
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!("Failed to inspect ASR output files: {}", e),
        })?
        .ok_or_else(|| AppError::ContentExtraction {
            path: url.to_string(),
            reason: "ASR completed but transcript file was not found".to_string(),
        })?;

        let transcript = async_fs::read_to_string(&transcript_path)
            .await
            .map_err(|e| AppError::FileRead {
                path: transcript_path.display().to_string(),
                reason: e.to_string(),
            })?;

        let normalized = Self::normalize_transcript_for_storage(&transcript);
        if normalized.is_empty() {
            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: "ASR produced an empty transcript".to_string(),
            });
        }

        Ok(normalized)
    }

    async fn extract_transcript_with_asr(&self, url: &str, directory: &Path) -> Result<String> {
        let audio_path = self.download_audio_for_asr(url, directory).await?;
        self.transcribe_audio_with_asr(url, &audio_path).await
    }

    async fn extract_video_metadata_with_ytdlp(&self, url: &str) -> Result<CleanArticle> {
        let temp_dir = TempDirectoryGuard::new("lattice-ytdlp")?;

        let command_future = Command::new(&self.config.yt_dlp_binary)
            .arg("--dump-single-json")
            .arg("--skip-download")
            .arg("--write-sub")
            .arg("--write-auto-sub")
            .arg("--sub-langs")
            .arg("en.*,en,-live_chat")
            .arg("--sub-format")
            .arg("vtt/best")
            .arg("--paths")
            .arg(temp_dir.path())
            .arg("-o")
            .arg("%(id)s.%(ext)s")
            .arg("--no-warnings")
            .arg("--")
            .arg(url)
            .kill_on_drop(true)
            .output();

        let output = timeout(
            Duration::from_secs(self.config.yt_dlp_timeout_secs),
            command_future,
        )
        .await
        .map_err(|_| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "yt-dlp timed out after {} seconds",
                self.config.yt_dlp_timeout_secs
            ),
        })?
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!(
                "Failed to execute yt-dlp '{}': {}",
                self.config.yt_dlp_binary, e
            ),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = Self::normalize_whitespace(stderr.as_ref());
            let stderr_preview = if stderr.chars().count() > 280 {
                format!("{}...", stderr.chars().take(280).collect::<String>())
            } else {
                stderr
            };

            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("yt-dlp extraction failed: {}", stderr_preview),
            });
        }

        let stdout = String::from_utf8(output.stdout).map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!("yt-dlp returned non-UTF8 output: {}", e),
        })?;

        let metadata: YtDlpMetadata =
            serde_json::from_str(&stdout).map_err(|e| AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("Failed to parse yt-dlp JSON output: {}", e),
            })?;

        let subtitle_transcript = tokio::task::spawn_blocking({
            let subtitles_dir = temp_dir.path().to_path_buf();
            move || Self::load_transcript_from_directory(&subtitles_dir)
        })
        .await
        .map_err(|e| AppError::ContentExtraction {
            path: url.to_string(),
            reason: format!("Failed to load subtitle transcript files: {}", e),
        })?;

        if let Some(transcript) = subtitle_transcript {
            return self.build_video_article_with_transcript(url, metadata, &transcript);
        }

        if self.config.enable_asr_fallback {
            match self.extract_transcript_with_asr(url, temp_dir.path()).await {
                Ok(transcript) => {
                    return self.build_video_article_with_transcript(url, metadata, &transcript);
                }
                Err(error) => {
                    warn!(url, error = %error, "ASR transcript fallback failed");
                }
            }
        }

        self.build_video_article_from_metadata(url, metadata)
    }

    pub(super) async fn extract_content_with_strategies(&self, url: &str) -> Result<CleanArticle> {
        let mut article_error: Option<AppError> = None;
        let mut video_error: Option<AppError> = None;

        for strategy in self.extraction_strategies_for_url(url) {
            let result = match strategy {
                ExtractionStrategy::Article => {
                    self.article_extractor.extract_article_from_url(url).await
                }
                ExtractionStrategy::VideoMetadata => {
                    self.extract_video_metadata_with_ytdlp(url).await
                }
            };

            match result {
                Ok(article) => {
                    debug!(?strategy, url, "Web extraction strategy succeeded");
                    return Ok(article);
                }
                Err(error) => {
                    warn!(?strategy, url, error = %error, "Web extraction strategy failed");
                    match strategy {
                        ExtractionStrategy::Article => article_error = Some(error),
                        ExtractionStrategy::VideoMetadata => video_error = Some(error),
                    }
                }
            }
        }

        let preferred_error = if self.is_likely_video_url(url) {
            video_error.or(article_error)
        } else {
            article_error.or(video_error)
        };

        Err(
            preferred_error.unwrap_or_else(|| AppError::ContentExtraction {
                path: url.to_string(),
                reason: "No web extraction strategy produced content".to_string(),
            }),
        )
    }
}
