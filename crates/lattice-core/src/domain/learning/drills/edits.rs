//! Planting edits: a defect enters the clean code as an exact text edit, and
//! the key's line spans are read off the edited files, never stated by a
//! model.

use super::drill::{DrillFile, Edit, LineSpan};

/// Why a list of edits cannot be applied. Positions are indexes into the list.
/// An edit's own faults are looked for in the order listed here, and the
/// edits are compared with each other only once every one has its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditFault {
    /// No file has the edit's path.
    UnknownFile { edit: usize },
    /// The edit's `before` is empty.
    Empty { edit: usize },
    /// The edit's `before` is not in its file.
    NotFound { edit: usize },
    /// The edit's `before` is in its file `count` times, occurrences that
    /// overlap each other included.
    Ambiguous { edit: usize, count: usize },
    /// Two edits, `first` before `second` in the list, cannot both stand: their
    /// `before` ranges in one file overlap or touch, or their results share a
    /// line, so that one line would be two issues.
    Overlaps { first: usize, second: usize },
}

/// Applies `edits` to `files` all at once, each against the original text, and
/// returns the edited files and, per edit in input order, the lines its `after`
/// text occupies in the result. Every file comes back, in order; one without an
/// edit is unchanged.
///
/// A line is a run of text ending in `\n` or at the end of the file, and an
/// `after` text occupies every line it has a character on; a final `\n` ends
/// its line, it does not start the next. A deletion (empty `after`) spans the
/// line where the removed text began, or the last line when it began after the
/// final `\n`.
///
/// Matching is exact, byte for byte: the caller normalises line endings to `\n`
/// first.
pub fn apply(
    files: &[DrillFile],
    edits: &[Edit],
) -> Result<(Vec<DrillFile>, Vec<LineSpan>), EditFault> {
    let placed = locate(files, edits)?;
    if let Some((first, second)) = colliding(&placed) {
        return Err(EditFault::Overlaps { first, second });
    }
    let mut edited = Vec::with_capacity(files.len());
    let mut landed = Vec::with_capacity(edits.len());
    for (index, file) in files.iter().enumerate() {
        let (rebuilt, spans) = rebuild(file, index, edits, &placed);
        edited.push(rebuilt);
        landed.extend(spans);
    }
    landed.sort_by_key(|&(edit, _)| edit);
    let spans: Vec<LineSpan> = landed.into_iter().map(|(_, span)| span).collect();
    if let Some((first, second)) = sharing_a_line(&spans) {
        return Err(EditFault::Overlaps { first, second });
    }
    Ok((edited, spans))
}

/// Whether two spans are in one file and have a line in common.
pub fn overlaps(a: &LineSpan, b: &LineSpan) -> bool {
    a.path == b.path && a.start <= b.end && b.start <= a.end
}

/// Where an edit's `before` text is in its file's original text.
struct Placed {
    edit: usize,
    file: usize,
    /// The byte range of `before`.
    start: usize,
    end: usize,
}

/// Finds the one place of each edit's `before`.
fn locate(files: &[DrillFile], edits: &[Edit]) -> Result<Vec<Placed>, EditFault> {
    let mut placed = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        let Some((file, target)) = files
            .iter()
            .enumerate()
            .find(|(_, candidate)| candidate.path == edit.path)
        else {
            return Err(EditFault::UnknownFile { edit: index });
        };
        if edit.before.is_empty() {
            return Err(EditFault::Empty { edit: index });
        }
        let found = occurrences(&target.text, &edit.before);
        match found.as_slice() {
            [] => return Err(EditFault::NotFound { edit: index }),
            [start] => placed.push(Placed {
                edit: index,
                file,
                start: *start,
                end: *start + edit.before.len(),
            }),
            _ => {
                return Err(EditFault::Ambiguous {
                    edit: index,
                    count: found.len(),
                })
            }
        }
    }
    Ok(placed)
}

/// Where `needle`, which is not empty, begins in `text`: every byte offset,
/// occurrences that overlap each other included.
fn occurrences(text: &str, needle: &str) -> Vec<usize> {
    let step = needle.chars().next().map_or(1, char::len_utf8);
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = text.get(from..).and_then(|rest| rest.find(needle)) {
        found.push(from + at);
        from += at + step;
    }
    found
}

/// The first pair of edits, in list order, whose `before` ranges in one file
/// overlap or touch.
fn colliding(placed: &[Placed]) -> Option<(usize, usize)> {
    for (position, a) in placed.iter().enumerate() {
        for b in placed.iter().skip(position + 1) {
            if a.file == b.file && a.start <= b.end && b.start <= a.end {
                return Some((a.edit, b.edit));
            }
        }
    }
    None
}

/// The first pair of spans, in list order, with a line in common.
fn sharing_a_line(spans: &[LineSpan]) -> Option<(usize, usize)> {
    for (first, a) in spans.iter().enumerate() {
        for (second, b) in spans.iter().enumerate().skip(first + 1) {
            if overlaps(a, b) {
                return Some((first, second));
            }
        }
    }
    None
}

/// `file` with its edits applied, and the span of each: the edit's place in the
/// list and the lines its `after` text occupies.
fn rebuild(
    file: &DrillFile,
    index: usize,
    edits: &[Edit],
    placed: &[Placed],
) -> (DrillFile, Vec<(usize, LineSpan)>) {
    let mut mine: Vec<&Placed> = placed.iter().filter(|p| p.file == index).collect();
    mine.sort_by_key(|p| p.start);
    let mut text = String::with_capacity(file.text.len());
    let mut copied = 0;
    let mut newlines = 0;
    let mut landed = Vec::with_capacity(mine.len());
    for p in mine {
        let kept = file.text.get(copied..p.start).unwrap_or_default();
        text.push_str(kept);
        newlines += count_newlines(kept);
        let after = edits.get(p.edit).map_or("", |edit| edit.after.as_str());
        let inside = count_newlines(after);
        let start = newlines + 1;
        let end = if after.is_empty() {
            start
        } else {
            start + inside - usize::from(after.ends_with('\n'))
        };
        text.push_str(after);
        newlines += inside;
        copied = p.end;
        landed.push((p.edit, start, end));
    }
    text.push_str(file.text.get(copied..).unwrap_or_default());
    let last = line_count(&text);
    let spans = landed
        .into_iter()
        .map(|(edit, start, end)| {
            let span = LineSpan {
                path: file.path.clone(),
                start: as_line(start.min(last)),
                end: as_line(end.min(last)),
            };
            (edit, span)
        })
        .collect();
    let edited = DrillFile {
        path: file.path.clone(),
        text,
    };
    (edited, spans)
}

fn count_newlines(text: &str) -> usize {
    text.bytes().filter(|&byte| byte == b'\n').count()
}

/// How many lines `text` has: at least one, and a final `\n` ends the last
/// line rather than starting another.
fn line_count(text: &str) -> usize {
    let unfinished = !text.is_empty() && !text.ends_with('\n');
    (count_newlines(text) + usize::from(unfinished)).max(1)
}

fn as_line(number: usize) -> u32 {
    u32::try_from(number).unwrap_or(u32::MAX)
}
