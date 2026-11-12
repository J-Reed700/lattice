//! The boundary table, the unit tests of each part, and the invariants of
//! the layer as property tests.

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use super::golden::{APP_TEXT, GOLDEN_RULES, KNOWN_MISSES};
use super::*;

fn pieces(text: &str) -> Vec<&str> {
    sentences(text)
        .into_iter()
        .map(|(start, end)| &text[start..end])
        .collect()
}

#[test]
fn flowed_sentences_are_exact_slices_when_a_sentence_wraps_over_a_crlf() {
    let text = "Thresholds with\r\nabstention for anything uncertain. Watch for drift\r\nas new types arrive.\r\n";
    let spans = flowed_sentences(text);
    let found: Vec<&str> = spans
        .iter()
        .map(|&(start, end)| &text[start..end])
        .collect();
    assert_eq!(
        found,
        [
            "Thresholds with\r\nabstention for anything uncertain.",
            "Watch for drift\r\nas new types arrive."
        ]
    );
    assert_eq!(spans.first(), Some(&(0, 51)));
}

#[test]
fn every_row_of_the_boundary_table() {
    let misses: Vec<_> = GOLDEN_RULES
        .iter()
        .filter(|row| row.golden.is_some())
        .collect();
    assert_eq!(misses.len(), KNOWN_MISSES);
    for row in misses {
        assert_ne!(row.golden, Some(row.ours), "{}", row.name);
    }
    let mut wrong = Vec::new();
    for row in GOLDEN_RULES.iter().chain(APP_TEXT) {
        let ours = pieces(row.text);
        if ours != row.ours {
            wrong.push(format!(
                "{}: {:?}\n    got  {:?}\n    want {:?}",
                row.name, row.text, ours, row.ours
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} rows differ from the table:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// The breaks `(the text the piece after one starts with, strength)` names,
/// each found in `text` after the one before it.
fn expected(text: &str, wanted: &[(&str, Strength)]) -> Vec<Break> {
    let mut from = 0;
    wanted
        .iter()
        .map(|&(starts, strength)| {
            let at = from + text[from..].find(starts).expect("the text has the piece");
            from = at + starts.len();
            Break { at, strength }
        })
        .collect()
}

fn at_least(breaks: Vec<Break>, strength: Strength) -> Vec<Break> {
    breaks
        .into_iter()
        .filter(|cut| cut.strength >= strength)
        .collect()
}

#[test]
fn prose_strengths_come_from_the_whitespace_before_a_break() {
    let text = "One. Two\nThree\n\nFour\n\n## Five\nSix, seven; eight — nine";
    assert_eq!(
        at_least(Prose.breaks(text), Strength::Clause),
        expected(
            text,
            &[
                ("Two", Strength::Sentence),
                ("Three", Strength::Line),
                ("Four", Strength::Paragraph),
                ("## Five", Strength::Section),
                ("Six", Strength::Line),
                ("seven", Strength::Clause),
                ("eight", Strength::Clause),
                ("nine", Strength::Clause),
            ]
        )
    );
}

#[test]
fn prose_cuts_a_fence_between_its_lines_only() {
    let text = "Intro.\n```\nlet a = 1. B\n\nlet c = 2\n```\nEnd.";
    assert_eq!(
        at_least(Prose.breaks(text), Strength::Clause),
        expected(
            text,
            &[
                ("```", Strength::Line),
                ("let a", Strength::Line),
                ("let c", Strength::Paragraph),
                ("```", Strength::Line),
                ("End.", Strength::Line),
            ]
        )
    );
    let before_b = text.find("B\n").expect("the text has B");
    let weaker = Prose
        .breaks(text)
        .into_iter()
        .filter(|cut| cut.at == before_b)
        .all(|cut| cut.strength <= Strength::Word);
    assert!(weaker);
}

#[test]
fn prose_words_break_at_the_end_of_each_whitespace_run() {
    let text = "a bb  ccc\tdd";
    assert_eq!(
        Prose.breaks(text),
        expected(
            text,
            &[
                ("bb", Strength::Word),
                ("ccc", Strength::Word),
                ("dd", Strength::Word),
            ]
        )
    );
}

#[test]
fn code_breaks_before_items_and_after_blank_lines() {
    let text = "use a;\n\nfn one() {}\n\n/// Doc.\nfn two() {}\nlet x = 1;\n\nlet y = 2;\n";
    assert_eq!(
        at_least(Code.breaks(text), Strength::Line),
        expected(
            text,
            &[
                ("fn one", Strength::Section),
                ("/// Doc.", Strength::Section),
                ("fn two", Strength::Line),
                ("let x", Strength::Line),
                ("let y", Strength::Paragraph),
            ]
        )
    );
}

#[test]
fn bytes_reach_the_last_char_boundary_that_fits() {
    assert_eq!(Bytes.reach("abc", 0, 2), 2);
    assert_eq!(Bytes.reach("時a", 0, 2), 0);
    assert_eq!(Bytes.reach("時a", 0, 3), 3);
    assert_eq!(Bytes.reach("時a", 0, 4), 4);
}

#[test]
fn chars_count_scalar_values() {
    assert_eq!(Chars.reach("時時a", 0, 2), 6);
    for (text, start) in [("abc", 1), ("時時a", 3), ("", 0)] {
        assert_eq!(Chars.reach(text, start, usize::MAX), text.len());
    }
}

#[test]
fn words_count_where_they_start_and_keep_the_space_after() {
    assert_eq!(Words.reach("  one", 0, 0), 2);
    assert_eq!(Words.reach("one two  three", 0, 2), 9);
    assert_eq!(Words.reach("one two", 0, 5), 7);
}

/// A boundary for the packers' tests that no rule of `Prose` gets in the way
/// of: a Sentence break after every full stop and whitespace, a Word break
/// after every other whitespace run.
struct Stops;

impl Boundaries for Stops {
    fn breaks(&self, text: &str) -> Vec<Break> {
        let mut breaks = Vec::new();
        let mut before = None;
        let mut in_space = false;
        for (at, c) in text.char_indices() {
            if c.is_whitespace() {
                in_space = true;
                continue;
            }
            if in_space {
                let strength = if before == Some('.') {
                    Strength::Sentence
                } else {
                    Strength::Word
                };
                breaks.push(Break { at, strength });
            }
            in_space = false;
            before = Some(c);
        }
        breaks
    }
}

fn rung(at_least: Strength, floor: f64) -> Rung {
    Rung { at_least, floor }
}

fn parts<'a, M: Measure>(
    text: &'a str,
    measure: &M,
    ladder: &[Rung],
    max: usize,
    overflow: Overflow,
) -> Vec<&'a str> {
    let plan = Plan {
        max,
        ladder,
        overflow,
    };
    partition(text, &Stops, measure, &plan)
        .into_iter()
        .map(|span| &text[span])
        .collect()
}

#[test]
fn a_piece_ends_at_the_latest_break_of_the_first_rung_that_finds_one() {
    let text = "aaaa. bbbb. cccc dddd eeee";
    let sentence = [rung(Strength::Sentence, 0.0)];
    assert_eq!(
        parts(text, &Chars, &sentence, 12, Overflow::Cut),
        ["aaaa. bbbb. ", "cccc dddd ee", "ee"]
    );
    let then_word = [rung(Strength::Sentence, 0.0), rung(Strength::Word, 0.0)];
    assert_eq!(
        parts(text, &Chars, &then_word, 12, Overflow::Cut),
        ["aaaa. bbbb. ", "cccc dddd ", "eeee"]
    );
    assert_eq!(
        parts(text, &Chars, &sentence, 12, Overflow::Extend),
        ["aaaa. bbbb. ", "cccc dddd eeee"]
    );
}

#[test]
fn a_rung_only_counts_breaks_past_its_floor() {
    let text = "aa. bbbbbbb cc dd";
    let floored = [rung(Strength::Sentence, 0.5), rung(Strength::Word, 0.0)];
    assert_eq!(
        parts(text, &Chars, &floored, 14, Overflow::Cut),
        ["aa. bbbbbbb ", "cc dd"]
    );
    let unfloored = [rung(Strength::Sentence, 0.0), rung(Strength::Word, 0.0)];
    assert_eq!(
        parts(text, &Chars, &unfloored, 14, Overflow::Cut),
        ["aa. ", "bbbbbbb cc dd"]
    );
}

#[test]
fn a_char_too_wide_for_the_budget_is_a_piece_of_its_own() {
    assert_eq!(
        parts("時時時時時", &Bytes, &[], 7, Overflow::Cut),
        ["時時", "時時", "時"]
    );
    assert_eq!(
        parts("時時時時時", &Bytes, &[], 2, Overflow::Cut),
        ["時", "時", "時", "時", "時"]
    );
}

#[test]
fn a_char_rung_cuts_at_the_budget() {
    let char_rung = [rung(Strength::Char, 0.0)];
    assert_eq!(
        parts("aaaa bbbb", &Chars, &char_rung, 6, Overflow::Cut),
        ["aaaa b", "bbb"]
    );
}

#[test]
fn units_cut_at_every_char_for_the_char_strength() {
    assert_eq!(units("a時b", &Prose, Strength::Char), [0..1, 1..4, 4..5]);
}

#[test]
fn windows_overlap_by_whole_sentences() {
    let text = "One two. Three four. Five six. Seven.";
    let plan = Plan {
        max: 25,
        ladder: &[rung(Strength::Sentence, 0.0)],
        overflow: Overflow::Cut,
    };
    let found: Vec<&str> = windows(text, &Prose, &Chars, &plan, 10)
        .into_iter()
        .map(|span| &text[span])
        .collect();
    assert_eq!(
        found,
        [
            "One two. Three four.",
            "Three four. Five six.",
            "Five six. Seven."
        ]
    );
}

#[test]
fn windows_start_at_the_strongest_rung_that_finds_a_break_before_the_step() {
    let text = "Aa bb. Cc dd ee ff gg hh. Ii jj.";
    let plan = Plan {
        max: 30,
        ladder: &[rung(Strength::Sentence, 0.0), rung(Strength::Word, 0.0)],
        overflow: Overflow::Cut,
    };
    assert_eq!(windows(text, &Prose, &Chars, &plan, 12), vec![0..25, 7..32]);
}

#[test]
fn a_shorter_run_inside_a_fence_keeps_its_lines_code() {
    let text = "Intro.\n\n````md\n```\nC. D.\n```\n````\n";
    let at = text.find("D.").unwrap();
    assert!(sentences(text).iter().all(|&(start, _)| start != at));
}

#[test]
fn the_next_window_starts_no_later_than_the_end_of_a_cut_made_by_a_stronger_rung() {
    let text = "aaa\n\nbbb ccc ddd eee fff ggg";
    let plan = Plan {
        max: 20,
        ladder: &[rung(Strength::Paragraph, 0.0), rung(Strength::Word, 0.0)],
        overflow: Overflow::Cut,
    };
    let found: Vec<&str> = windows(text, &Prose, &Chars, &plan, 15)
        .into_iter()
        .map(|span| &text[span])
        .collect();
    assert_eq!(found, ["aaa", "bbb ccc ddd eee fff", "eee fff ggg"]);
}

#[test]
fn an_emphasis_mark_opening_the_next_sentence_keeps_the_break_before_it() {
    assert_eq!(pieces("Done. **Bold** next."), ["Done.", "**Bold** next."]);
}

#[test]
fn a_sentence_goes_on_after_a_closing_emphasis_mark_too() {
    assert_eq!(
        pieces("**Stop!** she said. Then left."),
        ["**Stop!** she said.", "Then left."]
    );
}

#[test]
fn a_stop_inside_a_path_ends_no_sentence() {
    assert_eq!(
        pieces("The entry ../Name names the place above, and it is one sentence. The next one follows."),
        [
            "The entry ../Name names the place above, and it is one sentence.",
            "The next one follows."
        ]
    );
}

#[test]
fn a_stop_inside_a_file_name_ends_no_sentence() {
    assert_eq!(
        pieces("Open notes.Txt and read it. Then close it."),
        ["Open notes.Txt and read it.", "Then close it."]
    );
}

#[test]
fn a_question_mark_inside_an_address_ends_no_sentence() {
    assert_eq!(
        pieces("Go to page?Mode=Full to switch. Then return."),
        ["Go to page?Mode=Full to switch.", "Then return."]
    );
}

#[test]
fn a_terminator_with_no_space_after_it_stays_in_one_sentence() {
    assert_eq!(pieces("Is it ready?Yes"), ["Is it ready?Yes"]);
    assert_eq!(pieces("Stop!Now"), ["Stop!Now"]);
}

#[test]
fn a_stop_and_a_space_still_end_a_sentence() {
    assert_eq!(
        pieces("One ends here. Two starts."),
        ["One ends here.", "Two starts."]
    );
    assert_eq!(pieces("One. Two."), ["One.", "Two."]);
}

#[test]
fn a_closing_quote_or_emphasis_after_a_stop_and_a_space_still_ends_a_sentence() {
    assert_eq!(
        pieces("She said \"stop.\" Then she left."),
        ["She said \"stop.\"", "Then she left."]
    );
    assert_eq!(
        pieces("It is **done.** Next one."),
        ["It is **done.**", "Next one."]
    );
}

#[test]
fn a_full_width_stop_ends_a_sentence_without_a_space() {
    assert_eq!(
        pieces("これは一つ目です。これは二つ目です。"),
        ["これは一つ目です。", "これは二つ目です。"]
    );
}

const WORDS: [&str; 9] = [
    "alpha",
    "Beta",
    "γάμμα",
    "時",
    "x1",
    "3.14",
    "file.rs",
    "U.S.",
    "Dr.",
];
const SPACES: [&str; 6] = [" ", "  ", "\n", "\n\n", "\r\n", "\t"];
const MARKS: [&str; 16] = [
    ".", "!", "?", "…", "。", ",", ";", ":", "—", "\"", "'", ")", "]", "**", "_", "`",
];
const MARKDOWN: [&str; 7] = ["# ", "## ", "- ", "1. ", "| ", "```\n", "$x!$"];
const LADDER: [Strength; 6] = [
    Strength::Section,
    Strength::Paragraph,
    Strength::Line,
    Strength::Sentence,
    Strength::Clause,
    Strength::Word,
];
const FLOORS: [f64; 4] = [0.0, 0.25, 0.5, 0.8];
const MEASURES: [(&str, &dyn Measure); 3] =
    [("bytes", &Bytes), ("chars", &Chars), ("words", &Words)];

pub(super) fn texts() -> impl Strategy<Value = String> {
    let token = prop_oneof![
        3 => prop::sample::select(WORDS.to_vec()),
        3 => prop::sample::select(SPACES.to_vec()),
        2 => prop::sample::select(MARKS.to_vec()),
        1 => prop::sample::select(MARKDOWN.to_vec()),
    ];
    prop::collection::vec(token, 0..120).prop_map(|tokens| tokens.concat())
}

/// A random plan: its budget, a strictly descending ladder, what to do on
/// overflow, and a window step within the budget.
#[derive(Clone, Debug)]
struct Shape {
    max: usize,
    rungs: Vec<Rung>,
    extend: bool,
    step: usize,
}

impl Shape {
    fn plan(&self) -> Plan<'_> {
        Plan {
            max: self.max,
            ladder: &self.rungs,
            overflow: if self.extend {
                Overflow::Extend
            } else {
                Overflow::Cut
            },
        }
    }
}

fn shapes() -> impl Strategy<Value = Shape> {
    (
        1usize..200,
        prop::collection::vec(any::<bool>(), LADDER.len()),
        prop::collection::vec(0..FLOORS.len(), LADDER.len()),
        any::<bool>(),
        0usize..1000,
    )
        .prop_map(|(max, keep, floors, extend, step)| {
            let mut rungs: Vec<Rung> = LADDER
                .iter()
                .zip(keep)
                .zip(floors)
                .filter(|((_, keep), _)| *keep)
                .map(|((&at_least, _), floor)| Rung {
                    at_least,
                    floor: FLOORS[floor],
                })
                .collect();
            if extend {
                if let Some(last) = rungs.last_mut() {
                    last.floor = 0.0;
                }
            }
            Shape {
                max,
                rungs,
                extend,
                step: 1 + step % max,
            }
        })
}

fn check_breaks(text: &str, breaks: &[Break]) -> Result<(), TestCaseError> {
    for pair in breaks.windows(2) {
        prop_assert!(pair[0].at < pair[1].at, "{pair:?} out of order");
    }
    for cut in breaks {
        prop_assert!(
            0 < cut.at && cut.at < text.len() && text.is_char_boundary(cut.at),
            "{cut:?} is not strictly inside, on a char boundary"
        );
    }
    Ok(())
}

fn check_partition(
    text: &str,
    name: &str,
    measure: &dyn Measure,
    shape: &Shape,
) -> Result<(), TestCaseError> {
    let spans = partition(text, &Prose, measure, &shape.plan());
    let mut at = 0;
    for span in &spans {
        prop_assert_eq!(span.start, at, "{} pieces are not contiguous", name);
        prop_assert!(span.end > span.start, "{name} has an empty piece");
        prop_assert!(text.is_char_boundary(span.end), "{name} cuts a char");
        at = span.end;
    }
    prop_assert_eq!(at, text.len(), "{} pieces do not reach the end", name);
    let breaks = Prose.breaks(text);
    for span in &spans {
        if measure.reach(text, span.start, shape.max) >= span.end {
            continue;
        }
        match (shape.extend, shape.rungs.last()) {
            (true, Some(last)) => prop_assert!(
                !breaks.iter().any(|cut| span.start < cut.at
                    && cut.at < span.end
                    && cut.strength >= last.at_least),
                "{name} piece {span:?} runs past a break of the last rung"
            ),
            _ => prop_assert_eq!(
                text[span.clone()].chars().count(),
                1,
                "{} piece {:?} is over budget",
                name,
                span
            ),
        }
    }
    Ok(())
}

fn check_windows(
    text: &str,
    name: &str,
    measure: &dyn Measure,
    shape: &Shape,
) -> Result<(), TestCaseError> {
    let spans = windows(text, &Prose, measure, &shape.plan(), shape.step);
    for pair in spans.windows(2) {
        prop_assert!(pair[0].start < pair[1].start, "{name} starts do not rise");
    }
    for span in &spans {
        let window = &text[span.clone()];
        prop_assert!(
            !window.is_empty() && window == window.trim(),
            "{name} window {span:?} is empty or untrimmed"
        );
    }
    for (at, c) in text.char_indices().filter(|(_, c)| !c.is_whitespace()) {
        prop_assert!(
            spans
                .iter()
                .any(|span| span.start <= at && at + c.len_utf8() <= span.end),
            "{name} leaves {c:?} at {at} out of every window"
        );
    }
    Ok(())
}

fn check_units(text: &str) -> Result<(), TestCaseError> {
    for strength in [
        Strength::Char,
        Strength::Word,
        Strength::Clause,
        Strength::Sentence,
        Strength::Line,
        Strength::Paragraph,
        Strength::Section,
    ] {
        let spans = units(text, &Prose, strength);
        let mut at = 0;
        for span in &spans {
            prop_assert_eq!(span.start, at, "{:?} units are not contiguous", strength);
            prop_assert!(span.end > span.start, "{strength:?} has an empty unit");
            at = span.end;
        }
        prop_assert_eq!(at, text.len(), "{:?} units do not tile", strength);
    }
    Ok(())
}

fn check_sentences(text: &str) -> Result<(), TestCaseError> {
    let spans = sentences(text);
    let mut at = 0;
    for &(start, end) in &spans {
        prop_assert!(at <= start && start < end && end <= text.len());
        prop_assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        let sentence = &text[start..end];
        prop_assert!(sentence == sentence.trim(), "{sentence:?} is not trimmed");
        at = end;
    }
    Ok(())
}

proptest! {
    #[test]
    fn breaks_ascend_without_repeats_strictly_inside_on_char_boundaries(text in texts()) {
        check_breaks(&text, &Prose.breaks(&text))?;
        check_breaks(&text, &Flowed.breaks(&text))?;
        check_breaks(&text, &Code.breaks(&text))?;
    }

    #[test]
    fn partitions_tile_the_text_and_keep_to_their_budget(text in texts(), shape in shapes()) {
        for (name, measure) in MEASURES {
            check_partition(&text, name, measure, &shape)?;
        }
    }

    #[test]
    fn windows_start_further_on_and_cover_every_word(text in texts(), shape in shapes()) {
        for (name, measure) in MEASURES {
            check_windows(&text, name, measure, &shape)?;
        }
    }

    #[test]
    fn flowed_breaks_keep_the_offsets_of_prose(text in texts()) {
        let offsets = |breaks: Vec<Break>| breaks.into_iter().map(|cut| cut.at).collect::<Vec<_>>();
        prop_assert_eq!(offsets(Flowed.breaks(&text)), offsets(Prose.breaks(&text)));
    }

    #[test]
    fn units_tile_the_text(text in texts()) {
        check_units(&text)?;
    }

    #[test]
    fn sentences_ascend_trimmed_and_non_empty(text in texts()) {
        check_sentences(&text)?;
    }
}

#[test]
fn a_list_marker_and_the_whitespace_after_it_end_where_the_line_text_starts() {
    fn line(text: &str) -> Option<&str> {
        item_marker_end(text).map(|end| &text[..end])
    }
    assert_eq!(line("3. Alpha holds."), Some("3. "));
    assert_eq!(line("- Alpha holds."), Some("- "));
    assert_eq!(line("   2) Alpha holds."), Some("   2) "));
    assert_eq!(line("  * Alpha holds."), Some("  * "));
    assert_eq!(line("Alpha holds."), None);
    assert_eq!(line("3 things hold."), None);
    assert_eq!(line("-Alpha holds."), None);
    for text in ["3. A.", "- A.", "   2) A.", "A.", "3 things", "-A."] {
        assert_eq!(
            prose::is_list_item(text),
            item_marker_end(text).is_some(),
            "{text}"
        );
    }
}
