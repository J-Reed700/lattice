use super::super::shown::Drawn;
use super::*;
use proptest::prelude::*;
use serde_json::json;

struct Source(u32);
struct Picture(u32);
struct File(u32);
struct Figure(u32);

macro_rules! numbered_kind {
    ($kind:ident, $prefix:expr, $drawn:expr) => {
        impl Kind for $kind {
            const PREFIX: &'static str = $prefix;
            const DRAWN: Drawn = $drawn;
            type Ref = u32;

            fn reference(&self) -> u32 {
                self.0
            }
        }
    };
}

numbered_kind!(Source, "s", Drawn::Number);
numbered_kind!(Picture, "i", Drawn::Itself);
numbered_kind!(File, "f", Drawn::Name);
numbered_kind!(Figure, "fig", Drawn::Itself);

/// Twelve sources, three pictures, two files and two figures are shown; nothing else resolves.
fn scope() -> Scope {
    let sources: Vec<Source> = (1..=12).map(Source).collect();
    let pictures: Vec<Picture> = (1..=3).map(Picture).collect();
    let files: Vec<File> = (1..=2).map(File).collect();
    let figures: Vec<Figure> = (1..=2).map(Figure).collect();
    let mut scope = Scope::for_question();
    {
        let mut showing = scope.request();
        showing.show(&sources).unwrap();
        showing.show(&pictures).unwrap();
        showing.show(&files).unwrap();
        showing.show(&figures).unwrap();
    }
    scope
}

fn markers() -> Markers {
    Markers::new()
        .of::<Source>(Place::Inline)
        .of::<Picture>(Place::OwnLine)
        .of_lines::<File>(Place::Inline)
        .of::<Figure>(Place::Inline)
}

fn text(text: &str) -> Piece {
    Piece::Text(text.to_owned())
}

fn reference(prefix: &'static str, number: u32, drawn: Drawn, lines: Option<(u32, u32)>) -> Piece {
    Piece::Reference(Reference {
        prefix,
        number,
        reference: json!(number),
        drawn,
        lines: lines.map(|(first, last)| Lines { first, last }),
    })
}

fn source(number: u32) -> Piece {
    reference("s", number, Drawn::Number, None)
}

fn picture(number: u32) -> Piece {
    reference("i", number, Drawn::Itself, None)
}

fn figure(number: u32) -> Piece {
    reference("fig", number, Drawn::Itself, None)
}

fn file(number: u32, first: u32, last: u32) -> Piece {
    reference("f", number, Drawn::Name, Some((first, last)))
}

fn merge(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut merged: Vec<Piece> = Vec::new();
    for piece in pieces {
        if let (Piece::Text(next), Some(Piece::Text(last))) = (&piece, merged.last_mut()) {
            last.push_str(next);
            continue;
        }
        merged.push(piece);
    }
    merged
}

fn whole(scope: &Scope, input: &str) -> Vec<Piece> {
    merge(split(scope, markers(), input))
}

/// The pieces of `chunks` pushed one after another, adjacent text merged. Within one push
/// no text is empty and no two text pieces are adjacent.
fn chunked(scope: &Scope, chunks: &[&str]) -> Vec<Piece> {
    let mut splitter = Splitter::new(scope, markers());
    let mut all = Vec::new();
    for chunk in chunks {
        let pieces = splitter.push(chunk);
        assert_well_formed(&pieces);
        all.extend(pieces);
    }
    let rest = splitter.finish();
    assert_well_formed(&rest);
    all.extend(rest);
    merge(all)
}

fn assert_well_formed(pieces: &[Piece]) {
    for piece in pieces {
        if let Piece::Text(text) = piece {
            assert!(!text.is_empty(), "empty text piece in {pieces:?}");
        }
    }
    for pair in pieces.windows(2) {
        assert!(
            !matches!(pair, [Piece::Text(_), Piece::Text(_)]),
            "adjacent text pieces in {pieces:?}"
        );
    }
}

/// The input split whole, in two at every char boundary and one char at a time, must all give `expected`.
fn check(input: &str, expected: Vec<Piece>) {
    let scope = scope();
    assert_eq!(whole(&scope, input), expected, "whole: {input:?}");
    for (index, _) in input
        .char_indices()
        .chain(std::iter::once((input.len(), ' ')))
    {
        let (head, tail) = input.split_at(index);
        assert_eq!(
            chunked(&scope, &[head, tail]),
            expected,
            "cut at {index}: {input:?}"
        );
    }
    let singles: Vec<&str> = input.split_inclusive(|_| true).collect();
    assert_eq!(chunked(&scope, &singles), expected, "by char: {input:?}");
    let pieces = whole(&scope, input);
    assert_eq!(
        whole(&scope, &written(&pieces)),
        pieces,
        "written back: {input:?}"
    );
}

/// The input is the reply's own text, unchanged.
fn check_text(input: &str) {
    check(input, vec![text(input)]);
}

#[test]
fn a_marker_inline() {
    check(
        "See [s2] now.",
        vec![text("See "), source(2), text(" now.")],
    );
}

#[test]
fn a_marker_at_the_start_and_at_the_end() {
    check("[s1] and [s3]", vec![source(1), text(" and "), source(3)]);
    check("[s1]", vec![source(1)]);
    check("end [s1]", vec![text("end "), source(1)]);
}

#[test]
fn adjacent_markers() {
    check("[s1][s2]", vec![source(1), source(2)]);
    check(
        "[s1][s2][s3]x",
        vec![source(1), source(2), source(3), text("x")],
    );
}

#[test]
fn a_marker_carries_the_number_of_its_handle() {
    let scope = scope();
    let pieces = whole(&scope, "[s12]");
    let [Piece::Reference(found)] = pieces.as_slice() else {
        panic!("not one reference: {pieces:?}");
    };
    assert_eq!(found.number, 12);
    assert_eq!(found.prefix, "s");
    assert_eq!(found.reference, json!(12));
    assert_eq!(found.drawn, Drawn::Number);
    assert_eq!(found.lines, None);
}

#[test]
fn a_prefix_of_several_letters() {
    check("a [fig2] b", vec![text("a "), figure(2), text(" b")]);
    check_text("[fi2]");
    check_text("[figs2]");
}

#[test]
fn unknown_handles_stay_text() {
    check_text("[s99]");
    check_text("[s13]");
    check_text("[f3:1-2]");
    check_text("[i4]");
}

#[test]
fn undeclared_kinds_stay_text() {
    check_text("[x1]");
    check_text("[sx1]");
    check_text("[S1]");
}

#[test]
fn malformed_candidates_stay_text() {
    for input in [
        "[s0]",
        "[s01]",
        "[s]",
        "[1]",
        "[]",
        "[ s1]",
        "[s1 ]",
        "[s 1]",
        "[s1, s2]",
        "[s1,s2]",
        "[s1",
        "s1]",
        "[s4294967296]",
        "[s1:2]",
        "[s1:2-3]",
        "[f1:]",
        "[f1:05]",
        "[f1:5-]",
        "[f1:-5]",
        "[f1:5-3]",
        "[f1:5-6-7]",
        "[f1:a]",
        "[f1:5 - 6]",
        "[é1]",
        "[s1é]",
        "[s1\n]",
    ] {
        check_text(input);
    }
}

#[test]
fn a_bracket_inside_a_candidate_starts_a_new_one() {
    check("[[s1]]", vec![text("["), source(1), text("]")]);
    check("[s[s2]", vec![text("[s"), source(2)]);
    check("[s1[s2]", vec![text("[s1"), source(2)]);
}

#[test]
fn a_range_on_a_kind_with_lines() {
    check("[f1:10-20]", vec![file(1, 10, 20)]);
    check(
        "see [f2:7] there",
        vec![text("see "), file(2, 7, 7), text(" there")],
    );
    check("[f2:10-10]", vec![file(2, 10, 10)]);
    check(
        "[f1:4294967295-4294967295]",
        vec![file(1, u32::MAX, u32::MAX)],
    );
}

#[test]
fn a_kind_with_lines_also_reads_without_them() {
    check("[f2]", vec![reference("f", 2, Drawn::Name, None)]);
}

#[test]
fn written_gives_each_form_back() {
    let pieces = vec![
        text("a "),
        source(12),
        file(2, 10, 20),
        file(2, 10, 10),
        reference("f", 1, Drawn::Name, None),
        figure(1),
        text(" z"),
    ];
    assert_eq!(written(&pieces), "a [s12][f2:10-20][f2:10][f1][fig1] z");
    assert_eq!(written(&[]), "");
}

#[test]
fn an_own_line_marker_counts_alone_on_its_line() {
    check("[i1]", vec![picture(1)]);
    check(
        "before\n[i1]\nafter",
        vec![text("before\n"), picture(1), text("\nafter")],
    );
    check("[i1]\n", vec![picture(1), text("\n")]);
    check(
        "  [i1]  \t\nx",
        vec![text("  "), picture(1), text("  \t\nx")],
    );
    check("a\n\t[i2]", vec![text("a\n\t"), picture(2)]);
    check("a\n[i3]   ", vec![text("a\n"), picture(3), text("   ")]);
}

#[test]
fn an_own_line_marker_counts_on_lines_that_end_with_a_carriage_return() {
    check(
        "before\r\n[i1]\r\nafter",
        vec![text("before\r\n"), picture(1), text("\r\nafter")],
    );
    check("`open\r\n\r\n[s1]", vec![text("`open\r\n\r\n"), source(1)]);
}

#[test]
fn an_own_line_marker_with_text_around_it_stays_text() {
    check_text("see [i1]");
    check_text("[i1] after");
    check_text("see [i1] after");
    check_text("[i1][i2]");
    check_text("a\n[i1] b\nc");
    check_text("[i1] [i2]");
}

#[test]
fn an_inline_marker_does_not_make_an_own_line_marker_after_it_count() {
    check("[s1] [i1]", vec![source(1), text(" [i1]")]);
}

#[test]
fn a_fenced_block_holds_no_markers() {
    check(
        "```\n[s1]\n```\n[s2]",
        vec![text("```\n[s1]\n```\n"), source(2)],
    );
    check(
        "```rust\n[s1]\n```\n[s2]",
        vec![text("```rust\n[s1]\n```\n"), source(2)],
    );
    check(
        "a\n~~~\n[s1]\n~~~\n[s2]",
        vec![text("a\n~~~\n[s1]\n~~~\n"), source(2)],
    );
    check(
        "```\n[i1]\n```\n[i2]",
        vec![text("```\n[i1]\n```\n"), picture(2)],
    );
}

#[test]
fn a_fence_closes_on_a_run_at_least_as_long() {
    check(
        "```\n[s1]\n`````\n[s2]",
        vec![text("```\n[s1]\n`````\n"), source(2)],
    );
    check(
        "````\n```\n[s1]\n````\n[s2]",
        vec![text("````\n```\n[s1]\n````\n"), source(2)],
    );
    check(
        "~~~\n```\n[s1]\n~~~~\n[s2]",
        vec![text("~~~\n```\n[s1]\n~~~~\n"), source(2)],
    );
}

#[test]
fn an_unclosed_fence_runs_to_the_end() {
    check_text("```\n[s1]\n[s2]");
    check_text("~~~\n[s1]\n``` \n[s2]");
    check_text("```");
}

#[test]
fn an_indented_fence_counts() {
    check(
        "  ```\n[s1]\n  ```\n[s2]",
        vec![text("  ```\n[s1]\n  ```\n"), source(2)],
    );
}

#[test]
fn a_marker_before_a_fence_still_counts() {
    check(
        "[s1]\n```\n[s2]\n```",
        vec![source(1), text("\n```\n[s2]\n```")],
    );
}

#[test]
fn an_inline_span_holds_no_markers() {
    check("`[s1]` [s2]", vec![text("`[s1]` "), source(2)]);
    check(
        "[s1] `[s2]` [s3]",
        vec![source(1), text(" `[s2]` "), source(3)],
    );
    check(
        "``[s1] ` [s1]`` [s2]",
        vec![text("``[s1] ` [s1]`` "), source(2)],
    );
    check("`a` `[s1]` `b`", vec![text("`a` `[s1]` `b`")]);
}

#[test]
fn an_unclosed_span_runs_to_the_end_of_its_paragraph() {
    check_text("a `[s1] more\n[s2]");
    check_text("``a` [s1]");
    check("a `[s1]\n\n[s2]", vec![text("a `[s1]\n\n"), source(2)]);
    check(
        "a `[s1]\n  \t\n[s2]",
        vec![text("a `[s1]\n  \t\n"), source(2)],
    );
}

#[test]
fn a_span_that_closes_lets_later_markers_count() {
    check("`a` [s1]", vec![text("`a` "), source(1)]);
    check("``a`` [s1]", vec![text("``a`` "), source(1)]);
}

#[test]
fn a_run_of_tildes_that_is_not_a_fence_is_text() {
    check("a ~~~ [s1]", vec![text("a ~~~ "), source(1)]);
    check("~~ [s1]", vec![text("~~ "), source(1)]);
}

#[test]
fn text_with_multibyte_characters_around_markers() {
    check(
        "é [s1] ü\n[i2]\n日本",
        vec![
            text("é "),
            source(1),
            text(" ü\n"),
            picture(2),
            text("\n日本"),
        ],
    );
}

#[test]
fn an_empty_reply_has_no_pieces() {
    check("", vec![]);
}

#[test]
fn text_that_cannot_be_a_marker_is_released_at_once() {
    let scope = scope();
    let mut splitter = Splitter::new(&scope, markers());
    assert_eq!(splitter.push("hello world "), vec![text("hello world ")]);
    assert_eq!(splitter.push("[s"), vec![]);
    assert_eq!(splitter.push("x"), vec![text("[sx")]);
    assert_eq!(splitter.push("[s1]"), vec![source(1)]);
    assert_eq!(splitter.push("[s"), vec![]);
    assert_eq!(splitter.finish(), vec![text("[s")]);
}

#[test]
fn an_own_line_candidate_is_held_until_its_line_ends() {
    let scope = scope();
    let mut splitter = Splitter::new(&scope, markers());
    assert_eq!(splitter.push("[i1]"), vec![]);
    assert_eq!(splitter.push("  "), vec![]);
    assert_eq!(splitter.push("\nnext"), vec![picture(1), text("  \nnext")]);
    assert_eq!(splitter.finish(), vec![]);
}

#[test]
fn a_run_of_backticks_is_held_until_its_length_is_known() {
    let scope = scope();
    let mut splitter = Splitter::new(&scope, markers());
    assert_eq!(splitter.push("a `"), vec![text("a ")]);
    assert_eq!(splitter.push("`"), vec![]);
    assert_eq!(splitter.push("b [s1]"), vec![text("``b [s1]")]);
    assert_eq!(splitter.finish(), vec![]);
}

const TOKENS: &[&str] = &[
    "[", "]", "[", "]", "`", "``", "```", "````", "~~~", "~", "\n", "\n", "\n\n", " ", "  ", "\t",
    "s", "i", "f", "fig", "x", "0", "1", "2", "3", "12", "99", ":", "-", ",", "a", "é", "[s1]",
    "[s12]", "[i1]", "[i2]", "[f1:2-3]", "[f2:4]", "[fig1]", "```\n", "~~~\n", "text ",
];

fn replies() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(TOKENS.to_vec()), 0..40)
        .prop_map(|tokens| tokens.concat())
}

/// Cut points as char boundaries of `input`, ascending, each at most once.
fn chunks_of(input: &str, cuts: &[usize]) -> Vec<String> {
    let mut boundaries: Vec<usize> = input.char_indices().map(|(index, _)| index).collect();
    boundaries.push(input.len());
    let mut at: Vec<usize> = cuts
        .iter()
        .filter_map(|cut| boundaries.get(cut % boundaries.len()).copied())
        .collect();
    at.sort_unstable();
    at.dedup();
    let mut chunks = Vec::new();
    let mut start = 0;
    for end in at {
        chunks.push(input[start..end].to_owned());
        start = end;
    }
    chunks.push(input[start..].to_owned());
    chunks
}

proptest! {
    #[test]
    fn chunking_never_matters(
        input in replies(),
        cuts in prop::collection::vec(any::<usize>(), 0..10),
    ) {
        let scope = scope();
        let chunks = chunks_of(&input, &cuts);
        let refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
        prop_assert_eq!(chunked(&scope, &refs), whole(&scope, &input));
    }

    #[test]
    fn nothing_is_lost(input in replies()) {
        let scope = scope();
        let pieces = whole(&scope, &input);
        prop_assert_eq!(whole(&scope, &written(&pieces)), pieces.clone());
        let plain = pieces.iter().all(|piece| matches!(piece, Piece::Text(_)));
        if plain {
            prop_assert_eq!(written(&pieces), input);
        }
    }

}
