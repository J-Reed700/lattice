use super::super::test_support::source;
use super::*;

#[test]
fn a_claim_deep_in_a_whole_cited_page_is_found_but_a_scattered_one_is_not() {
    // The page the model read, not the search snippet: the supporting
    // sentence sits thousands of characters in.
    let page = format!(
        "{} Bush beans mature within sixty days and grow well in shallow window boxes. {} {}",
        "Notes on compost, mulch and seed trays. ".repeat(60),
        "Trellis spacing and irrigation schedules. ".repeat(60),
        "Pollination needs wind or hand shaking for corn. ".repeat(10),
    );
    let claims = lexical_pass(
        "Bush beans mature within sixty days in shallow window boxes [1].",
        &[source(&page)],
    );
    assert!(claims[0].supported);

    // Every word appears somewhere on the page, but no one stretch of it
    // says this; a single token set over the whole article would pass it.
    let claims = lexical_pass(
        "Compost pollination needs trellis irrigation seed windows [1].",
        &[source(&page)],
    );
    assert!(claims[0].overlap_ratio < 1.0);
}

#[test]
fn a_follow_on_sentence_answers_to_the_marker_of_its_own_point() {
    let sources = [
        source("High-calorie fruiting crops need 6–8 hours of strong light; use LED grow lights."),
        source("Thyme is tough indoors while rosemary wants direct sunlight."),
    ];
    let claims = lexical_pass(
        "- Fruiting crops need 6–8 hours of strong light, and LED grow lights are the workaround [1]. \
             So needing a lot of sun really means needing strong light for hours.\n\
             - Thyme copes indoors but rosemary wants direct sunlight [2].\n\
             ## A heading that says something about peanut pods underground\n\
             Peanuts develop pods underground, which is my inference and cites nothing at all.",
        &sources,
    );
    let follow_on = claims
        .iter()
        .find(|claim| claim.sentence.starts_with("So needing"))
        .expect("follow-on sentence is a claim");
    // The next marker is on another line, so it cannot claim this one.
    assert_eq!(follow_on.citation_ids, vec![1]);
    let aside = claims
        .iter()
        .find(|claim| claim.sentence.starts_with("Peanuts"))
        .expect("uncited paragraph is a claim");
    assert!(aside.citation_ids.is_empty());
}

#[test]
fn a_bolded_opening_sentence_does_not_split_inside_its_emphasis() {
    // Verbatim from a turn this got wrong: the bold closes after the full
    // stop, so the splitter cut there and the second claim reached the
    // judge — and the user — reading "** The ideal initial planting depth".
    let claims = lexical_pass(
        "**Bury it all the way — you don't leave the top sticking out.** The ideal initial \
             planting depth is 4–6 inches of soil above the seed potato [1].",
        &[source(
            "The ideal initial planting depth for potatoes is 4 to 6 inches. This initial 4- \
                 to 6-inch soil covering provides the seed potato with insulation and moisture.",
        )],
    );

    assert_eq!(claims.len(), 2);
    assert!(
        claims.iter().all(|claim| !claim.sentence.contains('*')),
        "emphasis markers reached the claim text: {:?}",
        claims.iter().map(|c| &c.sentence).collect::<Vec<_>>()
    );
    assert!(claims[0].sentence.starts_with("Bury it all the way"));
    assert!(claims[1]
        .sentence
        .starts_with("The ideal initial planting depth"));
}

#[test]
fn an_underscore_inside_a_word_is_not_emphasis() {
    assert_eq!(
        strip_emphasis_markers("Set _max_tokens_ in the *config* file"),
        "Set max_tokens in the config file"
    );
    assert_eq!(
        strip_emphasis_markers("A rule: ---- and ____"),
        "A rule: ---- and ____"
    );
}

#[test]
fn a_sentence_with_no_citation_is_never_sent_to_the_judge() {
    // Its only possible evidence would be whichever sources ranked first,
    // which it never claimed anything about. A closing summary line was
    // marked "contradicted" that way, against a different document.
    let claims = lexical_pass(
        "Potatoes want 4–6 inches of cover [1].\n\
             So: fully buried at first, then buried deeper as the plant grows.",
        &[source(
            "The ideal initial planting depth for potatoes is 4 to 6 inches of soil cover.",
        )],
    );

    let summary = claims
        .iter()
        .find(|claim| claim.sentence.starts_with("So:"))
        .expect("the uncited closing line is still a claim");
    assert!(summary.citation_ids.is_empty());
    assert!(!needs_judge(summary));
    // The cited one still goes, numbers being exactly what overlap cannot settle.
    let cited = claims
        .iter()
        .find(|claim| claim.citation_ids == vec![1])
        .expect("the cited claim");
    assert!(needs_judge(cited));
}

#[test]
fn a_sentence_with_no_citation_is_never_backed_on_shared_words() {
    // Half its words are the source's, and the source says nothing of yields:
    // overlap with whichever window ranks best is not a source for it.
    let claims = lexical_pass(
        "Salicylic acid treatment tripled blueberry yields in commercial orchards.",
        &[source(
            "Salicylic acid treatment improved cold tolerance in blueberry plants.",
        )],
    );

    let claim = claims.first().expect("the uncited sentence is a claim");
    assert!(claim.citation_ids.is_empty());
    assert!(claim.overlap_ratio > 0.0, "overlap is still measured");
    assert!(!claim.supported);
}

#[test]
fn grounded_claim_scores_above_threshold() {
    let claims = lexical_pass(
        "Blueberry plants showed improved cold tolerance after salicylic acid treatment [1].",
        &[source(
            "Salicylic acid treatment improved cold tolerance in blueberry plants during low temperature stress.",
        )],
    );

    assert_eq!(claims.len(), 1);
    assert!(claims[0].supported);
    assert_eq!(claims[0].citation_ids, vec![1]);
}

#[test]
fn unsupported_claim_scores_below_threshold() {
    let claims = lexical_pass(
        "Blueberry plants tripled yield after lunar-cycle irrigation [1].",
        &[source(
            "The study measured antioxidant enzyme response under low-temperature stress.",
        )],
    );

    assert_eq!(claims.len(), 1);
    assert!(!claims[0].supported);
}

#[test]
fn short_or_question_sentences_are_skipped() {
    let claims = lexical_pass("Any update?\nThanks.", &[source("Random source text.")]);
    assert!(claims.is_empty());
}

#[test]
fn the_cut_short_note_is_not_a_claim() {
    let answer = format!(
        "Bush beans mature within sixty days in shallow window boxes [1].{}",
        super::super::super::tool_loop::CUT_SHORT_NOTE
    );
    let claims = lexical_pass(
        &answer,
        &[source(
            "Bush beans mature within sixty days and grow well in shallow window boxes.",
        )],
    );
    assert_eq!(claims.len(), 1);
    assert!(claims[0].supported);
}

/// Each of the last round's notes: the rounds', the window's and the clock's.
#[test]
fn the_out_of_rounds_note_is_not_a_claim() {
    for note in [
        super::super::super::tool_loop::OUT_OF_ROUNDS_NOTE,
        super::super::super::tool_loop::WINDOW_FULL_NOTE,
        super::super::super::tool_loop::OUT_OF_TIME_NOTE,
    ] {
        let answer =
            format!("Bush beans mature within sixty days in shallow window boxes [1].{note}");
        let claims = lexical_pass(
            &answer,
            &[source(
                "Bush beans mature within sixty days and grow well in shallow window boxes.",
            )],
        );
        assert_eq!(claims.len(), 1, "{note}");
        assert!(claims[0].supported);
    }
}

#[test]
fn citation_index_selects_the_referenced_source() {
    let claims = lexical_pass(
        "Blueberry anthocyanins support vascular function and endothelial health [2].",
        &[
            source("Irrigation protocol and frost timing notes for greenhouse control."),
            source("Anthocyanins from blueberries were associated with improved vascular endothelial function in adults."),
        ],
    );

    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].citation_ids, vec![2]);
    assert_eq!(claims[0].cited_source_indices, vec![1]);
    assert!(claims[0].supported);
}

#[test]
fn mismatched_citation_index_is_flagged_even_if_another_source_matches() {
    let claims = lexical_pass(
        "Blueberry anthocyanins support vascular function and endothelial health [1].",
        &[
            source("Irrigation protocol and frost timing notes for greenhouse control."),
            source("Anthocyanins from blueberries were associated with improved vascular endothelial function in adults."),
        ],
    );

    assert_eq!(claims.len(), 1);
    assert!(!claims[0].supported);
}

#[test]
fn short_cited_numeric_answers_are_not_skipped() {
    let claims = lexical_pass(
        "Retention is 14 days [1].",
        &[source("Retention is 30 days.")],
    );
    assert_eq!(claims.len(), 1);
    assert!(needs_judge(&claims[0]));
}

#[test]
fn invalid_citation_cannot_borrow_support_from_another_source() {
    for citation in ["[99]", "[0]", "[1][99]"] {
        let claims = lexical_pass(
            &format!("Blueberry plants showed improved cold tolerance after salicylic acid treatment {citation}."),
            &[source("Blueberry plants showed improved cold tolerance after salicylic acid treatment.")],
        );
        assert_eq!(claims.len(), 1);
        assert!(
            !claims[0].supported,
            "invalid citation {citation} must fail closed"
        );
    }
}

#[test]
fn empty_source_does_not_shift_citation_evidence() {
    let mut empty = source("");
    empty.file_name.clear();
    empty.file_path.clear();
    let claims = lexical_pass(
        "Blueberry plants showed improved cold tolerance after salicylic acid treatment [2].",
        &[
            empty,
            source(
                "Blueberry plants showed improved cold tolerance after salicylic acid treatment.",
            ),
        ],
    );
    assert_eq!(claims[0].cited_source_indices, vec![1]);
    assert!(claims[0].supported);
}

#[test]
fn assigned_citation_ids_survive_source_reordering() {
    let mut matching =
        source("Blueberry plants showed improved cold tolerance after salicylic acid treatment.");
    matching.citation_id = Some(7);
    let mut unrelated =
        source("Irrigation protocol and frost timing notes for greenhouse control.");
    unrelated.citation_id = Some(2);
    let claims = lexical_pass(
        "Blueberry plants showed improved cold tolerance after salicylic acid treatment [7].",
        &[unrelated, matching],
    );
    assert_eq!(claims[0].citation_ids, vec![7]);
    assert_eq!(claims[0].cited_source_indices, vec![1]);
    assert!(claims[0].supported);
}

#[test]
fn claim_text_keeps_the_full_sentence() {
    let claims = lexical_pass(
        "The same epigenetic shifts are linked to altered expression of genes that control meristem activity, suggesting a mechanistic link between methylation dynamics and developmental transitions [1][2].",
        &[source("The study discusses WOX family structure and stress response without this meristem claim.")],
    );

    assert_eq!(claims.len(), 1);
    assert!(claims[0].claim_text.contains(
        "The same epigenetic shifts are linked to altered expression of genes that control meristem activity"
    ));
    assert!(claims[0]
        .claim_text
        .contains("mechanistic link between methylation dynamics and developmental transitions"));
    assert!(!claims[0].claim_text.contains("developmental tr..."));
}

#[test]
fn a_decimal_point_does_not_end_a_sentence() {
    let sentences = split_sentences(
        "The pooled effect was 1.2 °C per 10 points [1]. Surface readings ran 8.5 °C apart!\nSee example.com for v2.1.",
    );

    assert_eq!(
        sentences,
        vec![
            "The pooled effect was 1.2 °C per 10 points [1].",
            "Surface readings ran 8.5 °C apart!",
            "See example.com for v2.1.",
        ]
    );
}

#[test]
fn numbers_dates_and_negations_are_contradiction_prone() {
    assert!(is_contradiction_prone("Yields rose by 42 percent."));
    assert!(is_contradiction_prone("The trial began in March 2021."));
    assert!(is_contradiction_prone(
        "The treatment did not improve tolerance."
    ));
    assert!(is_contradiction_prone(
        "The sample didn't survive the freeze."
    ));
    assert!(is_contradiction_prone("Only the treated group recovered."));
    assert!(!is_contradiction_prone(
        "Salicylic acid treatment improved cold tolerance in blueberry plants."
    ));
}

#[test]
fn a_near_verbatim_copy_skips_the_judge_but_strong_overlap_or_a_number_does_not() {
    let copy = LexicalClaim {
        sentence: "s".into(),
        claim_text: "s".into(),
        citation_ids: vec![1],
        cited_source_indices: vec![0],
        overlap_ratio: 1.0,
        matching_tokens: 6,
        supported: true,
        near_verbatim: true,
        contradiction_prone: false,
    };
    assert!(!needs_judge(&copy));

    // Most of the passage's words, but not its wording: this is what
    // certified a dropped "not" or a swapped owner without a model.
    let strong_overlap = LexicalClaim {
        overlap_ratio: 0.8,
        near_verbatim: false,
        ..copy.clone()
    };
    assert!(needs_judge(&strong_overlap));

    let with_number = LexicalClaim {
        contradiction_prone: true,
        ..copy.clone()
    };
    assert!(needs_judge(&with_number));

    let too_short = LexicalClaim {
        matching_tokens: 3,
        ..copy.clone()
    };
    assert!(needs_judge(&too_short));

    let ambiguous = LexicalClaim {
        overlap_ratio: 0.40,
        matching_tokens: 3,
        near_verbatim: false,
        ..copy.clone()
    };
    assert!(needs_judge(&ambiguous));

    let weak = LexicalClaim {
        overlap_ratio: 0.10,
        matching_tokens: 1,
        supported: false,
        near_verbatim: false,
        ..copy
    };
    assert!(needs_judge(&weak));
}

#[test]
fn a_copy_is_only_a_copy_when_its_words_keep_their_order() {
    let passage = "The Sepwick field crew owns the pager for ingest-fanout. \
                       The Halloway reliability bench owns the underlying storage.";
    let copied = lexical_pass(
        "The Sepwick field crew owns the pager for ingest-fanout [1].",
        &[source(passage)],
    );
    assert!(copied[0].near_verbatim);
    assert!(!needs_judge(&copied[0]));

    // The same words, every one of them in the passage, with the owners
    // swapped. Overlap is total; the runs of four words are not there.
    let swapped = lexical_pass(
        "The Halloway reliability bench owns the pager for ingest-fanout [1].",
        &[source(passage)],
    );
    assert!((swapped[0].overlap_ratio - 1.0).abs() < f32::EPSILON);
    assert!(!swapped[0].near_verbatim);
    assert!(needs_judge(&swapped[0]));
}

#[test]
fn a_copied_sentence_is_read_for_what_the_claim_left_out() {
    let kept_everything = "The quota broker holds its state in memory.";
    assert!(!drops_a_marker(kept_everything, kept_everything));
    // The first word of a sentence is capitalised whatever it is.
    assert!(!drops_a_marker(
        "quota broker holds its state in memory",
        "Quota broker holds its state in memory."
    ));

    for (claim, sentence) in [
        ("Readings are reordered.", "Readings aren't reordered."),
        (
            "Readings are reordered.",
            "Only late readings are reordered.",
        ),
        ("The probe failed.", "The probe failed twice."),
        ("The probe failed.", "The probe failed 3 times."),
        ("The window opens.", "The window opens on Thursday."),
        ("The window opens.", "The window opens in March."),
        (
            "The pager belongs to the crew.",
            "The pager belongs to the Sepwick crew.",
        ),
    ] {
        assert!(
            drops_a_marker(claim, sentence),
            "{claim:?} dropped nothing from {sentence:?}"
        );
    }
}

/// Explorer answers point at lines in the folder. Those sentences are
/// about the file on screen, so the sources this turn retrieved cannot
/// judge them, even when one happens to share their words.
#[test]
fn a_sentence_with_a_line_reference_is_not_judged_against_sources() {
    let passage =
        source("The main function prints a greeting and exits with status zero after setup.");
    let claims = lexical_pass(
        "The main function prints a greeting and exits with status zero, see `src/main.rs:1-3` [1]. \
             The main function prints a greeting and exits with status zero after setup [1].",
        std::slice::from_ref(&passage),
    );
    assert_eq!(claims.len(), 1, "{claims:?}");
    assert!(!claims[0].sentence.contains("src/main.rs"));
}

/// A diagram's labels and a picture's caption are not claims: a `[1]` inside
/// a mermaid fence and an image line produce none, and the prose around them
/// is still checked.
#[test]
fn fenced_diagrams_and_picture_lines_make_no_claims() {
    let sources = [source(
        "The Saturn V launched Apollo 11 from Kennedy Space Center on 16 July 1969.",
    )];
    let claims = lexical_pass(
        "```mermaid\nflowchart LR\n  A[\"Launch pad cleared for the rocket [1]\"] --> B[Orbit]\n```\n\n\
         ![Saturn V lifting off from the launch pad [1]](img-1)\n\n\
         The Saturn V launched Apollo 11 from Kennedy Space Center in July 1969 [1].",
        &sources,
    );
    assert_eq!(claims.len(), 1, "{claims:?}");
    assert!(claims[0].sentence.contains("Kennedy Space Center"));
    assert!(claims[0].supported);
}

/// A displayed formula is set aside like a diagram. An inline one is part of
/// its sentence: its brackets are no citation and its TeX no claim word, so
/// `\frac` cannot cost a sentence its support.
#[test]
fn formulas_make_no_claims_markers_or_missing_words() {
    let sources = [source(
        "Precision is the share of retrieved passages that are relevant to the query.",
    )];
    let claims = lexical_pass(
        "$$\n\\text{precision} = \\frac{a[1]}{b}\n$$\n\n\
         Precision $\\frac{TP}{TP + FP}$ is the share of retrieved passages that are relevant [1].\n\n\
         The ordered list $a[2]$ of retrieved passages that are relevant to the query is sorted.",
        &sources,
    );
    assert_eq!(claims.len(), 2, "{claims:?}");
    assert!(claims
        .iter()
        .all(|claim| !claim.sentence.contains("\\text")));
    assert!(claims[0].sentence.contains("$\\frac{TP}{TP + FP}$"));
    assert_eq!(claims[0].citation_ids, vec![1]);
    assert_eq!(claims[0].overlap_ratio, 1.0, "{claims:?}");
    assert!(claims[1].citation_ids.is_empty(), "{claims:?}");
}

#[test]
fn a_formula_is_one_piece_of_its_sentence() {
    let sentences = sentences_with_citations(
        "Orderings number $n! \\cdot k_{i}$ for each run of passages [1]. Next one [2].",
    );
    assert_eq!(sentences.len(), 2, "{sentences:?}");
    assert_eq!(
        sentences[0],
        (
            "Orderings number $n! \\cdot k_{i}$ for each run of passages [1].".to_string(),
            vec![1]
        )
    );
}
