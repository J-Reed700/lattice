//! Non-blocking Markdown structure notices (ordered-list gaps and unclosed code fences).

use super::types::FieldNotice;
use crate::shared::text::segmentation::{code_fences, item_marker_end};

struct OrderedItem {
    indent: usize,
    number: u64,
    marker_end: usize,
}

struct ActiveList {
    indent: usize,
    marker_end: usize,
    items: Vec<(u64, u32)>,
}

fn line_number_at(text: &str, byte_offset: usize) -> u32 {
    let newlines = text
        .get(..byte_offset)
        .unwrap_or_default()
        .bytes()
        .filter(|&b| b == b'\n')
        .count();
    u32::try_from(newlines + 1).unwrap_or(u32::MAX)
}

fn parse_ordered_item(line: &str) -> Option<OrderedItem> {
    let marker_end = item_marker_end(line)?;
    let rest = line.trim_start_matches([' ', '\t']);
    let indent = line.len().saturating_sub(rest.len());
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let after_digits = rest.get(digits..)?;
    if !after_digits.starts_with('.') && !after_digits.starts_with(')') {
        return None;
    }
    let number = rest.get(..digits)?.parse::<u64>().ok()?;
    Some(OrderedItem {
        indent,
        number,
        marker_end,
    })
}

fn flush_list(list: ActiveList, path: &str, notices: &mut Vec<FieldNotice>) {
    let Some(&(first_num, _)) = list.items.first() else {
        return;
    };
    if list.items.iter().all(|&(num, _)| num == first_num) {
        return;
    }
    for pair in list.items.windows(2) {
        let (Some(&(prev, _)), Some(&(curr, curr_line))) = (pair.first(), pair.get(1)) else {
            continue;
        };
        if curr != prev.saturating_add(1) {
            notices.push(FieldNotice {
                path: path.to_owned(),
                code: "list_numbering".to_owned(),
                message: format!(
                    "The numbering jumps from {prev} to {curr} at line {curr_line}. A line may have lost its number."
                ),
                line: Some(curr_line),
            });
        }
    }
}

pub(crate) fn markdown_notices(path: &str, text: &str) -> Vec<FieldNotice> {
    let fences = code_fences(text);
    let mut notices = Vec::new();
    for fence in &fences {
        if fence.closing.is_none() {
            let line = line_number_at(text, fence.opening.start);
            notices.push(FieldNotice {
                path: path.to_owned(),
                code: "open_code_fence".to_owned(),
                message: format!("A code block that opens at line {line} is never closed."),
                line: Some(line),
            });
        }
    }

    let mut active_lists: Vec<ActiveList> = Vec::new();
    let mut after_blank = false;
    let mut byte_offset = 0usize;

    for (line_idx, raw_line) in text.split_inclusive('\n').enumerate() {
        let in_fence = fences
            .iter()
            .any(|fence| fence.whole().contains(&byte_offset));
        byte_offset = byte_offset.saturating_add(raw_line.len());
        let line_num = u32::try_from(line_idx + 1).unwrap_or(u32::MAX);
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let rest = line.trim_start_matches([' ', '\t']);
        let indent = line.len().saturating_sub(rest.len());

        if in_fence {
            if after_blank {
                while let Some(top) = active_lists.last() {
                    if indent >= top.marker_end {
                        break;
                    }
                    if let Some(popped) = active_lists.pop() {
                        flush_list(popped, path, &mut notices);
                    }
                }
            }
            continue;
        }

        if line.trim().is_empty() {
            after_blank = true;
            continue;
        }

        if let Some(item) = parse_ordered_item(line) {
            while let Some(top) = active_lists.last() {
                let compatible_indent = item.indent == top.indent || item.indent >= top.marker_end;
                if compatible_indent
                    && !(after_blank && item.indent != top.indent && item.indent < top.marker_end)
                {
                    break;
                }
                if let Some(popped) = active_lists.pop() {
                    flush_list(popped, path, &mut notices);
                }
            }
            if let Some(top) = active_lists
                .last_mut()
                .filter(|top| top.indent == item.indent)
            {
                top.marker_end = item.marker_end;
                top.items.push((item.number, line_num));
            } else {
                active_lists.push(ActiveList {
                    indent: item.indent,
                    marker_end: item.marker_end,
                    items: vec![(item.number, line_num)],
                });
            }
            after_blank = false;
            continue;
        }

        if item_marker_end(line).is_some() {
            while let Some(top) = active_lists.last() {
                if indent >= top.marker_end {
                    break;
                }
                if let Some(popped) = active_lists.pop() {
                    flush_list(popped, path, &mut notices);
                }
            }
            after_blank = false;
            continue;
        }

        if after_blank {
            while let Some(top) = active_lists.last() {
                if indent >= top.marker_end {
                    break;
                }
                if let Some(popped) = active_lists.pop() {
                    flush_list(popped, path, &mut notices);
                }
            }
        }
        after_blank = false;
    }

    while let Some(popped) = active_lists.pop() {
        flush_list(popped, path, &mut notices);
    }

    notices.sort_by_key(|notice| notice.line.unwrap_or(0));
    notices
}
