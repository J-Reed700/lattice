use proptest::prelude::*;

use super::super::edit::{consistent, Anchor, Applied, Change};
use super::super::shape::{Settled, Why};
use super::super::shown::{Picked, Scope};
use super::*;

const SENTENCES: &str =
    "Squares have four sides. Circles have none.\n\nTriangles have three sides.";
const LINES: &str = "alpha\n  beta\ngamma\n";

/// The rows a prompt shows for `units`, and the units' pointers as a reply would resolve them.
fn shown_rows(units: &Units) -> (String, Vec<Picked<Unit>>) {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = units.shown(&mut showing).expect("units are shown");
    let rows = units.rows(&shown);
    let count = shown.len();
    let picked: Vec<Picked<Unit>> = (1..=count)
        .map(|ordinal| {
            showing
                .resolve::<Unit>(&format!("u{ordinal}"))
                .expect("a shown unit resolves")
        })
        .collect();
    (rows, picked)
}

/// The reference of the unit at `index` of `units`, shown or not.
fn at(units: &Units, index: usize) -> UnitRef {
    UnitRef {
        index,
        ..units.shown[0].at
    }
}

fn nothing() -> Change<Unit, String> {
    Change {
        replace: Vec::new(),
        insert: Vec::new(),
        remove: Vec::new(),
        moved: Vec::new(),
    }
}

fn after(unit: &Picked<Unit>) -> Anchor<Unit> {
    Anchor::After(unit.clone())
}

fn applied(units: &mut Units, change: Change<Unit, String>) -> Applied<Unit> {
    match units.apply(consistent(change).expect("consistent")) {
        Settled::Answered(applied) => applied,
        Settled::Unanswered(why) => panic!("expected the change to apply, got {why:?}"),
    }
}

fn texts() -> Vec<&'static str> {
    vec![
        "",
        "One sentence.",
        "No stop at all",
        "   ",
        "  leading spaces. Then more.",
        "Tabs\tinside. And\tmore.\n",
        SENTENCES,
        "Paragraph one is here. It has two sentences.\n\n\nParagraph two.  \n",
        "line one\r\nline two\r\n\r\nline four",
        "no final newline",
        "trailing newline\n",
        "\n\nstarts blank",
        "\tindented\n  more\n    most",
        "mixed\r\nendings\nhere\r\n",
        "lone \r carriage return",
    ]
}

#[test]
fn an_unchanged_text_comes_back_byte_for_byte_at_both_grains() {
    for text in texts() {
        for grain in [Grain::Sentence, Grain::Line] {
            let units = Units::of(text, grain);
            assert_eq!(units.text(), text, "{grain:?} of {text:?}");
            assert!(!units.changed());
        }
    }
}

proptest! {
    #[test]
    fn any_text_round_trips(text in any::<String>()) {
        for grain in [Grain::Sentence, Grain::Line] {
            prop_assert_eq!(Units::of(&text, grain).text(), text.clone());
        }
    }
}

#[test]
fn sentence_units_are_the_segmentations_sentences() {
    let units = Units::of(SENTENCES, Grain::Sentence);
    assert_eq!(units.shown.len(), 3);
    assert_eq!(
        units.text_of(at(&units, 0)),
        Some("Squares have four sides.")
    );
    assert_eq!(units.text_of(at(&units, 1)), Some("Circles have none."));
    assert_eq!(
        units.text_of(at(&units, 2)),
        Some("Triangles have three sides.")
    );
    assert_eq!(units.text_of(at(&units, 3)), None);
    assert_eq!(units.gaps.len(), units.shown.len() + 1);
}

#[test]
fn line_units_are_the_lines_without_their_breaks() {
    let units = Units::of("a\r\n  b\n\nc", Grain::Line);
    let lines: Vec<&str> = units.shown.iter().map(Unit::text).collect();
    assert_eq!(lines, ["a", "  b", "", "c"]);
    assert_eq!(units.gaps, ["", "\r\n", "\n", "\n", ""]);

    let units = Units::of("a\nb\n", Grain::Line);
    let lines: Vec<&str> = units.shown.iter().map(Unit::text).collect();
    assert_eq!(lines, ["a", "b"]);
    assert_eq!(units.gaps, ["", "\n", "\n"]);
}

#[test]
fn an_empty_text_has_no_units() {
    for grain in [Grain::Sentence, Grain::Line] {
        let units = Units::of("", grain);
        assert!(units.shown.is_empty());
        assert_eq!(units.gaps.len(), 1);
        let (rows, picked) = shown_rows(&units);
        assert_eq!(rows, "");
        assert!(picked.is_empty());
    }
}

#[test]
fn a_text_of_one_unit_has_one_row() {
    let units = Units::of("Only this.", Grain::Sentence);
    let (rows, picked) = shown_rows(&units);
    assert_eq!(rows, "[u1] Only this.");
    assert_eq!(picked.len(), 1);
}

#[test]
fn rows_have_one_line_per_unit_each_starting_with_a_handle_that_resolves() {
    for (text, grain) in [(SENTENCES, Grain::Sentence), (LINES, Grain::Line)] {
        let units = Units::of(text, grain);
        let (rows, picked) = shown_rows(&units);
        assert_eq!(rows.lines().count(), units.shown.len());
        assert_eq!(picked.len(), units.shown.len());
        for (row, unit) in rows.lines().zip(&picked) {
            let written = unit.handle.written();
            assert!(row.starts_with(&format!("[{written}] ")), "{row}");
            assert_eq!(unit.handle.bracketed(), format!("[{written}]"));
        }
    }
}

#[test]
fn sentence_rows_hold_the_sentence_and_line_rows_hold_the_line_as_it_is() {
    let (rows, _) = shown_rows(&Units::of(SENTENCES, Grain::Sentence));
    assert_eq!(
        rows,
        "[u1] Squares have four sides.\n[u2] Circles have none.\n[u3] Triangles have three sides."
    );
    let (rows, _) = shown_rows(&Units::of("alpha\n  beta\n\ngamma", Grain::Line));
    assert_eq!(rows, "[u1] alpha\n[u2]   beta\n[u3] \n[u4] gamma");
}

#[test]
fn a_unit_with_a_line_break_in_it_is_one_row() {
    let mut units = Units::of("One.", Grain::Sentence);
    let second = at(&units, 1);
    units.shown.push(Unit {
        at: second,
        text: "two\nlines\r\nhere".to_owned(),
    });
    let (rows, _) = shown_rows(&units);
    assert_eq!(rows, "[u1] One.\n[u2] two lines  here");
}

#[test]
fn a_sentence_is_replaced_where_it_was() {
    let mut units = Units::of(SENTENCES, Grain::Sentence);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change
        .replace
        .push((u[1].clone(), "Circles are round.".to_owned()));
    let result = applied(&mut units, change);
    assert_eq!(
        units.text(),
        "Squares have four sides. Circles are round.\n\nTriangles have three sides."
    );
    assert_eq!(result.wrote, 1);
    assert!(units.changed());
}

#[test]
fn a_sentence_is_inserted_after_its_anchor() {
    let mut units = Units::of(SENTENCES, Grain::Sentence);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change
        .insert
        .push((after(&u[0]), "Hexagons have six sides.".to_owned()));
    applied(&mut units, change);
    assert_eq!(
        units.text(),
        "Squares have four sides. Hexagons have six sides. Circles have none.\n\nTriangles have three sides."
    );

    let mut units = Units::of(SENTENCES, Grain::Sentence);
    let mut change = nothing();
    change.insert.push((after(&u[2]), "Done.".to_owned()));
    change.insert.push((Anchor::Start, "First.".to_owned()));
    applied(&mut units, change);
    assert_eq!(
        units.text(),
        "First. Squares have four sides. Circles have none.\n\nTriangles have three sides. Done."
    );
}

#[test]
fn a_sentence_is_removed() {
    let mut units = Units::of(SENTENCES, Grain::Sentence);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change.remove.push(u[1].clone());
    applied(&mut units, change);
    assert_eq!(
        units.text(),
        "Squares have four sides.

Triangles have three sides."
    );
    assert!(units.changed());

    let mut units = Units::of(SENTENCES, Grain::Sentence);
    let mut change = nothing();
    change.remove.push(u[2].clone());
    applied(&mut units, change);
    assert_eq!(units.text(), "Squares have four sides. Circles have none.");
}

#[test]
fn a_sentence_is_moved() {
    let mut units = Units::of(SENTENCES, Grain::Sentence);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change.moved.push((u[2].clone(), Anchor::Start));
    applied(&mut units, change);
    assert_eq!(
        units.text(),
        "Triangles have three sides. Squares have four sides. Circles have none."
    );
    assert!(units.changed());
}

#[test]
fn a_line_is_replaced_by_one_or_by_several() {
    let mut units = Units::of(LINES, Grain::Line);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change.replace.push((u[1].clone(), "  BETA".to_owned()));
    applied(&mut units, change);
    assert_eq!(units.text(), "alpha\n  BETA\ngamma\n");

    let mut units = Units::of(LINES, Grain::Line);
    let mut change = nothing();
    change.replace.push((u[0].clone(), "x\ny\nz".to_owned()));
    applied(&mut units, change);
    assert_eq!(units.text(), "x\ny\nz\n  beta\ngamma\n");
}

#[test]
fn a_line_is_inserted_removed_and_moved() {
    let mut units = Units::of(LINES, Grain::Line);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change.insert.push((after(&u[2]), "delta".to_owned()));
    applied(&mut units, change);
    assert_eq!(units.text(), "alpha\n  beta\ngamma\ndelta\n");

    let mut units = Units::of(LINES, Grain::Line);
    let mut change = nothing();
    change.insert.push((Anchor::Start, "zero".to_owned()));
    applied(&mut units, change);
    assert_eq!(units.text(), "zero\nalpha\n  beta\ngamma\n");

    let mut units = Units::of(LINES, Grain::Line);
    let mut change = nothing();
    change.remove.push(u[0].clone());
    applied(&mut units, change);
    assert_eq!(units.text(), "  beta\ngamma\n");

    let mut units = Units::of(LINES, Grain::Line);
    let mut change = nothing();
    change.moved.push((u[0].clone(), after(&u[2])));
    applied(&mut units, change);
    assert_eq!(units.text(), "  beta\ngamma\nalpha\n");
}

#[test]
fn lines_moved_in_a_crlf_text_keep_its_line_breaks() {
    let mut units = Units::of("a\r\nb\r\nc", Grain::Line);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change.moved.push((u[2].clone(), Anchor::Start));
    applied(&mut units, change);
    assert_eq!(units.text(), "c\r\na\r\nb");
}

#[test]
fn a_second_change_on_a_claimed_unit_is_refused_and_the_text_is_unchanged() {
    for (text, grain) in [(SENTENCES, Grain::Sentence), (LINES, Grain::Line)] {
        let mut units = Units::of(text, grain);
        let (_, u) = shown_rows(&units);
        let mut first = nothing();
        first.replace.push((u[1].clone(), "Changed".to_owned()));
        applied(&mut units, first);
        let after_first = units.text();

        let mut second = nothing();
        second.remove.push(u[1].clone());
        second.insert.push((Anchor::Start, "Also".to_owned()));
        let settled = units.apply(consistent(second).expect("consistent"));
        assert!(matches!(settled, Settled::Unanswered(Why::Contradicted(_))));
        assert_eq!(units.text(), after_first);

        let mut anchored = nothing();
        anchored.insert.push((after(&u[1]), "Also".to_owned()));
        let settled = units.apply(consistent(anchored).expect("consistent"));
        assert!(matches!(settled, Settled::Unanswered(Why::Contradicted(_))));
        assert_eq!(units.text(), after_first);
    }
}

#[test]
fn text_of_a_unit_is_the_original_even_after_a_change() {
    let mut units = Units::of(SENTENCES, Grain::Sentence);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change.replace.push((u[0].clone(), "Replaced.".to_owned()));
    applied(&mut units, change);
    assert_eq!(
        units.text_of(at(&units, 0)),
        Some("Squares have four sides.")
    );
}

#[test]
fn units_of_two_texts_shown_in_one_request_are_never_one_thing() {
    let first = Units::of("Ann ran. Bob sat.", Grain::Sentence);
    let mut second = Units::of("Cy hid. Di won.", Grain::Sentence);
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let rows_of_first = first.rows(&first.shown(&mut showing).expect("shown"));
    let rows_of_second = second.rows(&second.shown(&mut showing).expect("shown"));
    assert_eq!(rows_of_first, ["[u1] Ann ran.", "[u2] Bob sat."].join("\n"));
    assert_eq!(rows_of_second, ["[u3] Cy hid.", "[u4] Di won."].join("\n"));

    let of_first = showing
        .resolve::<Unit>("u1")
        .expect("a shown unit resolves");
    assert_eq!(second.text_of(of_first.reference), None);
    let mut change = nothing();
    change.remove.push(of_first);
    assert_eq!(
        second.apply(consistent(change).expect("consistent")),
        Settled::Unanswered(Why::Contradicted("u1 cannot be placed.".to_owned()))
    );
    assert_eq!(second.text(), "Cy hid. Di won.");
}

#[test]
fn the_same_text_shown_again_keeps_its_handles() {
    let once = Units::of("Ann ran. Bob sat.", Grain::Sentence);
    let again = Units::of("Ann ran. Bob sat.", Grain::Sentence);
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let first = once.rows(&once.shown(&mut showing).expect("shown"));
    let second = again.rows(&again.shown(&mut showing).expect("shown"));
    assert_eq!(first, second);
}

/// `text` after `edit` has written its change over the units, at `grain`.
fn edited(
    text: &str,
    grain: Grain,
    edit: impl FnOnce(&[Picked<Unit>], &mut Change<Unit, String>),
) -> String {
    let mut units = Units::of(text, grain);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    edit(&u, &mut change);
    applied(&mut units, change);
    units.text()
}

#[test]
fn a_paragraph_break_beside_a_changed_sentence_stays() {
    let text = "Ann ran. Bob sat.\n\nCy hid.";
    let removed = edited(text, Grain::Sentence, |u, change| {
        change.remove.push(u[1].clone());
    });
    assert_eq!(removed, "Ann ran.\n\nCy hid.");
    let inserted = edited(text, Grain::Sentence, |u, change| {
        change.insert.push((after(&u[1]), "Di swam.".to_owned()));
    });
    assert_eq!(inserted, "Ann ran. Bob sat. Di swam.\n\nCy hid.");

    let text = "Ann ran.\n\nBob sat. Cy hid.";
    let both = edited(text, Grain::Sentence, |u, change| {
        change.remove.push(u[1].clone());
        change.insert.push((after(&u[1]), "Di swam.".to_owned()));
    });
    assert_eq!(both, "Ann ran.\n\nDi swam. Cy hid.");

    let text = "Ann ran. Bob sat.\n\nCy hid. Di swam.";
    let moved = edited(text, Grain::Sentence, |u, change| {
        change.moved.push((u[2].clone(), after(&u[0])));
    });
    assert_eq!(moved, "Ann ran. Cy hid. Bob sat.\n\nDi swam.");
}

#[test]
fn lines_are_removed_and_inserted_with_the_texts_own_line_breaks() {
    let middle = edited("a\nb\nc", Grain::Line, |u, change| {
        change.remove.push(u[1].clone());
    });
    assert_eq!(middle, "a\nc");
    let last = edited("a\nb\n", Grain::Line, |u, change| {
        change.remove.push(u[1].clone());
    });
    assert_eq!(last, "a\n");
    let last = edited("a\nb", Grain::Line, |u, change| {
        change.remove.push(u[1].clone());
    });
    assert_eq!(last, "a");
    let appended = edited("a\nb", Grain::Line, |u, change| {
        change.insert.push((after(&u[1]), "X".to_owned()));
    });
    assert_eq!(appended, "a\nb\nX");
    let crlf = edited("a\r\nb", Grain::Line, |u, change| {
        change.insert.push((after(&u[0]), "X".to_owned()));
    });
    assert_eq!(crlf, "a\r\nX\r\nb");
    let crlf = edited("a\r\nb\r\nc", Grain::Line, |u, change| {
        change.remove.push(u[1].clone());
    });
    assert_eq!(crlf, "a\r\nc");
}

#[test]
fn changes_applied_one_after_another_read_as_one() {
    let mut units = Units::of("Ann ran. Bob sat. Cy hid. Di swam.", Grain::Sentence);
    let (_, u) = shown_rows(&units);
    let mut first = nothing();
    first.moved.push((u[2].clone(), after(&u[0])));
    applied(&mut units, first);
    let mut second = nothing();
    second.moved.push((u[3].clone(), after(&u[0])));
    applied(&mut units, second);
    assert_eq!(units.text(), "Ann ran. Cy hid. Di swam. Bob sat.");
}
