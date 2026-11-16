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

/// `SENTENCES` with its sentence at `index` replaced by `with`.
fn replaced(index: usize, with: &str) -> String {
    let mut units = Units::of(SENTENCES, Grain::Sentence);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change.replace.push((u[index].clone(), with.to_owned()));
    applied(&mut units, change);
    units.text()
}

#[test]
fn a_replacement_written_as_its_row_is_the_text_after_its_label() {
    assert_eq!(
        replaced(1, "[u2] Circles are round."),
        replaced(1, "Circles are round.")
    );
    assert_eq!(
        replaced(1, "\n\n[u2] Circles are round."),
        replaced(1, "\n\nCircles are round.")
    );
}

#[test]
fn a_replacement_that_starts_with_another_units_label_is_left_as_written() {
    assert!(replaced(1, "[u1] Circles are round.").contains("[u1] Circles are round."));
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

#[test]
fn units_of_equal_texts_in_two_places_are_not_one_thing() {
    let first = Units::of_at("Ann ran. Bob sat.", Grain::Sentence, "/a");
    let second = Units::of_at("Ann ran. Bob sat.", Grain::Sentence, "/b");
    let bare = Units::of("Ann ran. Bob sat.", Grain::Sentence);
    assert_ne!(first.shown[0].at, second.shown[0].at);
    assert_ne!(first.shown[0].at, bare.shown[0].at);
    assert_eq!(first.text(), second.text());
    assert!(first.holds(first.shown[1].at));
    assert!(!first.holds(second.shown[1].at));
    assert!(!first.holds(bare.shown[1].at));
}

#[test]
fn units_of_one_place_keep_their_identity_when_the_text_is_cut_again() {
    let once = Units::of_at("Ann ran. Bob sat.", Grain::Sentence, "/a");
    let again = Units::of_at("Ann ran. Bob sat.", Grain::Sentence, "/a");
    assert_eq!(once.shown[0].at, again.shown[0].at);
}

#[test]
fn spans_are_the_bytes_of_each_unit_in_the_text_as_shown() {
    let text = "  Ann ran. Bob sat.\n\nCy hid.  ";
    let units = Units::of(text, Grain::Sentence);
    let spans = units.spans();
    let cut: Vec<&str> = spans
        .iter()
        .map(|(_, range)| &text[range.clone()])
        .collect();
    assert_eq!(cut, ["Ann ran.", "Bob sat.", "Cy hid."]);
    assert_eq!(spans[1].0, units.shown[1].at);

    let text = "a\r\n  b\n\nc";
    let units = Units::of(text, Grain::Line);
    let cut: Vec<&str> = units
        .spans()
        .iter()
        .map(|(_, range)| &text[range.clone()])
        .collect();
    assert_eq!(cut, ["a", "  b", "", "c"]);
    assert!(Units::of("", Grain::Sentence).spans().is_empty());
}

#[test]
fn marked_writes_each_handle_before_its_unit_and_keeps_everything_between() {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let other = Units::of("Di swam. Ed hid. Flo sat.", Grain::Sentence);
    other.shown(&mut showing).expect("shown");
    let text = "Ann ran. Bob sat.\n\nCy hid.";
    let units = Units::of(text, Grain::Sentence);
    let shown = units.shown(&mut showing).expect("shown");
    assert_eq!(
        units.marked(&shown),
        "[u4] Ann ran. [u5] Bob sat.\n\n[u6] Cy hid."
    );
    let others = other.shown(&mut showing).expect("shown");
    assert_eq!(units.marked(&others), text);
}

#[test]
fn marked_shows_the_text_as_it_was_shown_after_a_change() {
    let mut units = Units::of("Ann ran. Bob sat.", Grain::Sentence);
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = units.shown(&mut showing).expect("shown");
    let before = units.marked(&shown);
    let u: Vec<Picked<Unit>> = vec![
        showing.resolve::<Unit>("u1").expect("resolves"),
        showing.resolve::<Unit>("u2").expect("resolves"),
    ];
    let mut change = nothing();
    change.remove.push(u[0].clone());
    applied(&mut units, change);
    assert_eq!(before, "[u1] Ann ran. [u2] Bob sat.");
    assert_eq!(units.text(), "Bob sat.");
}

#[test]
fn a_change_touches_what_it_replaced_removed_moved_or_anchored_after() {
    let mut units = Units::of(
        "Ann ran. Bob sat. Cy hid. Di swam. Ed won.",
        Grain::Sentence,
    );
    let (_, u) = shown_rows(&units);
    let refs: Vec<UnitRef> = u.iter().map(|unit| unit.reference).collect();
    assert!(!units.touched(&refs));
    let mut change = nothing();
    change.replace.push((u[1].clone(), "Bob stood.".to_owned()));
    change.insert.push((after(&u[2]), "Cy ran.".to_owned()));
    applied(&mut units, change);
    assert!(units.touched(&[refs[1]]));
    assert!(units.touched(&[refs[2]]));
    assert!(units.touched(&[refs[0], refs[1]]));
    assert!(!units.touched(&[refs[0]]));
    assert!(!units.touched(&[refs[3], refs[4]]));
    assert!(!units.touched(&[]));
}

#[test]
fn stands_is_the_text_now_where_the_units_were() {
    let mut units = Units::of("Ann ran. Bob sat. Cy hid. Di swam.", Grain::Sentence);
    let (_, u) = shown_rows(&units);
    let refs: Vec<UnitRef> = u.iter().map(|unit| unit.reference).collect();
    assert_eq!(
        units.stands(&[refs[1], refs[2]]).as_deref(),
        Some("Bob sat. Cy hid.")
    );
    let mut change = nothing();
    change.replace.push((u[1].clone(), "Bob stood.".to_owned()));
    change.insert.push((after(&u[1]), "Between.".to_owned()));
    change.remove.push(u[3].clone());
    applied(&mut units, change);
    assert_eq!(
        units.stands(&[refs[1], refs[2]]).as_deref(),
        Some("Bob stood. Between. Cy hid.")
    );
    assert_eq!(units.stands(&[refs[2]]).as_deref(), Some("Cy hid."));
    assert_eq!(units.stands(&[refs[0]]).as_deref(), Some("Ann ran."));
    assert_eq!(units.stands(&[refs[3]]), None);
    assert_eq!(units.stands(&[]), None);
}

#[test]
fn stands_follows_a_moved_unit_to_where_it_now_is() {
    let mut units = Units::of("Ann ran. Bob sat. Cy hid.", Grain::Sentence);
    let (_, u) = shown_rows(&units);
    let mut change = nothing();
    change.moved.push((u[0].clone(), after(&u[2])));
    applied(&mut units, change);
    assert_eq!(units.text(), "Bob sat. Cy hid. Ann ran.");
    assert_eq!(units.stands(&[u[0].reference]).as_deref(), Some("Ann ran."));
}

#[test]
fn new_content_that_begins_with_a_line_break_starts_a_paragraph() {
    let text = "Ann ran. Bob sat.";
    let inserted = edited(text, Grain::Sentence, |u, change| {
        change
            .insert
            .push((after(&u[0]), "\n\nNew paragraph.".to_owned()));
    });
    assert_eq!(inserted, "Ann ran.\n\nNew paragraph. Bob sat.");
    let crlf = edited(text, Grain::Sentence, |u, change| {
        change
            .insert
            .push((after(&u[0]), "\r\nNew line.".to_owned()));
    });
    assert_eq!(crlf, "Ann ran.\r\nNew line. Bob sat.");
}

#[test]
fn new_content_that_ends_with_a_line_break_is_followed_by_no_other_break() {
    let inserted = edited("Ann ran. Bob sat.", Grain::Sentence, |u, change| {
        change.insert.push((after(&u[0]), "Cy hid.\n".to_owned()));
    });
    assert_eq!(inserted, "Ann ran. Cy hid.\nBob sat.");
}

#[test]
fn a_replacement_that_begins_with_a_blank_line_does_not_double_the_break_before_it() {
    let replaced = edited("Ann ran.\n\nBob sat.", Grain::Sentence, |u, change| {
        change
            .replace
            .push((u[1].clone(), "\n\nCy hid.".to_owned()));
    });
    assert_eq!(replaced, "Ann ran.\n\nCy hid.");
}

#[test]
fn new_content_without_line_breaks_joins_as_before() {
    let inserted = edited("Ann ran.\n\nBob sat.", Grain::Sentence, |u, change| {
        change.insert.push((after(&u[0]), "Cy hid.".to_owned()));
    });
    assert_eq!(inserted, "Ann ran. Cy hid.\n\nBob sat.");
}

#[test]
fn a_rewrite_makes_the_text_whole_over_earlier_changes() {
    let mut units = Units::of("Ann ran. Bob sat. Cy hid.", Grain::Sentence);
    units.rewrite("Dee sang.");
    assert_eq!(units.text(), "Dee sang.");
    assert!(units.changed());
    units.rewrite("");
    assert_eq!(units.text(), "");
    let mut empty = Units::of("", Grain::Sentence);
    empty.rewrite("Dee sang.");
    assert_eq!(empty.text(), "Dee sang.");
}

#[test]
fn a_unit_is_found_in_the_text_that_holds_it_with_what_the_text_is_a_text_of() {
    let mut texts = Texts::new();
    texts.push("first", "/a", "Ann ran. Bob sat.", Grain::Sentence);
    texts.push("second", "/b", "Cy hid. Dee sang.", Grain::Sentence);
    let unit = texts
        .iter()
        .nth(1)
        .and_then(|(_, units)| units.units().get(1))
        .map(|unit| unit.at)
        .expect("the second text has two units");
    assert_eq!(texts.found(unit), Some((&"second", "Dee sang.")));
}

#[test]
fn two_equal_texts_at_two_places_are_two_things() {
    let mut texts = Texts::new();
    texts.push("one", "/a", "Ann ran.", Grain::Sentence);
    texts.push("two", "/b", "Ann ran.", Grain::Sentence);
    let refs: Vec<UnitRef> = texts
        .iter()
        .filter_map(|(_, units)| units.units().first().map(|unit| unit.at))
        .collect();
    assert_eq!(refs.len(), 2);
    assert_ne!(refs.first(), refs.get(1));
    let found: Vec<Option<(&&str, &str)>> = refs.iter().map(|at| texts.found(*at)).collect();
    assert_eq!(
        found,
        vec![Some((&"one", "Ann ran.")), Some((&"two", "Ann ran."))]
    );
}

#[test]
fn a_unit_of_a_text_that_was_not_pushed_is_not_found() {
    let mut texts = Texts::new();
    texts.push("one", "/a", "Ann ran.", Grain::Sentence);
    let other = Units::of_at("Bob sat.", Grain::Sentence, "/b");
    let unit = other.units().first().map(|unit| unit.at).expect("a unit");
    assert_eq!(texts.found(unit), None);
    assert_eq!(Texts::<&str>::default().found(unit), None);
}

#[test]
fn iter_keeps_the_order_the_texts_were_pushed() {
    let mut texts = Texts::new();
    texts.push(3, "/c", "Cy hid.", Grain::Sentence);
    texts.push(1, "/a", "Ann ran.", Grain::Sentence);
    texts.push(2, "/b", "Bob sat.", Grain::Sentence);
    let order: Vec<i32> = texts.iter().map(|(of, _)| *of).collect();
    assert_eq!(order, vec![3, 1, 2]);
}

#[test]
fn a_text_shown_to_be_quoted_is_named_apart_from_the_material() {
    let material = Units::of("Ann ran. Bob sat.", Grain::Sentence);
    let source = Units::of("Cy hid. Di won.", Grain::Sentence);
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let own = material.rows(&material.shown(&mut showing).expect("shown"));
    let quotable = source.quoted();
    let quoted = source.rows(&showing.show(&quotable).expect("shown"));
    assert_eq!(own, ["[u1] Ann ran.", "[u2] Bob sat."].join("\n"));
    assert_eq!(quoted, ["[q1] Cy hid.", "[q2] Di won."].join("\n"));

    assert_eq!(showing.resolve::<Unit>("q1"), None);
    assert_eq!(showing.resolve::<Quoted>("u1"), None);
    let picked = showing.resolve::<Quoted>("q2").expect("resolves");
    assert_eq!(source.text_of(picked.reference), Some("Di won."));
}

#[test]
fn a_quoted_unit_that_is_not_shown_is_written_without_a_handle() {
    let source = Units::of("Cy hid.\n\nDi won. Ed lost.", Grain::Sentence);
    let chosen: Vec<Quoted> = source
        .quoted()
        .into_iter()
        .filter(|quoted| quoted.text() != "Di won.")
        .collect();
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = showing.show(&chosen).expect("shown");
    assert_eq!(
        source.marked(&shown),
        "[q1] Cy hid.\n\nDi won. [q2] Ed lost."
    );
}
