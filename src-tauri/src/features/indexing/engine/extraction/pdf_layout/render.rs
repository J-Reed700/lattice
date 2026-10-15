//! Grouping runs into lines and rendering them as markdown headings, tables and text.

use std::collections::BTreeMap;

use super::{
    Line, PageLayout, Run, DEEPEST_HEADING_LEVEL, HEADING_RATIO, MAX_HEADING_CHARS,
    MIN_TABLE_COLUMNS, MIN_TABLE_ROWS,
};

/// Group runs into visual lines, keeping the order the producer wrote them in.
///
/// Reading order is the producer's, not the geometry's. A two-column page is
/// normally written one column at a time, so sorting by descending y would
/// interleave the columns and destroy sentences; a page written row by row
/// across both columns still groups correctly here, because those runs are
/// adjacent in the stream. The cost is that a line whose runs are written out
/// of order (all the bold ones first, say) splits into several lines.
pub(super) fn group_lines(runs: Vec<Run>) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for run in runs {
        let tolerance = (run.size * 0.35).max(1.0);
        match lines.last_mut() {
            Some(line) if (line.y - run.y).abs() <= tolerance => line.runs.push(run),
            _ => lines.push(Line {
                y: run.y,
                runs: vec![run],
            }),
        }
    }
    for line in &mut lines {
        line.runs.sort_by(|left, right| left.x.total_cmp(&right.x));
    }
    lines
}

/// Round a size into half-point buckets so that 11.999 and 12.0 are one tier.
fn bucket(size: f32) -> u32 {
    (size.max(0.0) * 2.0).round() as u32
}

/// The document's body size: the size that sets the most characters.
///
/// A mode rather than a mean, because a mean is dragged upward by title pages
/// and downward by footnotes, and the body size is what every other decision
/// here is relative to.
pub(super) fn body_size(pages: &[PageLayout]) -> f32 {
    let mut weights: BTreeMap<u32, usize> = BTreeMap::new();
    for line in pages.iter().flat_map(|page| page.lines.iter()) {
        *weights.entry(bucket(line.size())).or_default() += line.char_count();
    }
    weights
        .into_iter()
        // Ties go to the smaller size: body text outnumbers headings, so a tie
        // means the larger tier is decoration.
        .max_by_key(|(size, weight)| (*weight, std::cmp::Reverse(*size)))
        .map(|(size, _)| size as f32 / 2.0)
        .unwrap_or(0.0)
}

/// Size buckets that count as headings, largest first.
pub(super) fn heading_tiers(pages: &[PageLayout], body: f32) -> Vec<u32> {
    if body <= 0.0 {
        return Vec::new();
    }
    let mut tiers: Vec<u32> = pages
        .iter()
        .flat_map(|page| page.lines.iter())
        .filter(|line| line.size() >= body * HEADING_RATIO && is_heading_shaped(line))
        .map(|line| bucket(line.size()))
        .collect();
    tiers.sort_unstable();
    tiers.dedup();
    tiers.reverse();
    tiers
}

/// A heading must be short enough that the chunker will recognise it.
fn is_heading_shaped(line: &Line) -> bool {
    let text = line.text();
    !text.is_empty() && text.chars().count() <= MAX_HEADING_CHARS
}

/// The markdown level for a line, or `None` if it is body text.
///
/// Known failure modes: a document typeset entirely in one size has no
/// headings (correctly — there is nothing to find); a document whose body is
/// bold throughout would promote every line, which the `size >= body`
/// requirement plus the ratio test for the non-bold case keeps rare.
fn heading_level(line: &Line, body: f32, tiers: &[u32]) -> Option<usize> {
    if body <= 0.0 || !is_heading_shaped(line) {
        return None;
    }
    let size = line.size();
    // Bold body-size lines sit below every size tier.
    let deepest = (2 + tiers.len()).min(DEEPEST_HEADING_LEVEL);
    if size >= body * HEADING_RATIO && !tiers.is_empty() {
        let tier = tiers
            .iter()
            .position(|tier| *tier == bucket(size))
            .unwrap_or(tiers.len());
        return Some((2 + tier).min(DEEPEST_HEADING_LEVEL));
    }
    // A bold run at body size is the other way documents mark a subheading.
    if line.bold() && size >= body {
        return Some(deepest);
    }
    None
}

/// Longest cell text a table row may carry. A cell is a datum; a sentence
/// that happens to start at a shared x is prose in a column layout.
const MAX_TABLE_CELL_CHARS: usize = 40;

/// Columns shared by a candidate table block, if there are enough of them.
///
/// Every run of every row must sit on one of the columns. That strictness is
/// deliberate: a two-column page of prose has its lines starting at exactly
/// two x-positions with words scattered between them, and admitting it as a
/// table would shred paragraphs into cells. The cost is that a table whose
/// cells are kerned into several positioned runs is not recognised.
///
/// Known failure modes: right-aligned numeric columns anchor at a different x
/// per row and will not cluster; a cell that wraps becomes its own row; a
/// three-column table with a merged cell is rejected.
fn table_columns(lines: &[Line], tolerance: f32) -> Option<Vec<f32>> {
    let first = lines.first()?;
    if first.runs.len() < MIN_TABLE_COLUMNS {
        return None;
    }
    let mut columns: Vec<f32> = Vec::new();
    for run in &first.runs {
        if !columns.iter().any(|x| (x - run.x).abs() <= tolerance) {
            columns.push(run.x);
        }
    }
    if columns.len() < MIN_TABLE_COLUMNS {
        return None;
    }
    for line in lines {
        if line.runs.len() != columns.len() {
            return None;
        }
        let aligned = line.runs.iter().all(|run| {
            run.text.chars().count() <= MAX_TABLE_CELL_CHARS
                && columns.iter().any(|x| (x - run.x).abs() <= tolerance)
        });
        if !aligned {
            return None;
        }
    }
    Some(columns)
}

/// The longest run of consecutive lines from `start` that share columns.
fn table_block(
    lines: &[Line],
    headings: &[bool],
    start: usize,
    tolerance: f32,
) -> Option<(usize, Vec<f32>)> {
    let mut end = start;
    let mut columns = None;
    while let Some(line) = lines.get(end) {
        if line.runs.len() < MIN_TABLE_COLUMNS || headings.get(end) == Some(&true) {
            break;
        }
        let candidate = lines
            .get(start..=end)
            .and_then(|block| table_columns(block, tolerance));
        match candidate {
            Some(found) => {
                columns = Some(found);
                end += 1;
            }
            None => break,
        }
    }
    let columns = columns?;
    if end.saturating_sub(start) < MIN_TABLE_ROWS {
        return None;
    }
    Some((end, columns))
}

fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|")
}

fn table_row(line: &Line, columns: &[f32]) -> String {
    let mut cells = vec![String::new(); columns.len()];
    for run in &line.runs {
        let index = columns
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                (*left - run.x).abs().total_cmp(&(*right - run.x).abs())
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        if let Some(cell) = cells.get_mut(index) {
            if !cell.is_empty() {
                cell.push(' ');
            }
            cell.push_str(run.text.trim());
        }
    }
    let cells: Vec<String> = cells.iter().map(|cell| escape_cell(cell.trim())).collect();
    format!("| {} |", cells.join(" | "))
}

pub(super) fn render_page(page: &PageLayout, body: f32, tiers: &[u32], tolerance: f32) -> String {
    if let Some(text) = &page.fallback_text {
        return text.clone();
    }
    let levels: Vec<Option<usize>> = page
        .lines
        .iter()
        .map(|line| heading_level(line, body, tiers))
        .collect();
    let headings: Vec<bool> = levels.iter().map(Option::is_some).collect();
    let mut out = String::new();
    let mut index = 0;
    while let Some(line) = page.lines.get(index) {
        let level = levels.get(index).copied().flatten();
        // A heading is never a table row, even when it happens to have three
        // runs: promoting it would hide the structure the chunker wants most.
        if level.is_none() {
            if let Some((end, columns)) = table_block(&page.lines, &headings, index, tolerance) {
                for row_index in index..end {
                    let Some(row) = page.lines.get(row_index) else {
                        break;
                    };
                    out.push_str(&table_row(row, &columns));
                    out.push('\n');
                    if row_index == index {
                        let separator: Vec<&str> = columns.iter().map(|_| "---").collect();
                        out.push_str(&format!("| {} |\n", separator.join(" | ")));
                    }
                }
                index = end;
                continue;
            }
        }
        let text = line.text();
        if !text.is_empty() {
            if let Some(level) = level {
                out.push_str(&"#".repeat(level));
                out.push(' ');
            }
            out.push_str(&text);
            out.push('\n');
        }
        index += 1;
    }
    out.trim_end().to_string()
}
