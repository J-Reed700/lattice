//! Pure extraction helpers: text normalization, transcript parsing, and
//! building articles from video metadata.

use super::WebIngestionService;
use crate::features::function_calling::dto::CleanArticle;
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExtractionStrategy {
    Article,
    VideoMetadata,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct YtDlpMetadata {
    pub(super) title: Option<String>,
    pub(super) uploader: Option<String>,
    pub(super) channel: Option<String>,
    pub(super) description: Option<String>,
    pub(super) webpage_url: Option<String>,
    pub(super) original_url: Option<String>,
    pub(super) upload_date: Option<String>,
    pub(super) duration: Option<f64>,
    pub(super) extractor: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Json3Transcript {
    events: Option<Vec<Json3Event>>,
}

#[derive(Debug, Deserialize, Default)]
struct Json3Event {
    segs: Option<Vec<Json3Segment>>,
}

#[derive(Debug, Deserialize, Default)]
struct Json3Segment {
    utf8: Option<String>,
}

impl WebIngestionService {
    fn host_matches_domain(host: &str, base_domain: &str) -> bool {
        host == base_domain || host.ends_with(&format!(".{}", base_domain))
    }

    pub(super) fn normalize_whitespace(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    fn parse_yt_dlp_upload_date(&self, value: &str) -> Option<DateTime<Utc>> {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        let datetime = date.and_hms_opt(0, 0, 0)?;
        Some(DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc))
    }

    fn calculate_reading_time_minutes(word_count: usize) -> i64 {
        ((word_count as f64 / 200.0).ceil() as i64).max(1)
    }

    fn generate_excerpt(text: &str) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        if trimmed.len() <= 200 {
            return Some(trimmed.to_string());
        }

        let truncated = &trimmed[..crate::shared::text_utils::floor_char_boundary(trimmed, 200)];
        if let Some(last_space) = truncated.rfind(' ') {
            Some(format!("{}...", &trimmed[..last_space]))
        } else {
            Some(format!("{}...", truncated))
        }
    }

    fn strip_html_like_tags(text: &str) -> String {
        let mut output = String::new();
        let mut in_tag = false;

        for ch in text.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => output.push(ch),
                _ => {}
            }
        }

        output
    }

    fn dedupe_consecutive_lines(lines: Vec<String>) -> Vec<String> {
        let mut deduped = Vec::new();

        for line in lines {
            if deduped
                .last()
                .is_some_and(|previous: &String| previous == &line)
            {
                continue;
            }
            deduped.push(line);
        }

        deduped
    }

    pub(super) fn parse_vtt_transcript_text(contents: &str) -> String {
        let mut lines = Vec::new();
        let mut skip_note_block = false;

        for raw_line in contents.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                skip_note_block = false;
                continue;
            }

            if trimmed.starts_with("NOTE") {
                skip_note_block = true;
                continue;
            }

            if skip_note_block
                || trimmed == "WEBVTT"
                || trimmed.starts_with("Kind:")
                || trimmed.starts_with("Language:")
                || trimmed.contains("-->")
                || trimmed.parse::<usize>().is_ok()
            {
                continue;
            }

            let cleaned = Self::normalize_whitespace(
                &Self::strip_html_like_tags(trimmed).replace("&nbsp;", " "),
            );
            if !cleaned.is_empty() {
                lines.push(cleaned);
            }
        }

        Self::dedupe_consecutive_lines(lines).join("\n")
    }

    pub(super) fn parse_srt_transcript_text(contents: &str) -> String {
        let mut lines = Vec::new();

        for raw_line in contents.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.contains("-->") || trimmed.parse::<usize>().is_ok() {
                continue;
            }

            let cleaned = Self::normalize_whitespace(&Self::strip_html_like_tags(trimmed));
            if !cleaned.is_empty() {
                lines.push(cleaned);
            }
        }

        Self::dedupe_consecutive_lines(lines).join("\n")
    }

    pub(super) fn parse_json3_transcript_text(contents: &str) -> Option<String> {
        let parsed = serde_json::from_str::<Json3Transcript>(contents).ok()?;
        let mut lines = Vec::new();

        for event in parsed.events.unwrap_or_default() {
            for segment in event.segs.unwrap_or_default() {
                if let Some(text) = segment.utf8 {
                    let cleaned = Self::normalize_whitespace(&Self::strip_html_like_tags(&text));
                    if !cleaned.is_empty() {
                        lines.push(cleaned);
                    }
                }
            }
        }

        let transcript = Self::dedupe_consecutive_lines(lines).join("\n");
        if transcript.is_empty() {
            None
        } else {
            Some(transcript)
        }
    }

    pub(super) fn build_video_article_with_transcript(
        &self,
        source_url: &str,
        metadata: YtDlpMetadata,
        transcript: &str,
    ) -> Result<CleanArticle> {
        let mut article = self.build_video_article_from_metadata(source_url, metadata)?;
        let normalized_transcript = Self::normalize_whitespace(transcript);
        if normalized_transcript.is_empty() {
            return Ok(article);
        }

        article.text_content = format!(
            "{}\n\nTranscript:\n{}",
            article.text_content, normalized_transcript
        );
        article.word_count = article.text_content.split_whitespace().count();
        article.reading_time_minutes = Self::calculate_reading_time_minutes(article.word_count);
        article.excerpt = Self::generate_excerpt(&article.text_content);

        let transcript_html = Self::escape_html(&normalized_transcript).replace('\n', "<br/>");
        if let Some(stripped) = article.content.strip_suffix("</article>") {
            article.content = format!(
                "{}<h2>Transcript</h2><p>{}</p></article>",
                stripped, transcript_html
            );
        } else {
            article.content = format!(
                "{}<h2>Transcript</h2><p>{}</p>",
                article.content, transcript_html
            );
        }

        Ok(article)
    }

    pub(super) fn is_likely_video_url(&self, url: &str) -> bool {
        let parsed = match url::Url::parse(url) {
            Ok(parsed) => parsed,
            Err(_) => return false,
        };

        let host = parsed
            .host_str()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if host.is_empty() {
            return false;
        }

        let known_video_hosts = [
            "youtube.com",
            "youtu.be",
            "vimeo.com",
            "dailymotion.com",
            "tiktok.com",
            "twitch.tv",
            "twitter.com",
            "x.com",
            "instagram.com",
            "facebook.com",
            "rumble.com",
        ];

        if known_video_hosts
            .iter()
            .any(|site| Self::host_matches_domain(&host, site))
        {
            return true;
        }

        let path = parsed.path().to_ascii_lowercase();
        [".mp4", ".mov", ".m4v", ".webm", ".mkv"]
            .iter()
            .any(|ext| path.ends_with(ext))
    }

    pub(super) fn extraction_strategies_for_url(&self, url: &str) -> Vec<ExtractionStrategy> {
        if !self.config.enable_video_fallback {
            return vec![ExtractionStrategy::Article];
        }

        if self.is_likely_video_url(url) {
            vec![
                ExtractionStrategy::VideoMetadata,
                ExtractionStrategy::Article,
            ]
        } else {
            vec![ExtractionStrategy::Article]
        }
    }

    pub(super) fn build_video_article_from_metadata(
        &self,
        source_url: &str,
        metadata: YtDlpMetadata,
    ) -> Result<CleanArticle> {
        let title = metadata
            .title
            .map(|value| Self::normalize_whitespace(&value))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Video".to_string());

        let author = metadata
            .uploader
            .or(metadata.channel)
            .map(|value| Self::normalize_whitespace(&value))
            .filter(|value| !value.is_empty());

        let description = metadata
            .description
            .map(|value| Self::normalize_whitespace(&value))
            .filter(|value| !value.is_empty());

        let canonical_url = metadata
            .webpage_url
            .or(metadata.original_url)
            .unwrap_or_else(|| source_url.to_string());

        let published_date = metadata
            .upload_date
            .as_ref()
            .and_then(|value| self.parse_yt_dlp_upload_date(value));

        let duration_seconds = metadata.duration.unwrap_or(0.0).max(0.0).round() as i64;
        let duration_text = if duration_seconds > 0 {
            format!("Duration: {} seconds", duration_seconds)
        } else {
            "Duration: Unknown".to_string()
        };

        let extractor_text = metadata
            .extractor
            .map(|value| Self::normalize_whitespace(&value))
            .filter(|value| !value.is_empty())
            .map(|value| format!("Extractor: {}", value));

        let mut text_sections = vec![format!("Title: {}", title)];
        if let Some(author_value) = &author {
            text_sections.push(format!("Creator: {}", author_value));
        }
        text_sections.push(duration_text.clone());
        if let Some(description_value) = &description {
            text_sections.push(description_value.clone());
        }
        if let Some(extractor_value) = &extractor_text {
            text_sections.push(extractor_value.clone());
        }
        text_sections.push(format!("Source: {}", canonical_url));

        let text_content = text_sections.join("\n\n");
        let word_count = text_content.split_whitespace().count();
        let reading_time_minutes = Self::calculate_reading_time_minutes(word_count);
        let excerpt = Self::generate_excerpt(&text_content);

        if word_count == 0 {
            return Err(AppError::ContentExtraction {
                path: source_url.to_string(),
                reason: "Video metadata extraction produced empty content".to_string(),
            });
        }

        let mut content_parts = vec![format!("<h1>{}</h1>", Self::escape_html(&title))];
        if let Some(author_value) = &author {
            content_parts.push(format!(
                "<p><strong>Creator:</strong> {}</p>",
                Self::escape_html(author_value)
            ));
        }
        content_parts.push(format!("<p>{}</p>", Self::escape_html(&duration_text)));
        if let Some(description_value) = &description {
            content_parts.push(format!("<p>{}</p>", Self::escape_html(description_value)));
        }
        if let Some(extractor_value) = &extractor_text {
            content_parts.push(format!(
                "<p><strong>{}</strong></p>",
                Self::escape_html(extractor_value)
            ));
        }
        content_parts.push(format!(
            "<p><a href=\"{}\">Open source video</a></p>",
            Self::escape_html(&canonical_url)
        ));

        Ok(CleanArticle {
            title,
            author,
            content: format!("<article>{}</article>", content_parts.join("")),
            text_content,
            word_count,
            reading_time_minutes,
            published_date,
            excerpt,
        })
    }
}
