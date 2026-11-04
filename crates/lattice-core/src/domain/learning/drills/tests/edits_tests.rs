//! Planting edits: where each one lands, the faults, and what holds for any
//! edits that are accepted.

use proptest::prelude::*;

use super::*;
use crate::domain::learning::drills::edits::replace;

/// Lines `span.start` to `span.end` of `text`, with their line breaks.
fn lines_of(text: &str, span: &LineSpan) -> String {
    text.split_inclusive('\n')
        .skip(span.start as usize - 1)
        .take((span.end - span.start) as usize + 1)
        .collect()
}

fn applied(files: &[DrillFile], edits: &[Edit]) -> (Vec<DrillFile>, Vec<LineSpan>) {
    apply(files, edits).unwrap()
}

fn fault(files: &[DrillFile], edits: &[Edit]) -> EditFault {
    apply(files, edits).unwrap_err()
}

#[test]
fn an_edit_replaces_its_before_text_and_its_span_is_the_line_it_lands_on() {
    let files = [file("main.py", "a\nb\nc\n")];
    let (edited, spans) = applied(&files, &[edit("main.py", "b", "B")]);
    assert_eq!(edited, vec![file("main.py", "a\nB\nc\n")]);
    assert_eq!(spans, vec![span("main.py", 2, 2)]);
}

#[test]
fn an_edit_inside_a_line_spans_that_line() {
    let files = [file("main.py", "x = 1\ntotal = a + b\n")];
    let (edited, spans) = applied(&files, &[edit("main.py", "+", "-")]);
    assert_eq!(edited[0].text, "x = 1\ntotal = a - b\n");
    assert_eq!(spans, vec![span("main.py", 2, 2)]);
}

#[test]
fn spans_come_in_the_order_of_the_edits_whatever_their_order_in_the_file() {
    let files = [file("main.py", "a\nb\nc\n")];
    let edits = [edit("main.py", "c", "C"), edit("main.py", "a", "A")];
    let (edited, spans) = applied(&files, &edits);
    assert_eq!(edited[0].text, "A\nb\nC\n");
    assert_eq!(spans, vec![span("main.py", 3, 3), span("main.py", 1, 1)]);
}

#[test]
fn every_edit_is_found_in_the_original_text_and_its_span_is_in_the_result() {
    let files = [file("main.py", "x = 1\ny = 2\nz = 3\n")];
    let edits = [
        edit("main.py", "x = 1", "x = 1\nw = 0"),
        edit("main.py", "z = 3", "z = 4"),
    ];
    let (edited, spans) = applied(&files, &edits);
    assert_eq!(edited[0].text, "x = 1\nw = 0\ny = 2\nz = 4\n");
    assert_eq!(spans, vec![span("main.py", 1, 2), span("main.py", 4, 4)]);
}

#[test]
fn an_after_text_spans_every_line_it_has_a_character_on() {
    let files = [file("main.py", "a\nb\nc\n")];
    let cases: [(&str, u32, u32); 6] = [
        ("B", 2, 2),
        ("B1\nB2", 2, 3),
        ("B1\nB2\n", 2, 3),
        ("\nB", 2, 3),
        ("B\n\n", 2, 3),
        ("\n", 2, 2),
    ];
    for (after, start, end) in cases {
        let (edited, spans) = applied(&files, &[edit("main.py", "b", after)]);
        assert_eq!(spans, vec![span("main.py", start, end)], "{after:?}");
        assert!(
            lines_of(&edited[0].text, &spans[0]).contains(after),
            "{after:?}"
        );
    }
}

#[test]
fn a_deletion_spans_the_line_where_the_removed_text_began() {
    let files = [file("main.py", "a\nb\nc\n")];
    let (edited, spans) = applied(&files, &[edit("main.py", "b\n", "")]);
    assert_eq!(edited[0].text, "a\nc\n");
    assert_eq!(spans, vec![span("main.py", 2, 2)]);
    let files = [file("main.py", "a\nfoo bar\nc\n")];
    let (edited, spans) = applied(&files, &[edit("main.py", "foo ", "")]);
    assert_eq!(edited[0].text, "a\nbar\nc\n");
    assert_eq!(spans, vec![span("main.py", 2, 2)]);
}

#[test]
fn a_deletion_that_began_after_the_final_line_break_spans_the_last_line() {
    let (edited, spans) = applied(&[file("main.py", "a\nb\n")], &[edit("main.py", "b\n", "")]);
    assert_eq!(edited[0].text, "a\n");
    assert_eq!(spans, vec![span("main.py", 1, 1)]);
    let (edited, spans) = applied(&[file("main.py", "x")], &[edit("main.py", "x", "")]);
    assert_eq!(edited[0].text, "");
    assert_eq!(spans, vec![span("main.py", 1, 1)]);
}

#[test]
fn every_file_comes_back_in_order_and_one_without_an_edit_is_unchanged() {
    let files = [
        file("a.py", "one\n"),
        file("b.py", "two\nthree\n"),
        file("c.py", "four\n"),
    ];
    let (edited, spans) = applied(&files, &[edit("b.py", "three", "THREE")]);
    assert_eq!(
        edited,
        vec![
            file("a.py", "one\n"),
            file("b.py", "two\nTHREE\n"),
            file("c.py", "four\n"),
        ]
    );
    assert_eq!(spans, vec![span("b.py", 2, 2)]);
}

#[test]
fn no_edits_change_nothing() {
    let files = [file("a.py", "one\n"), file("b.py", "")];
    let (edited, spans) = applied(&files, &[]);
    assert_eq!(edited, files.to_vec());
    assert!(spans.is_empty());
}

#[test]
fn an_edit_is_looked_for_only_in_its_own_file() {
    let files = [file("a.py", "x = 1\n"), file("b.py", "x = 1\n")];
    let (edited, spans) = applied(&files, &[edit("b.py", "x = 1", "x = 2")]);
    assert_eq!(edited[0].text, "x = 1\n");
    assert_eq!(edited[1].text, "x = 2\n");
    assert_eq!(spans, vec![span("b.py", 1, 1)]);
}

#[test]
fn edits_at_the_same_place_in_different_files_do_not_overlap() {
    let files = [file("a.py", "x = 1\n"), file("b.py", "x = 1\n")];
    let edits = [edit("a.py", "x", "y"), edit("b.py", "x", "z")];
    let (edited, spans) = applied(&files, &edits);
    assert_eq!(edited[0].text, "y = 1\n");
    assert_eq!(edited[1].text, "z = 1\n");
    assert_eq!(spans, vec![span("a.py", 1, 1), span("b.py", 1, 1)]);
}

#[test]
fn matching_is_exact_byte_for_byte() {
    let files = [file("main.py", "Total = 1  \n")];
    for before in ["total = 1", "Total  = 1", "Total = 1\t", "Total = 1\r\n"] {
        assert_eq!(
            fault(&files, &[edit("main.py", before, "x")]),
            EditFault::NotFound { edit: 0 },
            "{before:?}"
        );
    }
}

#[test]
fn text_with_multi_byte_characters_is_edited_and_counted_by_character() {
    let files = [file("main.py", "# café ☕\nname = 'é'\n")];
    let (edited, spans) = applied(&files, &[edit("main.py", "'é'", "'e'")]);
    assert_eq!(edited[0].text, "# café ☕\nname = 'e'\n");
    assert_eq!(spans, vec![span("main.py", 2, 2)]);
    assert_eq!(
        fault(&[file("main.py", "ééé")], &[edit("main.py", "éé", "x")]),
        EditFault::Ambiguous { edit: 0, count: 2 }
    );
}

#[test]
fn an_edit_and_then_its_reverse_restore_the_original() {
    let original = [file(
        "main.py",
        "def total(items):\n    return sum(items)\n",
    )];
    let forward = edit("main.py", "sum(items)", "sum(items[1:])");
    let (planted, _) = applied(&original, std::slice::from_ref(&forward));
    assert_eq!(
        planted[0].text,
        "def total(items):\n    return sum(items[1:])\n"
    );
    let reverse = edit("main.py", &forward.after, &forward.before);
    let (restored, _) = applied(&planted, &[reverse]);
    assert_eq!(restored, original.to_vec());
}

#[test]
fn the_reverse_of_a_deletion_is_an_insertion_which_an_edit_cannot_express() {
    let original = [file("main.py", "a\nb\nc\n")];
    let (planted, _) = applied(&original, &[edit("main.py", "b\n", "")]);
    assert_eq!(
        fault(&planted, &[edit("main.py", "", "b\n")]),
        EditFault::Empty { edit: 0 }
    );
}

#[test]
fn an_edit_for_a_file_there_is_not_is_an_unknown_file() {
    let files = [file("main.py", "a\n")];
    let edits = [edit("main.py", "a", "b"), edit("other.py", "a", "b")];
    assert_eq!(fault(&files, &edits), EditFault::UnknownFile { edit: 1 });
}

#[test]
fn an_empty_before_is_empty() {
    let files = [file("main.py", "a\n")];
    assert_eq!(
        fault(&files, &[edit("main.py", "", "b")]),
        EditFault::Empty { edit: 0 }
    );
    assert_eq!(
        fault(&[file("main.py", "")], &[edit("main.py", "", "b")]),
        EditFault::Empty { edit: 0 }
    );
}

#[test]
fn a_before_the_file_lacks_is_not_found() {
    let files = [file("main.py", "a\n")];
    assert_eq!(
        fault(&files, &[edit("main.py", "b", "c")]),
        EditFault::NotFound { edit: 0 }
    );
    assert_eq!(
        fault(&[file("main.py", "")], &[edit("main.py", "b", "c")]),
        EditFault::NotFound { edit: 0 }
    );
}

#[test]
fn a_before_that_occurs_more_than_once_is_ambiguous_and_the_occurrences_are_counted() {
    let files = [file("main.py", "a b a\nb a\n")];
    assert_eq!(
        fault(&files, &[edit("main.py", "a", "x")]),
        EditFault::Ambiguous { edit: 0, count: 3 }
    );
    assert_eq!(
        fault(&files, &[edit("main.py", "b a", "x")]),
        EditFault::Ambiguous { edit: 0, count: 2 }
    );
}

#[test]
fn occurrences_that_overlap_each_other_are_counted_too() {
    assert_eq!(
        fault(&[file("main.py", "aaa")], &[edit("main.py", "aa", "x")]),
        EditFault::Ambiguous { edit: 0, count: 2 }
    );
}

#[test]
fn a_before_that_occurs_once_is_not_ambiguous() {
    let files = [file("main.py", "a b a\n")];
    let (edited, _) = applied(&files, &[edit("main.py", "b", "x")]);
    assert_eq!(edited[0].text, "a x a\n");
}

#[test]
fn the_befores_of_two_edits_that_overlap_are_refused() {
    let files = [file("main.py", "abcdef")];
    let edits = [edit("main.py", "abc", "X"), edit("main.py", "cde", "Y")];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
    let edits = [edit("main.py", "cde", "Y"), edit("main.py", "abc", "X")];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
}

#[test]
fn a_before_inside_another_is_refused() {
    let files = [file("main.py", "abcdef")];
    let edits = [edit("main.py", "bcd", "X"), edit("main.py", "c", "Y")];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
}

#[test]
fn the_befores_of_two_edits_that_touch_are_refused() {
    let files = [file("main.py", "abcdef")];
    let edits = [edit("main.py", "abc", "X"), edit("main.py", "def", "Y")];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
    let files = [file("main.py", "foo\nbar\n")];
    let edits = [edit("main.py", "foo\n", "x\n"), edit("main.py", "bar", "y")];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
}

#[test]
fn the_befores_of_two_edits_a_character_apart_on_different_lines_are_accepted() {
    let files = [file("main.py", "foo\nbar\n")];
    let edits = [edit("main.py", "foo", "x"), edit("main.py", "bar", "y")];
    let (edited, spans) = applied(&files, &edits);
    assert_eq!(edited[0].text, "x\ny\n");
    assert_eq!(spans, vec![span("main.py", 1, 1), span("main.py", 2, 2)]);
}

#[test]
fn two_edits_that_would_share_a_line_in_the_result_are_refused() {
    let files = [file("main.py", "a b\n")];
    let edits = [edit("main.py", "a", "x"), edit("main.py", "b", "y")];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
}

#[test]
fn an_edit_that_joins_lines_can_make_two_edits_share_one() {
    let files = [file("main.py", "a\n b\n")];
    let edits = [edit("main.py", "a\n", "x"), edit("main.py", "b", "y")];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
}

#[test]
fn an_after_that_runs_onto_the_next_edits_line_makes_them_overlap() {
    let files = [file("main.py", "a b\nc\n")];
    let edits = [edit("main.py", "a", "x\ny"), edit("main.py", "b", "z")];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
}

#[test]
fn the_first_overlapping_pair_in_list_order_is_reported() {
    let files = [file("main.py", "abcdefghi")];
    let edits = [
        edit("main.py", "abc", "1"),
        edit("main.py", "ghi", "2"),
        edit("main.py", "cdefg", "3"),
    ];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 2
        }
    );
}

#[test]
fn an_edit_that_cannot_be_placed_is_reported_before_any_overlap() {
    let files = [file("main.py", "abcdef")];
    let edits = [
        edit("main.py", "abc", "1"),
        edit("main.py", "cde", "2"),
        edit("main.py", "zzz", "3"),
    ];
    assert_eq!(fault(&files, &edits), EditFault::NotFound { edit: 2 });
}

#[test]
fn the_first_edit_that_cannot_be_placed_is_reported() {
    let files = [file("main.py", "a a b\n")];
    let edits = [
        edit("main.py", "b", "x"),
        edit("main.py", "a", "x"),
        edit("main.py", "q", "x"),
    ];
    assert_eq!(
        fault(&files, &edits),
        EditFault::Ambiguous { edit: 1, count: 2 }
    );
}

#[test]
fn one_edits_faults_are_found_in_the_order_of_the_file_then_the_before() {
    let files = [file("main.py", "a\n")];
    assert_eq!(
        fault(&files, &[edit("other.py", "", "x")]),
        EditFault::UnknownFile { edit: 0 }
    );
    assert_eq!(
        fault(&files, &[edit("main.py", "", "x")]),
        EditFault::Empty { edit: 0 }
    );
}

#[test]
fn a_replacement_accepts_two_results_on_one_line_which_apply_refuses() {
    let files = [file("main.py", "a b\n")];
    let edits = [edit("main.py", "a", "x"), edit("main.py", "b", "y")];
    let (edited, spans) = replace(&files, &edits).unwrap();
    assert_eq!(edited[0].text, "x y\n");
    assert_eq!(spans, vec![span("main.py", 1, 1), span("main.py", 1, 1)]);
    assert_eq!(
        fault(&files, &edits),
        EditFault::Overlaps {
            first: 0,
            second: 1
        }
    );
}

#[test]
fn a_replacement_refuses_what_apply_refuses_but_for_shared_lines() {
    let files = [file("main.py", "abcdef a\n")];
    let cases = [
        (
            vec![edit("other.py", "a", "b")],
            EditFault::UnknownFile { edit: 0 },
        ),
        (vec![edit("main.py", "", "b")], EditFault::Empty { edit: 0 }),
        (
            vec![edit("main.py", "zz", "b")],
            EditFault::NotFound { edit: 0 },
        ),
        (
            vec![edit("main.py", "a", "b")],
            EditFault::Ambiguous { edit: 0, count: 2 },
        ),
        (
            vec![edit("main.py", "abc", "X"), edit("main.py", "cde", "Y")],
            EditFault::Overlaps {
                first: 0,
                second: 1,
            },
        ),
        (
            vec![edit("main.py", "abc", "X"), edit("main.py", "def", "Y")],
            EditFault::Overlaps {
                first: 0,
                second: 1,
            },
        ),
    ];
    for (edits, expected) in cases {
        assert_eq!(replace(&files, &edits).unwrap_err(), expected);
        assert_eq!(fault(&files, &edits), expected);
    }
}

/// A generated line: its filler and, when the line is edited, the filler of
/// the text that replaces it.
type Row = (String, Option<String>);

fn rows(replacement: &'static str) -> impl Strategy<Value = Vec<Row>> {
    proptest::collection::vec(("[a-c ]{0,4}", proptest::option::of(replacement)), 1..10)
}

/// A file of numbered lines, `L000:` and a filler each, and one edit for every
/// line that has a replacement, which turns the whole line, without its line
/// break, into `after(number, replacement)`. Every `before` occurs once.
fn build(
    rows: &[Row],
    final_break: bool,
    after: impl Fn(usize, &str) -> String,
) -> (DrillFile, Vec<Edit>) {
    let mut lines = Vec::new();
    let mut edits = Vec::new();
    for (number, (filler, replacement)) in rows.iter().enumerate() {
        let line = format!("L{number:03}:{filler}");
        if let Some(replacement) = replacement {
            edits.push(edit("main.py", &line, &after(number, replacement)));
        }
        lines.push(line);
    }
    let mut text = lines.join("\n");
    if final_break {
        text.push('\n');
    }
    (file("main.py", &text), edits)
}

/// An `after` that begins `<` and its line number and ends `>`, so that it
/// occurs once in the result.
fn marked(number: usize, replacement: &str) -> String {
    format!("<{number}:{replacement}>")
}

proptest! {
    #[test]
    fn every_span_reads_back_exactly_the_text_that_replaced_its_line(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
    ) {
        let (original, edits) = build(&generated, final_break, marked);
        let (edited, spans) = apply(&[original], &edits).unwrap();
        prop_assert_eq!(spans.len(), edits.len());
        let text = &edited[0].text;
        for (planted, span) in edits.iter().zip(&spans) {
            let read = lines_of(text, span);
            prop_assert!(read.contains(&planted.after));
            prop_assert_eq!(read.strip_suffix('\n').unwrap_or(&read), planted.after.as_str());
            prop_assert!((1..=span.end).contains(&span.start));
            prop_assert!((span.end as usize) <= text.split_inclusive('\n').count());
        }
    }

    #[test]
    fn an_after_text_of_any_shape_is_inside_the_lines_of_its_span(
        generated in rows("[xyz\n]{1,5}"),
        final_break in any::<bool>(),
    ) {
        let (original, edits) =
            build(&generated, final_break, |_, replacement| replacement.to_owned());
        let (edited, spans) = apply(&[original], &edits).unwrap();
        let text = &edited[0].text;
        for (planted, span) in edits.iter().zip(&spans) {
            prop_assert!(lines_of(text, span).contains(&planted.after));
            prop_assert!((1..=span.end).contains(&span.start));
            prop_assert!((span.end as usize) <= text.split_inclusive('\n').count());
        }
    }

    #[test]
    fn files_without_edits_come_back_as_they_were(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
        first_text in "[a-z\n]{0,20}",
        last_text in "[a-z\n]{0,20}",
    ) {
        let (target, edits) = build(&generated, final_break, marked);
        let files = vec![file("first.py", &first_text), target, file("last.py", &last_text)];
        let (edited, _) = apply(&files, &edits).unwrap();
        prop_assert_eq!(edited.len(), 3);
        prop_assert_eq!(&edited[0], &files[0]);
        prop_assert_eq!(edited[1].path.as_str(), "main.py");
        prop_assert_eq!(&edited[2], &files[2]);
    }

    #[test]
    fn applying_the_edits_and_then_their_reverses_restores_the_files(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
    ) {
        let (original, edits) = build(&generated, final_break, marked);
        let files = [original];
        let (planted, _) = apply(&files, &edits).unwrap();
        let reverses: Vec<Edit> = edits
            .iter()
            .map(|forward| edit(&forward.path, &forward.after, &forward.before))
            .collect();
        let (restored, _) = apply(&planted, &reverses).unwrap();
        prop_assert_eq!(restored, files.to_vec());
    }

    #[test]
    fn spans_of_edits_that_do_not_overlap_never_overlap_and_keep_the_files_order(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
    ) {
        let (original, edits) = build(&generated, final_break, marked);
        let (_, spans) = apply(&[original], &edits).unwrap();
        for (first, a) in spans.iter().enumerate() {
            for b in spans.iter().skip(first + 1) {
                prop_assert!(!overlaps(a, b));
                prop_assert!(a.end < b.start);
            }
        }
    }

    #[test]
    fn a_replacement_and_apply_agree_when_no_two_results_share_a_line(
        generated in rows("[xyz\n]{0,4}"),
        final_break in any::<bool>(),
    ) {
        let (original, edits) = build(&generated, final_break, marked);
        let files = [original];
        prop_assert_eq!(replace(&files, &edits), apply(&files, &edits));
    }

    #[test]
    fn an_ambiguous_before_reports_how_many_times_it_occurs(copies in 2usize..8) {
        let text = vec!["x = 1"; copies].join("\n");
        prop_assert_eq!(
            fault(&[file("main.py", &text)], &[edit("main.py", "x = 1", "x = 2")]),
            EditFault::Ambiguous { edit: 0, count: copies }
        );
    }

    #[test]
    fn a_before_is_found_exactly_where_a_brute_force_search_finds_it(
        text in "[ab\n]{0,12}",
        before in "[ab\n]{0,3}",
    ) {
        let result = apply(&[file("main.py", &text)], &[edit("main.py", &before, "X")]);
        if before.is_empty() {
            prop_assert_eq!(result.unwrap_err(), EditFault::Empty { edit: 0 });
        } else {
            let count = (0..text.len())
                .filter(|&at| text[at..].starts_with(before.as_str()))
                .count();
            match count {
                0 => prop_assert_eq!(result.unwrap_err(), EditFault::NotFound { edit: 0 }),
                1 => {
                    let (edited, spans) = result.unwrap();
                    prop_assert_eq!(&edited[0].text, &text.replacen(before.as_str(), "X", 1));
                    prop_assert!(lines_of(&edited[0].text, &spans[0]).contains('X'));
                }
                _ => prop_assert_eq!(
                    result.unwrap_err(),
                    EditFault::Ambiguous { edit: 0, count }
                ),
            }
        }
    }
}
