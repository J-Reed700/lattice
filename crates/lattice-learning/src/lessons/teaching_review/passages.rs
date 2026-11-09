//! Split a section body along its own structure: paragraphs, list items,
//! fenced code and tables. Separators belong to the passage before them.

/// The indent of a Markdown list item line, or `None` for any other line.
fn list_indent(line: &str) -> Option<usize> {
    let text = line.trim_start_matches([' ', '\t']);
    let indent = line.chars().take_while(|c| matches!(c, ' ' | '\t')).count();
    let mut chars = text.chars();
    let first = chars.next()?;
    let rest = if matches!(first, '-' | '*' | '+') {
        chars.as_str()
    } else if first.is_ascii_digit() {
        let after = text.trim_start_matches(|c: char| c.is_ascii_digit());
        let mut marker = after.chars();
        if !matches!(marker.next(), Some('.' | ')')) {
            return None;
        }
        marker.as_str()
    } else {
        return None;
    };
    (rest.is_empty() || rest.starts_with([' ', '\t', '\r', '\n'])).then_some(indent)
}

pub(super) fn split(body: &str) -> Vec<&str> {
    let mut passages = Vec::new();
    let (mut start, mut position) = (0, 0);
    let mut fence: Option<char> = None;
    let (mut has_text, mut after_blank, mut in_table) = (false, false, false);
    let mut item_indent: Option<usize> = None;
    for line in body.split_inclusive('\n') {
        let text = line.trim();
        if fence.is_none() {
            if text.is_empty() {
                after_blank = true;
            } else {
                let table = text.starts_with('|');
                let item = list_indent(line);
                let breaks = has_text
                    && (after_blank
                        || table != in_table
                        || item.is_some_and(|indent| item_indent.is_none_or(|k| indent <= k)));
                if breaks {
                    passages.push(body.get(start..position).unwrap_or_default());
                    start = position;
                    item_indent = None;
                }
                if item.is_some() && item_indent.is_none() {
                    item_indent = item;
                }
                has_text = true;
                after_blank = false;
                in_table = table;
            }
        }
        let marker = if text.starts_with("```") {
            Some('`')
        } else if text.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        match (fence, marker) {
            (None, Some(m)) => fence = Some(m),
            (Some(open), Some(m)) if open == m => fence = None,
            _ => {}
        }
        position += line.len();
    }
    if start < body.len() {
        passages.push(body.get(start..).unwrap_or_default());
    }
    passages
}
