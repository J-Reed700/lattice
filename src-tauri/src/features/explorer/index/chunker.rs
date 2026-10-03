//! Splits a text file into the passages the folder index embeds.
//!
//! Line-based and structure-aware without a parser: a chunk grows to about
//! 1,500 characters (40 to 80 lines of ordinary code), and where it is cut
//! prefers the line before a top-level item (`fn`, `class`, `export`, a
//! Markdown heading…) or the line after a blank one, so a passage tends to
//! hold one whole definition. No chunk passes 120 lines.
//!
//! Line numbers are 1-based and inclusive, the same grammar the answer uses
//! to point at lines, so a passage's heading is already a line reference.

/// A chunk past this many lines is cut wherever a boundary first allows.
pub const SOFT_MAX_LINES: usize = 80;
/// No chunk is longer than this, boundary or not.
pub const MAX_CHUNK_LINES: usize = 120;
/// Characters a chunk grows to before it is cut.
pub const MAX_CHUNK_CHARS: usize = 1_500;
/// A longer line is clipped. A minified bundle can put a whole file on one
/// line, and one line must not become one huge passage.
pub const MAX_LINE_CHARS: usize = 500;
/// Files over this are not indexed, the bound `search_files` reads to.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Average line length past which a file is taken as generated or minified.
const MAX_AVERAGE_LINE_CHARS: usize = 300;

/// One passage of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// 1-based, inclusive.
    pub start_line: u32,
    /// 1-based, inclusive.
    pub end_line: u32,
    /// The lines themselves, joined with `\n`, overlong ones clipped.
    pub body: String,
}

impl Chunk {
    /// What is embedded and full-text indexed: the line reference, then the
    /// lines. The path is a strong signal in code search ("the retry module"),
    /// and the heading keeps it with the passage wherever the passage goes.
    pub fn indexed_text(&self, path: &str) -> String {
        indexed_text(path, self.start_line, self.end_line, &self.body)
    }
}

pub fn indexed_text(path: &str, start_line: u32, end_line: u32, body: &str) -> String {
    format!("{path}:{start_line}-{end_line}\n{body}")
}

/// The lines of a stored passage, without the heading [`indexed_text`] put on.
pub fn body_of(indexed: &str) -> &str {
    indexed.split_once('\n').map_or("", |(_, body)| body)
}

/// Lockfiles and minified bundles: text, but noise to a search by meaning.
pub fn skipped_by_name(relative: &str) -> bool {
    let name = relative.rsplit('/').next().unwrap_or(relative);
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".lock")
        || matches!(
            lower.as_str(),
            "package-lock.json" | "npm-shrinkwrap.json" | "pnpm-lock.yaml" | "go.sum"
        )
        || lower.contains(".min.")
}

/// Generated or minified text: lines that average past what people write.
pub fn looks_minified(text: &str) -> bool {
    let lines = text.lines().count().max(1);
    text.len() / lines > MAX_AVERAGE_LINE_CHARS
}

fn clip(line: &str) -> String {
    match line.char_indices().nth(MAX_LINE_CHARS) {
        Some((cut, _)) => format!("{}…", line.get(..cut).unwrap_or(line)),
        None => line.to_string(),
    }
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// A line at indentation 0 that opens a top-level item, or leads into one
/// (an attribute, a doc comment, a decorator).
fn opens_item(line: &str) -> bool {
    const OPENERS: [&str; 26] = [
        "fn ",
        "pub ",
        "pub(",
        "impl ",
        "impl<",
        "struct ",
        "enum ",
        "trait ",
        "mod ",
        "unsafe ",
        "async ",
        "class ",
        "def ",
        "function ",
        "export ",
        "const ",
        "interface ",
        "type ",
        "func ",
        "module ",
        "#[",
        "///",
        "//!",
        "/**",
        "@",
        "#!",
    ];
    if line.starts_with(char::is_whitespace) {
        return false;
    }
    if OPENERS.iter().any(|opener| line.starts_with(opener)) {
        return true;
    }
    // A Markdown heading: one to six `#` and a space.
    let hashes = line.chars().take_while(|&c| c == '#').count();
    (1..=6).contains(&hashes) && line.chars().nth(hashes) == Some(' ')
}

/// How good a place the line boundary before `lines[at]` is to cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Cut {
    None,
    /// After a blank line.
    Paragraph,
    /// Before the first line of a top-level item, its attributes and doc
    /// comment included.
    Item,
}

fn cut_before(lines: &[String], at: usize) -> Cut {
    let (Some(previous), Some(line)) = (
        at.checked_sub(1).and_then(|index| lines.get(index)),
        lines.get(at),
    ) else {
        return Cut::None;
    };
    if is_blank(line) {
        return Cut::None;
    }
    if opens_item(line) && !opens_item(previous) {
        Cut::Item
    } else if is_blank(previous) {
        Cut::Paragraph
    } else {
        Cut::None
    }
}

/// The passages of `text`, in order. Blank-only stretches are left out.
pub fn chunk_text(text: &str) -> Vec<Chunk> {
    let lines: Vec<String> = text.lines().map(clip).collect();
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        // Grow as far as the caps allow, stopping early at a boundary only
        // once the chunk is past the soft line cap.
        let mut end = start;
        let mut chars = 0usize;
        while let Some(line) = lines.get(end) {
            let taken = end - start;
            if taken >= MAX_CHUNK_LINES
                || (taken > 0 && chars + line.len() + 1 > MAX_CHUNK_CHARS)
                || (taken >= SOFT_MAX_LINES && cut_before(&lines, end) != Cut::None)
            {
                break;
            }
            chars += line.len() + 1;
            end += 1;
        }
        // Cut short of the cap at the best boundary in the back half, so a
        // definition that starts near the end moves whole into the next chunk.
        if end < lines.len() {
            let earliest = start + ((end - start) / 2).max(1);
            // `max_by_key` keeps the last of equals: the latest cut of the
            // best kind, so the chunk stays as long as it can.
            let best = (earliest..=end)
                .map(|at| (cut_before(&lines, at), at))
                .max_by_key(|(cut, _)| *cut)
                .filter(|(cut, _)| *cut != Cut::None);
            if let Some((_, at)) = best {
                end = at;
            }
        }
        let body = lines.get(start..end).unwrap_or_default().join("\n");
        if !body.trim().is_empty() {
            chunks.push(Chunk {
                start_line: start as u32 + 1,
                end_line: end as u32,
                body,
            });
        }
        start = end;
    }
    chunks
}
