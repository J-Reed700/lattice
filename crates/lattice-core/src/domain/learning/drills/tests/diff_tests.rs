//! The line diff that shows a fix: the lines in both texts are a longest common run, and the rest are removed or
//! added in order.

use proptest::prelude::*;

use super::*;

fn shown(diff: &[DiffLine]) -> Vec<String> {
    diff.iter()
        .map(|line| {
            let mark = match line.kind {
                DiffKind::Unchanged => ' ',
                DiffKind::Removed => '-',
                DiffKind::Added => '+',
            };
            format!("{mark}{}", line.text)
        })
        .collect()
}

#[test]
fn a_replaced_line_is_removed_and_added_between_the_lines_around_it() {
    let diff = line_diff(
        "def allow(self, key):\n    if len(recent) > self.limit:\n        return False\n",
        "def allow(self, key):\n    if len(recent) >= self.limit:\n        return False\n",
    );
    assert_eq!(
        shown(&diff),
        [
            " def allow(self, key):",
            "-    if len(recent) > self.limit:",
            "+    if len(recent) >= self.limit:",
            "         return False",
        ]
    );
}

#[test]
fn texts_that_are_the_same_have_no_changed_line() {
    let diff = line_diff("a\nb\n", "a\nb\n");
    assert_eq!(shown(&diff), [" a", " b"]);
}

#[test]
fn a_line_kept_between_two_changes_stays_unchanged() {
    let diff = line_diff("one\nkeep\nthree\n", "uno\nkeep\ntres\n");
    assert_eq!(shown(&diff), ["-one", "+uno", " keep", "-three", "+tres"]);
}

#[test]
fn a_line_moved_is_removed_where_it_was_and_added_where_it_is() {
    let diff = line_diff("a\nb\nc\n", "b\nc\na\n");
    assert_eq!(shown(&diff), ["-a", " b", " c", "+a"]);
}

#[test]
fn a_text_with_nothing_in_it_is_all_added_or_all_removed() {
    assert_eq!(shown(&line_diff("", "x\ny\n")), ["+x", "+y"]);
    assert_eq!(shown(&line_diff("x\ny\n", "")), ["-x", "-y"]);
    assert_eq!(line_diff("", ""), Vec::<DiffLine>::new());
}

#[test]
fn a_deleted_block_leaves_the_lines_around_it() {
    let diff = line_diff("head\nstale\nold\ntail\n", "head\ntail\n");
    assert_eq!(shown(&diff), [" head", "-stale", "-old", " tail"]);
}

#[test]
fn line_endings_are_not_part_of_a_line() {
    let diff = line_diff("a\r\nb\r\n", "a\nb\n");
    assert_eq!(shown(&diff), [" a", " b"]);
}

#[test]
fn a_diff_of_long_texts_needs_no_cap() {
    let before: String = (0..3_000).map(|n| format!("line {n}\n")).collect();
    let after: String = (0..3_000)
        .map(|n| {
            if n % 100 == 50 {
                format!("changed {n}\n")
            } else {
                format!("line {n}\n")
            }
        })
        .collect();
    let diff = line_diff(&before, &after);
    let added = diff
        .iter()
        .filter(|line| line.kind == DiffKind::Added)
        .count();
    let removed = diff
        .iter()
        .filter(|line| line.kind == DiffKind::Removed)
        .count();
    assert_eq!((added, removed), (30, 30));
    assert_eq!(diff.len(), 3_030);
}

proptest! {
    #[test]
    fn the_diff_holds_both_texts_and_keeps_the_longest_common_run(
        old in proptest::collection::vec("[ab]{0,2}", 0..9),
        new in proptest::collection::vec("[ab]{0,2}", 0..9),
    ) {
        let text = |lines: &[String]| lines.iter().map(|line| format!("{line}\n")).collect::<String>();
        let diff = line_diff(&text(&old), &text(&new));
        let side = |dropped: DiffKind| -> Vec<String> {
            diff.iter()
                .filter(|line| line.kind != dropped)
                .map(|line| line.text.clone())
                .collect()
        };
        prop_assert_eq!(side(DiffKind::Added), old.clone());
        prop_assert_eq!(side(DiffKind::Removed), new.clone());
        let kept = diff.iter().filter(|line| line.kind == DiffKind::Unchanged).count();
        prop_assert_eq!(kept, longest_common_run(&old, &new));
    }
}

/// The length of the longest common subsequence, by the textbook table.
fn longest_common_run(old: &[String], new: &[String]) -> usize {
    let mut table = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    table[0][0]
}
