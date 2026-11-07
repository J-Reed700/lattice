//! A line diff of two texts, for showing a fix as the lines it removes and the lines it adds.

use serde::{Deserialize, Serialize};

/// What a diff does with one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    /// In both texts.
    Unchanged,
    /// Only in the first text.
    Removed,
    /// Only in the second text.
    Added,
}

/// One line of a diff, without its line break.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

/// The lines that turn `before` into `after`, in order: the lines of both texts, each marked as in both, only in
/// `before` or only in `after`. The lines in both are a longest common run, so the diff changes as few lines as it
/// can. Line endings are not part of a line.
pub fn line_diff(before: &str, after: &str) -> Vec<DiffLine> {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - head;
    let tail = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    let middle_old = old.get(head..old.len() - tail).unwrap_or_default();
    let middle_new = new.get(head..new.len() - tail).unwrap_or_default();
    let mut lines: Vec<DiffLine> = old
        .iter()
        .take(head)
        .map(|text| line(DiffKind::Unchanged, text))
        .collect();
    lines.extend(between(middle_old, middle_new));
    lines.extend(
        old.iter()
            .skip(old.len() - tail)
            .map(|text| line(DiffKind::Unchanged, text)),
    );
    lines
}

fn line(kind: DiffKind, text: &str) -> DiffLine {
    DiffLine {
        kind,
        text: text.to_owned(),
    }
}

/// The diff of two runs of lines that share neither their first nor their last line.
fn between(old: &[&str], new: &[&str]) -> Vec<DiffLine> {
    let width = new.len() + 1;
    // The cell at row `i` and column `j` holds the length of the longest common run of `old[i..]` and `new[j..]`.
    let mut table = vec![0u32; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            let length = if old.get(i) == new.get(j) {
                cell(&table, width, i + 1, j + 1) + 1
            } else {
                cell(&table, width, i + 1, j).max(cell(&table, width, i, j + 1))
            };
            if let Some(slot) = table.get_mut(i * width + j) {
                *slot = length;
            }
        }
    }
    let text = |lines: &[&str], at: usize| lines.get(at).copied().unwrap_or_default().to_owned();
    let mut diff = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old.get(i) == new.get(j) {
            diff.push(line(DiffKind::Unchanged, &text(old, i)));
            i += 1;
            j += 1;
        } else if cell(&table, width, i + 1, j) >= cell(&table, width, i, j + 1) {
            diff.push(line(DiffKind::Removed, &text(old, i)));
            i += 1;
        } else {
            diff.push(line(DiffKind::Added, &text(new, j)));
            j += 1;
        }
    }
    diff.extend(
        old.iter()
            .skip(i)
            .map(|removed| line(DiffKind::Removed, removed)),
    );
    diff.extend(new.iter().skip(j).map(|added| line(DiffKind::Added, added)));
    diff
}

fn cell(table: &[u32], width: usize, row: usize, column: usize) -> u32 {
    table.get(row * width + column).copied().unwrap_or(0)
}
