//! Pictures and fenced blocks in assistant markdown, and the pictures a
//! reader attached to their own message.
//!
//! An answer shows a stored picture as `![caption](img-N)`, where `img-N` is
//! the ref `find_image` returned. Persistence keeps the refs the answer
//! actually wrote, memory and handoff read a caption in the picture's place,
//! and the grounding check reads the answer without code fences (diagrams)
//! or pictures. Text inside a fenced block is code, never a picture.
//!
//! A user message's attached pictures live in its `metadata.images`; every
//! later reader (history, memory, handoff) reads one caption line per
//! picture, never the pixels, which ride only in the turn they were sent in.
//! Design: docs/design/2026-10-17-chat-images-and-diagrams.md §7.2, §7.11;
//! docs/design/2026-10-17-chat-vision.md §11.

/// The `img-N` targets of `![…](img-N)` outside code fences, in the order
/// the answer first writes them, each once.
pub fn image_refs(markdown: &str) -> Vec<String> {
    let mut refs: Vec<String> = Vec::new();
    for_each_line(markdown, |line, in_fence| {
        if in_fence {
            return;
        }
        for image in find_images(line) {
            if is_image_ref(image.target) && !refs.iter().any(|r| r == image.target) {
                refs.push(image.target.to_string());
            }
        }
    });
    refs
}

/// The markdown with each picture outside a code fence replaced by
/// `[Picture: alt]`, so a reader that cannot see pictures reads what the
/// answer showed. Fenced blocks are copied unchanged.
pub fn images_to_captions(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    for_each_line(markdown, |line, in_fence| {
        if in_fence {
            out.push_str(line);
        } else {
            replace_images(line, &mut out, |alt| format!("[Picture: {}]", alt.trim()));
        }
    });
    out
}

/// The markdown with every fenced block and every picture removed, for the
/// grounding check: a diagram's labels and a picture's caption are not
/// claims about the sources. A removed fence leaves one blank line so the
/// sentences on either side stay apart; an unclosed fence runs to the end.
pub fn strip_fences_and_images(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut fence_open = false;
    for_each_line(markdown, |line, in_fence| {
        if in_fence {
            if !fence_open {
                out.push('\n');
                fence_open = true;
            }
            return;
        }
        fence_open = false;
        replace_images(line, &mut out, |_| String::new());
    });
    out
}

/// One `[Picture attached: name]` line per picture, for a reader that sees
/// the message's text but not its pictures. Empty when there are none.
pub fn picture_captions(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("[Picture attached: {}]", one_line(name)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The names of the pictures a message's `metadata` JSON lists under
/// `images`: each one's `originalName` (the attached file), else its
/// `caption`. Empty for no metadata, unreadable metadata or no pictures.
pub fn attached_picture_names(metadata: Option<&str>) -> Vec<String> {
    let Some(value) = metadata.and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
    else {
        return Vec::new();
    };
    let Some(images) = value.get("images").and_then(|images| images.as_array()) else {
        return Vec::new();
    };
    images
        .iter()
        .filter_map(|image| {
            let named = |key: &str| {
                image
                    .get(key)
                    .and_then(|name| name.as_str())
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
            };
            named("originalName")
                .or_else(|| named("caption"))
                .map(str::to_string)
        })
        .collect()
}

/// A user message as a reader that cannot see its pictures reads it: the
/// text, then one caption line per attached picture.
pub fn with_picture_captions(text: &str, pictures: &[String]) -> String {
    if pictures.is_empty() {
        return text.to_string();
    }
    let captions = picture_captions(pictures);
    if text.trim().is_empty() {
        captions
    } else {
        format!("{}\n{captions}", text.trim_end())
    }
}

/// `text` with line breaks and brackets that would end a caption line
/// replaced.
fn one_line(text: &str) -> String {
    text.trim()
        .chars()
        .map(|c| match c {
            '\n' | '\r' => ' ',
            '[' => '(',
            ']' => ')',
            c => c,
        })
        .collect()
}

/// `img-` followed by one or more ASCII digits, nothing else.
pub fn is_image_ref(target: &str) -> bool {
    target
        .strip_prefix("img-")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// An open code fence: its marker character and run length.
struct Fence {
    marker: char,
    len: usize,
}

/// Call `f(line, in_fence)` for every line (with its newline). Fence lines
/// themselves — the opening and closing markers — count as inside.
fn for_each_line(markdown: &str, mut f: impl FnMut(&str, bool)) {
    let mut open: Option<Fence> = None;
    for line in markdown.split_inclusive('\n') {
        match &open {
            Some(fence) => {
                if closes(fence, line) {
                    open = None;
                }
                f(line, true);
            }
            None => match opens(line) {
                Some(fence) => {
                    open = Some(fence);
                    f(line, true);
                }
                None => f(line, false),
            },
        }
    }
}

/// The line with its indentation, blockquote markers and one list marker
/// removed: a fence inside a list item or a blockquote is still a fence.
fn block_content(line: &str) -> &str {
    let mut rest = line.trim_start();
    while let Some(after) = rest.strip_prefix('>') {
        rest = after.trim_start();
    }
    if let Some(after) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        return after.trim_start();
    }
    let after = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    if after.len() < rest.len() {
        if let Some(after) = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))
        {
            return after.trim_start();
        }
    }
    rest
}

fn opens(line: &str) -> Option<Fence> {
    let content = block_content(line);
    let marker = content.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let info = content.trim_start_matches(marker);
    let len = content.len() - info.len();
    if len < 3 {
        return None;
    }
    // A backtick fence's info string cannot itself hold a backtick.
    if marker == '`' && info.contains('`') {
        return None;
    }
    Some(Fence { marker, len })
}

fn closes(fence: &Fence, line: &str) -> bool {
    let content = block_content(line);
    let rest = content.trim_start_matches(fence.marker);
    content.len() - rest.len() >= fence.len && rest.trim().is_empty()
}

/// One `![alt](target)` in a line, by byte range.
struct ImageSpan<'a> {
    start: usize,
    end: usize,
    alt: &'a str,
    target: &'a str,
}

/// Every inline image in `line`. Brackets and parentheses nest; an optional
/// `"title"` after the target and `<…>` around it are dropped from `target`.
fn find_images(line: &str) -> Vec<ImageSpan<'_>> {
    let bytes = line.as_bytes();
    let mut images = Vec::new();
    let mut i = 0;
    while let Some(offset) = line.get(i..).and_then(|rest| rest.find("![")) {
        let start = i + offset;
        let alt_start = start + 2;
        let Some(alt_end) = matching(bytes, alt_start, b'[', b']') else {
            break;
        };
        if bytes.get(alt_end + 1) != Some(&b'(') {
            i = alt_start;
            continue;
        }
        let target_start = alt_end + 2;
        let Some(target_end) = matching(bytes, target_start, b'(', b')') else {
            i = alt_start;
            continue;
        };
        let raw = line.get(target_start..target_end).unwrap_or("").trim();
        let target = raw.split_whitespace().next().unwrap_or("");
        let target = target
            .strip_prefix('<')
            .and_then(|t| t.strip_suffix('>'))
            .unwrap_or(target);
        images.push(ImageSpan {
            start,
            end: target_end + 1,
            alt: line.get(alt_start..alt_end).unwrap_or(""),
            target,
        });
        i = target_end + 1;
    }
    images
}

/// Index of the `close` byte that balances an already-consumed `open`,
/// scanning from `from`.
fn matching(bytes: &[u8], from: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, b) in bytes.get(from..)?.iter().enumerate() {
        if *b == open {
            depth += 1;
        } else if *b == close {
            if depth == 0 {
                return Some(from + offset);
            }
            depth -= 1;
        }
    }
    None
}

fn replace_images(line: &str, out: &mut String, replacement: impl Fn(&str) -> String) {
    let mut last = 0;
    for image in find_images(line) {
        out.push_str(line.get(last..image.start).unwrap_or(""));
        out.push_str(&replacement(image.alt));
        last = image.end;
    }
    out.push_str(line.get(last..).unwrap_or(""));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_captions_writes_one_line_per_picture() {
        assert_eq!(picture_captions(&[]), "");
        assert_eq!(
            picture_captions(&["chart.png".to_string(), "a [draft]\nscan".to_string()]),
            "[Picture attached: chart.png]\n[Picture attached: a (draft) scan]"
        );
    }

    #[test]
    fn attached_picture_names_prefers_the_file_name_over_the_caption() {
        let metadata = r#"{"attachments":["notes.pdf"],"images":[
            {"id":"a","caption":"chart.png","originalName":"Q3 chart.png"},
            {"id":"b","caption":"A heron"},
            {"id":"c","caption":"  "}]}"#;
        assert_eq!(
            attached_picture_names(Some(metadata)),
            vec!["Q3 chart.png".to_string(), "A heron".to_string()]
        );
        assert!(attached_picture_names(Some("{}")).is_empty());
        assert!(attached_picture_names(Some("not json")).is_empty());
        assert!(attached_picture_names(None).is_empty());
    }

    #[test]
    fn with_picture_captions_appends_after_the_text() {
        let pictures = vec!["plot.png".to_string()];
        assert_eq!(
            with_picture_captions("What does this show?\n", &pictures),
            "What does this show?\n[Picture attached: plot.png]"
        );
        assert_eq!(
            with_picture_captions("", &pictures),
            "[Picture attached: plot.png]"
        );
        assert_eq!(with_picture_captions("Hi", &[]), "Hi");
    }

    #[test]
    fn image_refs_lists_each_ref_once_in_order() {
        let md = "A heron ![Grey heron](img-2) and ![Egret](img-1).\n\
                  Again ![Grey heron](img-2) and a link ![x](https://example.org/a.png).";
        assert_eq!(image_refs(md), vec!["img-2", "img-1"]);
    }

    #[test]
    fn image_refs_ignores_pictures_inside_fences() {
        let md = "```markdown\n![shown as code](img-1)\n```\n![real](img-3)\n";
        assert_eq!(image_refs(md), vec!["img-3"]);
    }

    #[test]
    fn image_refs_accepts_titles_angle_brackets_and_nested_alt() {
        let md = "![a [b] c](img-1 \"title\") ![d](<img-2>) ![e](img-x) ![f](img-)";
        assert_eq!(image_refs(md), vec!["img-1", "img-2"]);
    }

    #[test]
    fn images_to_captions_replaces_pictures_and_keeps_fences() {
        let md = "Look: ![A grey heron](img-1)\n```\n![kept](img-2)\n```\n";
        assert_eq!(
            images_to_captions(md),
            "Look: [Picture: A grey heron]\n```\n![kept](img-2)\n```\n"
        );
    }

    #[test]
    fn strip_removes_fences_and_images() {
        let md = "Herons wade [1].\n```mermaid\nflowchart LR\n  A[1] --> B\n```\n\
                  ![Heron](img-1)\nThey eat fish [2].";
        let stripped = strip_fences_and_images(md);
        assert!(!stripped.contains("flowchart"));
        assert!(!stripped.contains("A[1]"));
        assert!(!stripped.contains("img-1"));
        assert!(stripped.contains("Herons wade [1]."));
        assert!(stripped.contains("They eat fish [2]."));
    }

    #[test]
    fn strip_handles_a_fence_inside_a_list() {
        let md = "- Step one [1]\n  ```mermaid\n  graph TD; A[2]-->B\n  ```\n- Step two [3]\n";
        let stripped = strip_fences_and_images(md);
        assert!(!stripped.contains("graph TD"));
        assert!(stripped.contains("Step one [1]"));
        assert!(stripped.contains("Step two [3]"));
    }

    #[test]
    fn strip_runs_an_unclosed_fence_to_the_end() {
        let md = "Intro [1].\n```mermaid\nflowchart LR\n  A[2] --> B\n";
        let stripped = strip_fences_and_images(md);
        assert_eq!(stripped.trim(), "Intro [1].");
    }

    #[test]
    fn strip_removes_an_image_inside_a_blockquote() {
        let md = "> ![Quoted heron [4]](img-1)\n> The heron stands still [5].\n";
        let stripped = strip_fences_and_images(md);
        assert!(!stripped.contains("[4]"));
        assert!(stripped.contains("The heron stands still [5]."));
    }

    #[test]
    fn a_fence_inside_a_blockquote_is_a_fence() {
        let md = "> ```\n> ![code](img-1)\n> ```\n";
        assert!(image_refs(md).is_empty());
    }

    #[test]
    fn a_shorter_or_different_marker_does_not_close_a_fence() {
        let md = "````\n```\n![x](img-1)\n~~~~\n````\n![y](img-2)\n";
        assert_eq!(image_refs(md), vec!["img-2"]);
    }
}
