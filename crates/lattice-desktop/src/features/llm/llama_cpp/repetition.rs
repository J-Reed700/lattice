//! Detects a model output that has fallen into a repetition loop.

/// Minimum bytes before checking for a degenerate repetition loop.
const LOOP_CHECK_MIN_BYTES: usize = 4_096;
/// Bytes of new output between consecutive loop checks.
pub(super) const LOOP_CHECK_STRIDE_BYTES: usize = 256;
/// Sliding tail inspected for repetition loops.
const LOOP_WINDOW_BYTES: usize = 24_576;
/// Consecutive identical copies of a period that confirm a runaway loop.
const CONSECUTIVE_LOOP_REPEATS: usize = 6;
/// Minimum and maximum period (in bytes) for consecutive cycle checks.
const MIN_CYCLE_BYTES: usize = 24;
const MAX_CYCLE_BYTES: usize = 2_048;
/// Non-adjacent interleaved-loop check: requires BOTH a repeated trailing block
/// AND collapsed novelty across a full tail window, so valid structured JSON
/// arrays that legitimately repeat shared source quotes across distinct items
/// never false-positive.
const INTERLEAVED_MIN_WINDOW_BYTES: usize = 16_384;
const REPEAT_BLOCK_BYTES: usize = 128;
const REPEAT_BLOCK_MIN_NON_WS: usize = 80;
const REPEAT_BLOCK_OCCURRENCES: usize = 8;
const NOVELTY_CHUNK_BYTES: usize = 64;
/// Maximum unique-chunk percentage (0..=100) for an interleaved loop: at least
/// 75% of the tail window must be verbatim recycled chunks.
const INTERLEAVED_MAX_UNIQUE_PERCENT: usize = 25;
/// Minimum normalized words in the tail window before checking 4-word shingle
/// collapse (catches paraphrased or DRY-perturbed loops where one word varies
/// per cycle so exact-byte checks miss them).
const SHINGLE_MIN_WORDS: usize = 1_500;
/// Maximum unique 4-word shingle percentage (0..=100) confirming a perturbed loop.
const SHINGLE_MAX_UNIQUE_PERCENT: usize = 18;
/// Minimum share (0..=100) of 4-word shingles in the newer half of the tail
/// window that already appeared in the older half.
const SHINGLE_MIN_SECOND_HALF_REPEAT_PERCENT: usize = 82;
/// Consecutive trailing ASCII whitespace bytes confirming a whitespace runaway
/// (for example a JSON-grammar `ws` loop), far beyond any normal indentation.
const WHITESPACE_RUNAWAY_BYTES: usize = 2_048;

fn has_shingle_collapse(window: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(window) else {
        return false;
    };
    let words: Vec<String> = text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    if words.len() < SHINGLE_MIN_WORDS {
        return false;
    }
    let total = words.len().saturating_sub(3);
    let (older, newer) = words.split_at(words.len() / 2);
    let mut first_half = std::collections::HashSet::with_capacity(older.len());
    for shingle in older.windows(4) {
        first_half.insert(shingle);
    }
    let mut all_unique = first_half.clone();
    let mut second_total = 0usize;
    let mut second_repeated = 0usize;
    for shingle in newer.windows(4) {
        all_unique.insert(shingle);
        second_total += 1;
        if first_half.contains(shingle) {
            second_repeated += 1;
        }
    }
    if second_total == 0 || total == 0 {
        return false;
    }
    let unique_collapsed =
        all_unique.len().saturating_mul(100) <= total.saturating_mul(SHINGLE_MAX_UNIQUE_PERCENT);
    let second_half_recycled = second_repeated.saturating_mul(100)
        >= second_total.saturating_mul(SHINGLE_MIN_SECOND_HALF_REPEAT_PERCENT);
    unique_collapsed && second_half_recycled
}

/// Detects a confirmed degenerate repetition loop in the tail of streamed
/// output without imposing any length limit on unique content.
pub(super) fn has_repetition_loop(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < LOOP_CHECK_MIN_BYTES {
        return false;
    }
    let start = bytes.len().saturating_sub(LOOP_WINDOW_BYTES);
    let Some(window) = bytes.get(start..) else {
        return false;
    };
    if window
        .get(window.len().saturating_sub(WHITESPACE_RUNAWAY_BYTES)..)
        .is_some_and(|tail| {
            tail.len() == WHITESPACE_RUNAWAY_BYTES && tail.iter().all(u8::is_ascii_whitespace)
        })
    {
        return true;
    }
    let max_period = MAX_CYCLE_BYTES.min(window.len() / CONSECUTIVE_LOOP_REPEATS);
    if max_period >= MIN_CYCLE_BYTES {
        for period in MIN_CYCLE_BYTES..=max_period {
            let Some(unit) = window.get(window.len() - period..) else {
                continue;
            };
            if unit.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let repeated = (1..CONSECUTIVE_LOOP_REPEATS).all(|copy| {
                let end = window.len() - copy * period;
                window.get(end - period..end) == Some(unit)
            });
            if repeated {
                return true;
            }
        }
    }
    if window.len() >= INTERLEAVED_MIN_WINDOW_BYTES {
        if let Some(needle) = window.get(window.len().saturating_sub(REPEAT_BLOCK_BYTES)..) {
            if needle.len() == REPEAT_BLOCK_BYTES
                && needle
                    .iter()
                    .filter(|byte| !byte.is_ascii_whitespace())
                    .count()
                    >= REPEAT_BLOCK_MIN_NON_WS
            {
                let mut count = 0usize;
                let mut cursor = 0usize;
                while let Some(rest) = window.get(cursor..) {
                    let Some(found) = rest
                        .windows(REPEAT_BLOCK_BYTES)
                        .position(|slice| slice == needle)
                    else {
                        break;
                    };
                    count += 1;
                    if count >= REPEAT_BLOCK_OCCURRENCES {
                        break;
                    }
                    cursor = cursor
                        .saturating_add(found)
                        .saturating_add(REPEAT_BLOCK_BYTES);
                }
                if count >= REPEAT_BLOCK_OCCURRENCES {
                    let (chunks, _) = window.as_chunks::<NOVELTY_CHUNK_BYTES>();
                    let unique: std::collections::HashSet<&[u8; NOVELTY_CHUNK_BYTES]> =
                        chunks.iter().collect();
                    let total = chunks.len();
                    if total > 0
                        && unique.len().saturating_mul(100)
                            <= total.saturating_mul(INTERLEAVED_MAX_UNIQUE_PERCENT)
                    {
                        return true;
                    }
                }
            }
        }
        // Align `window` to a UTF-8 char boundary before shingle analysis.
        let mut utf8_start = start;
        while utf8_start < bytes.len() && !text.is_char_boundary(utf8_start) {
            utf8_start += 1;
        }
        if let Some(utf8_window) = bytes.get(utf8_start..) {
            if has_shingle_collapse(utf8_window) {
                return true;
            }
        }
    }
    false
}
