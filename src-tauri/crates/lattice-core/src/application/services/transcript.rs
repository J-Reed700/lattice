//! Turning whisper segments into document text.
//!
//! The transcript is rendered with inline `[m:ss–m:ss]` markers rather than
//! headings because `ChunkingService::is_sentence_ending` treats `'\n'` as a
//! sentence boundary and then joins sentences with spaces — a line-based
//! detector would lose the markers, a substring scan does not. The markers are
//! read back out by `MetadataExtractor::extract_section` to produce each chunk's
//! `section`.

use crate::application::ports::transcription_port::TranscriptSegment;

/// Target speech per transcript window, in seconds.
pub const TRANSCRIPT_WINDOW_SECS: u64 = 45;

/// A run of consecutive segments rendered as one labelled block.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptWindow {
    /// Start of the first segment in the window.
    pub start_ms: u64,
    /// End of the last segment in the window.
    pub end_ms: u64,
    /// The window's speech, one space between segments.
    pub text: String,
}

/// Format a millisecond offset as `"0:07"` under an hour, `"1:02:07"` over.
///
/// The leading unit is never zero-padded, so the label reads the way a player's
/// scrubber does.
pub fn format_timestamp(ms: u64) -> String {
    let total_secs = ms / 1000;
    let seconds = total_secs % 60;
    let minutes = (total_secs / 60) % 60;
    let hours = total_secs / 3600;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Group consecutive segments into windows of at least `window_secs` of speech.
///
/// A window is closed as soon as it reaches the target, so windows run
/// `window_secs` to roughly `window_secs + one segment` in practice — a segment
/// is never split across windows.
pub fn group_segments_into_windows(
    segments: &[TranscriptSegment],
    window_secs: u64,
) -> Vec<TranscriptWindow> {
    let window_ms = window_secs.saturating_mul(1000).max(1);
    let mut windows: Vec<TranscriptWindow> = Vec::new();
    let mut current: Option<TranscriptWindow> = None;

    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }

        match current.as_mut() {
            None => {
                current = Some(TranscriptWindow {
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    text: text.to_string(),
                });
            }
            Some(window) => {
                window.end_ms = segment.end_ms.max(window.end_ms);
                window.text.push(' ');
                window.text.push_str(text);
            }
        }

        let reached_target = current
            .as_ref()
            .is_some_and(|window| window.end_ms.saturating_sub(window.start_ms) >= window_ms);

        if reached_target {
            if let Some(window) = current.take() {
                windows.push(window);
            }
        }
    }

    if let Some(window) = current.take() {
        windows.push(window);
    }

    windows
}

/// Render the windows as the document text for an audio file.
///
/// Each window is a `[start–end]` marker on its own line followed by its speech;
/// windows are separated by a blank line.
pub fn render_transcript(windows: &[TranscriptWindow]) -> String {
    windows
        .iter()
        .map(|window| {
            format!(
                "[{}\u{2013}{}]\n{}",
                format_timestamp(window.start_ms),
                format_timestamp(window.end_ms),
                window.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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

    fn segment(start_ms: u64, end_ms: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            start_ms,
            end_ms,
            text: text.to_string(),
        }
    }

    #[test]
    fn formats_timestamps() {
        assert_eq!(format_timestamp(0), "0:00");
        assert_eq!(format_timestamp(7_000), "0:07");
        assert_eq!(format_timestamp(760_000), "12:40");
        assert_eq!(format_timestamp(3_727_000), "1:02:07");
    }

    #[test]
    fn groups_segments_into_45s_windows() {
        let segments: Vec<TranscriptSegment> = (0..20)
            .map(|i| segment(i * 5_000, (i + 1) * 5_000, &format!("line {i}")))
            .collect();

        let windows = group_segments_into_windows(&segments, TRANSCRIPT_WINDOW_SECS);

        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].start_ms, 0);
        assert_eq!(windows[0].end_ms, 45_000);
        assert_eq!(windows[1].start_ms, 45_000);
        assert_eq!(windows[1].end_ms, 90_000);
        assert_eq!(windows[2].start_ms, 90_000);
        assert_eq!(windows[2].end_ms, 100_000);

        // Every segment appears exactly once, in order — none was split.
        let rendered = windows
            .iter()
            .map(|window| window.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        let expected = (0..20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(rendered, expected);
    }

    #[test]
    fn renders_markers_on_their_own_line() {
        let windows = group_segments_into_windows(
            &[
                segment(0, 45_000, "first window speech."),
                segment(45_000, 90_000, "second window speech."),
            ],
            TRANSCRIPT_WINDOW_SECS,
        );

        let text = render_transcript(&windows);

        assert!(
            text.contains("[0:00\u{2013}0:45]\n"),
            "marker missing its own line: {text}"
        );
        assert!(
            text.contains("\n\n[0:45\u{2013}1:30]\n"),
            "windows not separated by a blank line: {text}"
        );
    }
}
