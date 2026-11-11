use super::*;
#[test]
fn small_repairs_preserve_every_other_byte_and_reject_ambiguous_or_foreign_edits() -> Result<()> {
    let original = json!({"blocks":[{"kind":"explanation","body":"Keep 🧪 this. Wrong fact. Keep this too.","title":"Retained"},{"body":"Other section"}],"questions":[]});
    let patch =
        json!({"edits":[{"path":"/blocks/0/body","before":"Wrong fact.","after":"Correct fact."}]});
    let repaired = apply(&original, &patch.to_string())?;
    let mut expected = original.clone();
    expected["blocks"][0]["body"] = json!("Keep 🧪 this. Correct fact. Keep this too.");
    assert_eq!(repaired, expected);
    for edits in [
        json!([{"path":"/blocks/0/body","before":"Keep","after":"Rewrite"}]),
        json!([{"path":"/blocks/0/kind","before":"explanation","after":"recap"}]),
        json!([{"path":"/blocks/0","before":original["blocks"][0],"after":{}}]),
        json!([{"path":"/blocks/0/body","before":"Wrong fact.","after":"Correct."},{"path":"/blocks/0/body","before":"fact.","after":"assertion."}]),
    ] {
        assert!(apply(&original, &json!({"edits":edits}).to_string()).is_err());
    }
    assert!(
        apply(&original, &repaired.to_string()).is_err(),
        "Full rewrites are not edit responses"
    );
    Ok(())
}

fn two_sections() -> Value {
    json!({"blocks":[
        {"kind":"explanation","body":"Alpha is the first of them all. Beta is second."},
        {"kind":"explanation","body":"Gamma stands here. Delta ends it."}
    ],"questions":[]})
}

#[test]
fn an_edit_adding_a_sentence_another_section_has_is_rejected_and_the_field_is_unchanged() {
    let original = two_sections();
    let repeat = json!({"path":"/blocks/1/body","before":"Delta ends it.","after":"Delta ends it. Alpha is the first of them all."});
    let alone = apply(&original, &json!({"edits":[repeat]}).to_string());
    assert!(alone.is_err(), "the sentence is in section 1 already");
    assert!(
        alone
            .unwrap_err()
            .to_string()
            .contains("Alpha is the first of them all."),
        "the reason names the sentence"
    );
    let good =
        json!({"path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."});
    let mixed = apply(&original, &json!({"edits":[repeat, good]}).to_string()).unwrap();
    assert_eq!(mixed["blocks"][1], original["blocks"][1]);
    assert_eq!(
        mixed["blocks"][0]["body"],
        "Alpha is the first of them all. Beta comes second."
    );
}

fn cited_sections() -> Value {
    json!({"blocks":[
        {"kind":"explanation","body":"Alpha is the first of them all. Beta is second."},
        {"kind":"explanation","body":"Gamma stands here. Delta ends it.","citations":[{"sentence":"Gamma stands here.","sourceIndex":0,"quote":"Gamma is stated by the source."}]}
    ],"questions":[]})
}

#[test]
fn a_citation_sentence_may_equal_a_sentence_the_lesson_has() {
    let original = cited_sections();
    let body =
        json!({"path":"/blocks/1/body","before":"Delta ends it.","after":"Delta closes it."});
    let cite = json!({"path":"/blocks/1/citations/0/sentence","before":"Gamma stands here.","after":"Alpha is the first of them all."});
    let repaired = apply_in(
        &original,
        &json!({"edits":[body, cite]}).to_string(),
        &original,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        repaired["blocks"][1]["citations"][0]["sentence"],
        "Alpha is the first of them all."
    );
    assert_eq!(
        repaired["blocks"][1]["body"],
        "Gamma stands here. Delta closes it."
    );
}

#[test]
fn a_citation_quote_may_equal_a_sentence_the_lesson_has() {
    let original = cited_sections();
    let body =
        json!({"path":"/blocks/1/body","before":"Delta ends it.","after":"Delta closes it."});
    let cite = json!({"path":"/blocks/1/citations/0/quote","before":"Gamma is stated by the source.","after":"Beta is second."});
    let repaired = apply_in(
        &original,
        &json!({"edits":[body, cite]}).to_string(),
        &original,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        repaired["blocks"][1]["citations"][0]["quote"],
        "Beta is second."
    );
    assert_eq!(
        repaired["blocks"][1]["body"],
        "Gamma stands here. Delta closes it."
    );
}

#[test]
fn a_list_item_step_another_item_has_may_be_added() {
    let original = json!({"blocks":[
        {"kind":"explanation","body":"Intro line.\n\n- Type `pwd`.\n- Read the output."},
        {"kind":"explanation","body":"Other text.\n\n- Open a terminal."}
    ],"questions":[]});
    let edit = json!({"path":"/blocks/1/body","before":"- Open a terminal.","after":"- Open a terminal.\n- Type `pwd`."});
    let repaired = apply(&original, &json!({"edits":[edit]}).to_string());
    assert!(repaired.is_ok(), "a step is not a sentence of the prose");
    assert_eq!(
        repaired.unwrap()["blocks"][1]["body"],
        "Other text.\n\n- Open a terminal.\n- Type `pwd`."
    );
}

#[test]
fn a_line_under_a_step_after_its_fence_may_be_added_to_another_step() {
    let original = json!({"blocks":[
        {"kind":"explanation","body":"Method A\n\n1. Run:\n   ```\n   pwd\n   ```\n   Expected output: `/home/learner/Music`."},
        {"kind":"explanation","body":"Method B\n\n1. Run:\n   ```\n   pwd\n   ```"}
    ],"questions":[]});
    let edit = json!({"path":"/blocks/1/body","before":"   ```\n   pwd\n   ```","after":"   ```\n   pwd\n   ```\n   Expected output: `/home/learner/Music`."});
    let repaired = apply(&original, &json!({"edits":[edit]}).to_string());
    assert!(repaired.is_ok(), "the line belongs to the step");
}

#[test]
fn an_edit_adding_a_sentence_its_own_field_has_is_rejected() {
    let original = two_sections();
    let repeat = json!({"path":"/blocks/0/body","before":"Beta is second.","after":"Beta is second. Alpha is the first of them all."});
    let outcome = apply(&original, &json!({"edits":[repeat]}).to_string());
    assert!(outcome.is_err(), "the field has the sentence already");
}

#[test]
fn a_sentence_rewritten_in_place_is_accepted() {
    let original = two_sections();
    let edits = [
        json!({"path":"/blocks/0/body","before":"Beta is second.","after":"Beta is the second one."}),
        json!({"path":"/blocks/1/body","before":"Gamma stands here. Delta ends it.","after":"Gamma stands here. Delta closes it."}),
    ];
    let repaired = apply(&original, &json!({"edits":edits}).to_string()).unwrap();
    assert_eq!(
        repaired["blocks"][0]["body"],
        "Alpha is the first of them all. Beta is the second one."
    );
    assert_eq!(
        repaired["blocks"][1]["body"],
        "Gamma stands here. Delta closes it."
    );
}

#[test]
fn a_refused_patch_says_which_edit_failed_and_why() {
    let original =
        json!({"blocks":[{"kind":"explanation","body":"Keep this. Wrong fact. Keep this too."}]});
    let refused = |edits: Value| {
        apply(&original, &json!({ "edits": edits }).to_string())
            .unwrap_err()
            .to_string()
    };
    let repeated = refused(json!([
        {"path":"/blocks/0/body","before":"Wrong fact.","after":"Right fact."},
        {"path":"/blocks/0/body","before":"Keep this","after":"Hold this"}
    ]));
    assert!(
        repeated.contains("edit 1") && repeated.contains("more than once"),
        "{repeated}"
    );
    let missing = refused(json!([{"path":"/blocks/0/body","before":"Absent.","after":"Present."}]));
    assert!(
        missing.contains("does not occur in /blocks/0/body"),
        "{missing}"
    );
}

const TIMEOUT: &str = "The default timeout is 30 seconds.";

fn cited_lesson() -> Value {
    json!({"blocks":[{"kind":"explanation","body":format!("Calls end after a timeout. {TIMEOUT} Retry later."),"citations":[]}],
        "questions":[{"kind":"quiz","prompt":"Which?","options":["a","b","c","d"],"explanation":"The cache holds 128 entries.","citations":[]}]})
}

fn citing(citations: Value) -> String {
    json!({"edits":[],"citations":citations}).to_string()
}

#[test]
fn a_citation_is_appended_to_the_part_it_names_and_nothing_else_changes() -> Result<()> {
    let original = cited_lesson();
    let first = json!({"sentence":TIMEOUT,"sourceIndex":0,"quote":"A timeout of 30 seconds applies to each request."});
    let second = json!({"sentence":"The cache holds 128 entries.","sourceIndex":null,"quote":"The cache holds at most 128 entries at a time."});

    let revised = apply(
        &original,
        &citing(json!([
            {"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,"quote":"A timeout of 30 seconds applies to each request."},
            {"path":"/questions/0","sentence":"The cache holds 128 entries.","sourceIndex":null,"quote":"The cache holds at most 128 entries at a time."}
        ])),
    )?;

    let mut expected = original.clone();
    expected["blocks"][0]["citations"] = json!([first]);
    expected["questions"][0]["citations"] = json!([second]);
    assert_eq!(revised, expected, "the sentences themselves are untouched");
    // Edits and a citation go together: one sentence rewritten, another cited.
    let both = json!({
        "edits":[{"path":"/blocks/0/body","before":"Retry later.","after":"Retry after a pause."}],
        "citations":[{"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,"quote":"A timeout of 30 seconds applies to each request."}]
    });
    let revised = apply(&original, &both.to_string())?;
    assert_eq!(
        revised["blocks"][0]["body"],
        format!("Calls end after a timeout. {TIMEOUT} Retry after a pause.")
    );
    assert_eq!(revised["blocks"][0]["citations"], json!([first]));
    Ok(())
}

#[test]
fn a_citation_may_name_a_sentence_of_a_choices_reason() -> Result<()> {
    let mut original = cited_lesson();
    original["questions"][0]["explanation"] = json!("The cache is small.");
    original["questions"][0]["rationales"] = json!([
        "",
        "The retry limit is 5 attempts.",
        "Three fails.",
        "Four fails."
    ]);
    let cite = json!({"sentence":"The retry limit is 5 attempts.","sourceIndex":0,"quote":"Each request is retried at most 5 attempts."});

    let revised = apply(
        &original,
        &citing(
            json!([{"path":"/questions/0","sentence":"The retry limit is 5 attempts.","sourceIndex":0,"quote":"Each request is retried at most 5 attempts."}]),
        ),
    )?;

    assert_eq!(revised["questions"][0]["citations"], json!([cite]));
    Ok(())
}

#[test]
fn a_citation_the_draft_cannot_hold_is_left_out_and_the_rest_of_the_repair_is_kept() -> Result<()> {
    let original = cited_lesson();
    let quote = "A timeout of 30 seconds applies to each request.";
    let patch = json!({
        "edits":[{"path":"/blocks/0/body","before":"Retry later.","after":"Retry after a pause."}],
        "citations":[
            {"path":"/questions/0","sentence":"Timeouts last 30 seconds.","sourceIndex":0,"quote":quote},
            {"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,"quote":quote},
            {"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,"quote":quote},
            {"path":"/blocks/7","sentence":TIMEOUT,"sourceIndex":0,"quote":quote}
        ]
    });

    let revised = apply(&original, &patch.to_string())?;

    let mut expected = original.clone();
    expected["blocks"][0]["body"] = json!(format!(
        "Calls end after a timeout. {TIMEOUT} Retry after a pause."
    ));
    expected["blocks"][0]["citations"] =
        json!([{"sentence":TIMEOUT,"sourceIndex":0,"quote":quote}]);
    assert_eq!(revised, expected);
    Ok(())
}

#[test]
fn a_citation_that_names_no_part_or_a_sentence_the_part_lacks_or_one_it_lists_is_refused_alone() {
    let original = cited_lesson();
    let quote = "A timeout of 30 seconds applies to each request.";
    let refused = |path: &str, sentence: &str| {
        apply(
            &original,
            &citing(json!([{"path":path,"sentence":sentence,"sourceIndex":0,"quote":quote}])),
        )
        .unwrap_err()
        .to_string()
    };
    for path in ["/blocks", "/blocks/x", "/blocks/0/body", "/lesson"] {
        assert!(
            refused(path, TIMEOUT).contains("not a block or a question"),
            "{path}"
        );
    }
    assert!(refused("/blocks/4", TIMEOUT).contains("not in the draft"));
    assert!(refused("/blocks/0", "Timeouts last 30 seconds.").contains("copy it exactly"));
    assert!(refused("/blocks/0", "  ").contains("copy it exactly"));
    // The cache sentence is in the question's explanation, not a body.
    assert!(refused("/blocks/0", "The cache holds 128 entries.").contains("copy it exactly"));
    // Given twice, a citation is appended once; given again, it leaves
    // nothing to apply.
    let twice = json!([
        {"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,"quote":quote},
        {"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,"quote":quote}
    ]);
    let once = apply(&original, &citing(twice)).unwrap();
    assert_eq!(
        once["blocks"][0]["citations"],
        json!([{"sentence":TIMEOUT,"sourceIndex":0,"quote":quote}])
    );
    let again = json!([{"path":"/blocks/0","sentence":TIMEOUT,"sourceIndex":0,"quote":quote}]);
    let message = apply(&once, &citing(again)).unwrap_err().to_string();
    assert!(
        message.contains("citation 0") && message.contains("listed in /blocks/0 already"),
        "{message}"
    );
    // A response with neither edits nor citations says nothing.
    assert!(apply(&original, "{}").is_err());
}

#[test]
fn a_lessons_schema_offers_citations_and_another_schema_does_not() {
    let original = cited_lesson();
    let lesson = crate::generation::lesson_schema(2);

    let offered = schema_for(&original, &lesson);

    assert_eq!(offered["required"], json!(["edits", "citations"]));
    assert_eq!(offered["properties"]["edits"]["minItems"], 0);
    let item = &offered["properties"]["citations"]["items"];
    assert_eq!(
        item["properties"]["path"]["enum"],
        json!(["/blocks/0", "/questions/0"])
    );
    assert_eq!(
        item["properties"]["sourceIndex"],
        json!({"type":"integer","minimum":0,"maximum":1})
    );
    assert_eq!(
        item["required"],
        json!(["path", "sentence", "sourceIndex", "quote"])
    );
    assert!(instructions(&lesson).contains("Prefer citing it"));
    // An outline's, an assessment's or any other schema gets the edits alone.
    let other = json!({"type":"object"});
    assert_eq!(schema_for(&original, &other), schema(&original));
    assert_eq!(instructions(&other), INSTRUCTIONS);
    assert_eq!(schema(&original)["properties"]["edits"]["minItems"], 1);
}

fn follows_the_writers_citation_rules(text: &str) {
    for rule in [
        "Never say in the lesson's text where a statement comes from, by number or by name",
        "Wording quoted from a source goes in double quotation marks, and only in a cited sentence; never put wording in single quotation marks.",
        "A misconception, a learner's thought, something a friend says or an example remark is written in the lesson's own sentence without quotation marks.",
    ] {
        assert!(text.contains(rule), "missing {rule:?} in {text}");
    }
    assert!(!text.contains("attribution"), "{text}");
}

#[test]
fn the_teaching_repair_follows_the_writers_citation_rules() {
    follows_the_writers_citation_rules(&instructions(&crate::generation::lesson_schema(2)));
}

#[test]
fn the_claim_repair_follows_the_writers_citation_rules() {
    follows_the_writers_citation_rules(&rewrite_instructions(&crate::generation::lesson_schema(2)));
}

#[test]
fn the_teaching_repair_says_what_needs_a_citation_in_one_sentence() {
    let text = instructions(&crate::generation::lesson_schema(2));
    let sentence = "A sentence that states a figure, a date, a version, a product's behaviour or quoted wording needs a citation.";
    assert!(text.contains(sentence), "missing {sentence:?} in {text}");
}

fn parted() -> Value {
    json!({"blocks":[
        {"kind":"explanation","title":"First","body":"Alpha holds. Beta fails."},
        {"kind":"explanation","title":"Second","body":"Gamma holds. Delta fails."},
        {"kind":"explanation","title":"Third","body":"Epsilon holds.","citations":[{"sentence":"Epsilon holds.","sourceIndex":0,"quote":"Epsilon is stated by the source."}]}],
        "questions":[{"kind":"quiz","prompt":"Which?","options":["a","b"],"explanation":"The cache holds 128 entries."}]})
}

fn patched(edits: Value) -> Result<Value> {
    apply(&parted(), &json!({ "edits": edits }).to_string())
}

fn good() -> Value {
    json!({"path":"/blocks/0/body","before":"Beta fails.","after":"Beta holds."})
}

fn only_block_0_changed(revised: &Value) {
    let mut expected = parted();
    expected["blocks"][0]["body"] = json!("Alpha holds. Beta holds.");
    assert_eq!(revised, &expected);
}

#[test]
fn a_block_with_a_broken_edit_keeps_its_text_and_another_blocks_good_edit_applies() -> Result<()> {
    let revised = patched(json!([
        good(),
        {"path":"/blocks/1/body","before":"Absent.","after":"Present."}
    ]))?;

    only_block_0_changed(&revised);
    assert_eq!(revised["blocks"][1], parted()["blocks"][1]);
    Ok(())
}

#[test]
fn an_edit_that_changes_nothing_is_dropped_and_the_others_apply() -> Result<()> {
    let revised = patched(json!([
        {"path":"/blocks/1/title","before":"Second","after":"Second"},
        good(),
        {"path":"/blocks/2/citations/0/sourceIndex","before":0,"after":0}
    ]))?;

    only_block_0_changed(&revised);
    Ok(())
}

#[test]
fn a_patch_whose_parts_all_have_problems_is_refused_and_names_every_edit() {
    let message = patched(json!([
        {"path":"/blocks/0/body","before":"Absent.","after":"Present."},
        {"path":"/blocks/1/body","before":"Missing.","after":"Present."},
        {"path":"/questions/0/explanation","before":"Nowhere.","after":"Somewhere."}
    ]))
    .unwrap_err()
    .to_string();

    for number in ["edit 0", "edit 1", "edit 2"] {
        assert!(message.contains(number), "{number}: {message}");
    }
}

#[test]
fn a_part_with_one_good_and_one_broken_edit_keeps_all_its_values() {
    let original = parted();
    let message = patched(json!([
        good(),
        {"path":"/blocks/0/title","before":"Absent","after":"Present"}
    ]))
    .unwrap_err()
    .to_string();

    assert!(message.contains("edit 1"), "{message}");
    assert_eq!(original, parted());
}

#[test]
fn a_one_section_input_with_one_broken_edit_is_refused_whole() {
    let section = parted()["blocks"][0].clone();
    let input = json!({ "blocks": [section] });
    let patch = json!({"edits":[
        {"path":"/blocks/0/title","before":"First","after":"Opening"},
        {"path":"/blocks/0/body","before":"Absent.","after":"Present."}
    ]});

    let message = apply(&input, &patch.to_string()).unwrap_err().to_string();

    assert!(message.contains("edit 1"), "{message}");
}

#[test]
fn a_questions_broken_edit_does_not_stop_a_blocks_good_edit() -> Result<()> {
    let revised = patched(json!([
        {"path":"/questions/0/explanation","before":"Nowhere.","after":"Somewhere."},
        good()
    ]))?;

    only_block_0_changed(&revised);
    Ok(())
}

#[test]
fn five_edits_to_one_number_refuse_that_block_only() -> Result<()> {
    let revised = patched(json!([
        good(),
        {"path":"/blocks/2/citations/0/sourceIndex","before":0,"after":1},
        {"path":"/blocks/2/citations/0/sourceIndex","before":1,"after":2},
        {"path":"/blocks/2/citations/0/sourceIndex","before":2,"after":3},
        {"path":"/blocks/2/citations/0/sourceIndex","before":3,"after":4},
        {"path":"/blocks/2/citations/0/sourceIndex","before":4,"after":5}
    ]))?;

    only_block_0_changed(&revised);
    assert_eq!(revised["blocks"][2], parted()["blocks"][2]);
    Ok(())
}

#[test]
fn an_edit_to_an_unknown_block_is_a_problem_of_that_part_alone() -> Result<()> {
    let revised = patched(json!([
        good(),
        {"path":"/blocks/9/body","before":"Alpha.","after":"Omega."}
    ]))?;

    only_block_0_changed(&revised);
    let message = patched(json!([
        {"path":"/blocks/9/body","before":"Alpha.","after":"Omega."},
        {"path":"/blocks/1/body","before":"Missing.","after":"Present."}
    ]))
    .unwrap_err()
    .to_string();
    assert!(
        message.contains("edit 0") && message.contains("edit 1"),
        "{message}"
    );
    Ok(())
}

#[test]
fn a_repair_is_told_not_to_come_between_a_sentence_and_one_that_refers_back_to_it() {
    assert!(
        INSTRUCTIONS.contains(
            "never between a sentence and the next one when the next one refers back to it"
        ),
        "{INSTRUCTIONS}"
    );
}

#[test]
fn an_edit_adding_a_sentence_too_short_to_quote_that_the_lesson_has_is_applied() {
    let original = json!({"blocks":[
        {"kind":"explanation","body":"The first section opens here. No."},
        {"kind":"explanation","body":"The second section opens here. Yes."}
    ],"questions":[]});
    let edit = json!({"path":"/blocks/1/body","before":"Yes.","after":"No."});
    let repaired = apply_in(
        &original,
        &json!({"edits":[edit]}).to_string(),
        &original,
        &Default::default(),
    );
    assert!(repaired.is_ok(), "{repaired:?}");
    assert_eq!(
        repaired.unwrap()["blocks"][1]["body"],
        "The second section opens here. No."
    );
}

fn anything(_: Option<&str>, _: &str) -> std::result::Result<(), String> {
    Ok(())
}

fn only_f1_in_block_0(finding: Option<&str>, part: &str) -> std::result::Result<(), String> {
    if finding == Some("f1") && part == "/blocks/0" {
        Ok(())
    } else {
        Err("is outside its finding's part.".to_owned())
    }
}

fn apply_named(candidate: &Value, patch: Value, rule: MayChange<'_>) -> Result<AppliedFor> {
    apply_for(
        candidate,
        &patch.to_string(),
        &Value::Null,
        &Wording::default(),
        rule,
    )
}

#[test]
fn two_findings_edits_to_one_field_both_apply_and_each_owns_its_sentence() -> Result<()> {
    let done = apply_named(
        &two_sections(),
        json!({"edits":[
            {"finding":"f1","path":"/blocks/0/body","before":"Alpha is the first of them all.","after":"Alpha opens them all."},
            {"finding":"f2","path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."}
        ]}),
        &anything,
    )?;
    assert_eq!(
        done.value["blocks"][0]["body"],
        "Alpha opens them all. Beta comes second."
    );
    for finding in ["f1", "f2"] {
        assert_eq!(
            done.named[finding].applied,
            BTreeSet::from(["/blocks/0/body".to_owned()])
        );
    }
    let wrote = |finding: &str, sentence: &str| {
        done.written.iter().any(|(who, path, text)| {
            who == finding && path == "/blocks/0/body" && text.trim() == sentence
        })
    };
    assert!(wrote("f1", "Alpha opens them all."), "{:?}", done.written);
    assert!(wrote("f2", "Beta comes second."), "{:?}", done.written);
    Ok(())
}

#[test]
fn an_edit_overlapping_another_findings_edit_is_left_out_and_the_earlier_one_stays() -> Result<()> {
    let done = apply_named(
        &two_sections(),
        json!({"edits":[
            {"finding":"f1","path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."},
            {"finding":"f2","path":"/blocks/0/body","before":"is second.","after":"is last."}
        ]}),
        &anything,
    )?;
    assert_eq!(
        done.value["blocks"][0]["body"],
        "Alpha is the first of them all. Beta comes second."
    );
    assert!(done.named["f1"].problems.is_empty());
    assert!(done.named["f2"].applied.is_empty());
    assert_eq!(
        done.named["f2"].problems,
        ["edit 1 to /blocks/0/body overlaps an edit for another finding."]
    );
    Ok(())
}

#[test]
fn a_refused_group_reports_its_reason_under_its_finding_and_the_others_apply() -> Result<()> {
    let both = |rule: MayChange<'_>| {
        apply_named(
            &two_sections(),
            json!({"edits":[
                {"finding":"f1","path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."},
                {"finding":"f2","path":"/blocks/1/body","before":"Delta ends it.","after":"Delta closes it."}
            ]}),
            rule,
        )
    };
    let done = both(&only_f1_in_block_0)?;
    assert_eq!(
        done.value["blocks"][0]["body"],
        "Alpha is the first of them all. Beta comes second."
    );
    assert_eq!(done.value["blocks"][1], two_sections()["blocks"][1]);
    assert_eq!(
        done.named["f2"].problems,
        ["edit 1 is outside its finding's part."]
    );
    assert!(done.named["f2"].applied.is_empty());
    assert_eq!(
        done.named["f1"].applied,
        BTreeSet::from(["/blocks/0/body".to_owned()])
    );
    Ok(())
}

#[test]
fn an_edit_naming_no_finding_is_left_out_when_the_rule_wants_one() -> Result<()> {
    let wants_one = |finding: Option<&str>, _: &str| match finding {
        Some(_) => Ok(()),
        None => Err("names no finding.".to_owned()),
    };
    let done = apply_named(
        &two_sections(),
        json!({"edits":[
            {"path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."},
            {"finding":"f1","path":"/blocks/1/body","before":"Delta ends it.","after":"Delta closes it."}
        ]}),
        &wants_one,
    )?;
    assert_eq!(done.value["blocks"][0], two_sections()["blocks"][0]);
    assert_eq!(
        done.value["blocks"][1]["body"],
        "Gamma stands here. Delta closes it."
    );
    assert_eq!(done.named.len(), 1);
    Ok(())
}

#[test]
fn a_citation_and_a_withdrawn_citation_are_refused_outside_their_findings_part() -> Result<()> {
    let cite = |path: &str, sentence: &str| json!({"finding":"f1","path":path,"sentence":sentence,"sourceIndex":0,"quote":"A timeout of 30 seconds applies to each request."});
    let done = apply_named(
        &cited_lesson(),
        json!({"edits":[],"citations":[
            cite("/blocks/0", TIMEOUT),
            cite("/questions/0", "The cache holds 128 entries.")
        ]}),
        &only_f1_in_block_0,
    )?;
    assert_eq!(
        done.value["blocks"][0]["citations"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(done.value["questions"][0]["citations"], json!([]));
    assert_eq!(
        done.named["f1"].applied,
        BTreeSet::from(["/blocks/0/citations".to_owned()])
    );
    assert_eq!(
        done.named["f1"].problems,
        ["citation 1 is outside its finding's part."]
    );

    let mut held = cited_lesson();
    held["blocks"][0]["citations"] = json!([{"sentence":TIMEOUT,"sourceIndex":0,"quote":"A timeout of 30 seconds applies to each request."}]);
    held["questions"][0]["citations"] = json!([{"sentence":"The cache holds 128 entries.","sourceIndex":0,"quote":"The cache holds at most 128 entries at a time."}]);
    let done = apply_named(
        &held,
        json!({"edits":[],"citations":[],"uncite":[
            {"finding":"f1","path":"/blocks/0/citations/0"},
            {"finding":"f1","path":"/questions/0/citations/0"}
        ]}),
        &only_f1_in_block_0,
    )?;
    assert_eq!(done.value["blocks"][0]["citations"], json!([]));
    assert_eq!(
        done.value["questions"][0]["citations"],
        held["questions"][0]["citations"]
    );
    assert_eq!(
        done.named["f1"].problems,
        ["uncite 1 is outside its finding's part."]
    );
    Ok(())
}

#[test]
fn the_schema_for_findings_names_the_finding_of_every_change() {
    let mut original = cited_lesson();
    original["blocks"][0]["citations"] = json!([{"sentence":TIMEOUT,"sourceIndex":0,"quote":"A timeout of 30 seconds applies to each request."}]);
    let lesson = crate::generation::lesson_schema(2);
    let ids = vec!["f1".to_owned(), "f2".to_owned()];

    let named = schema_for_findings(&original, &lesson, &ids);
    let edit = &named["properties"]["edits"]["items"];
    assert_eq!(
        edit["properties"]["finding"],
        json!({"type":"string","enum":ids})
    );
    assert_eq!(
        edit["required"],
        json!(["finding", "path", "before", "after"])
    );
    let cite = &named["properties"]["citations"]["items"];
    assert_eq!(cite["properties"]["finding"]["enum"], json!(["f1", "f2"]));
    assert_eq!(
        cite["required"],
        json!(["finding", "path", "sentence", "sourceIndex", "quote"])
    );
    assert_eq!(
        named["properties"]["uncite"]["items"]["required"],
        json!(["finding", "path"])
    );
    assert!(named["properties"]["citations"].get("maxItems").is_none());
    assert!(named["properties"]["uncite"].get("uniqueItems").is_none());

    let plain = schema_for(&original, &lesson);
    assert!(plain["properties"]["edits"]["items"]["properties"]
        .get("finding")
        .is_none());
    assert_eq!(
        plain["properties"]["uncite"]["items"],
        json!({"type":"string","enum":["/blocks/0/citations/0"]})
    );
    assert!(plain["properties"]["citations"].get("maxItems").is_none());

    let alone = schema_for_findings(&original, &json!({"type":"object"}), &ids);
    assert_eq!(alone["required"], json!(["edits"]));
    assert_eq!(
        alone["properties"]["edits"]["items"]["properties"]["finding"]["enum"],
        json!(["f1", "f2"])
    );
}

#[test]
fn a_group_refused_only_for_overlapping_another_findings_edit_is_deferred() -> Result<()> {
    let done = apply_named(
        &two_sections(),
        json!({"edits":[
            {"finding":"f1","path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."},
            {"finding":"f2","path":"/blocks/0/body","before":"is second.","after":"is last."}
        ]}),
        &anything,
    )?;
    assert!(done.named["f2"].deferred);
    assert!(done.named["f2"].applied.is_empty());
    assert!(!done.named["f1"].deferred);
    assert!(!done.named["f1"].applied.is_empty());
    Ok(())
}

#[test]
fn a_group_refused_for_any_other_reason_is_not_deferred() -> Result<()> {
    let done = apply_named(
        &two_sections(),
        json!({"edits":[
            {"finding":"f1","path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."},
            {"finding":"f2","path":"/blocks/1/body","before":"Delta ends it.","after":"Delta closes it."}
        ]}),
        &only_f1_in_block_0,
    )?;
    assert!(!done.named["f2"].deferred);
    assert!(!done.named["f1"].deferred);
    Ok(())
}

#[test]
fn a_finding_refused_for_overlap_and_for_another_reason_is_not_deferred() -> Result<()> {
    let not_f2_in_block_1 = |finding: Option<&str>, part: &str| {
        if finding == Some("f2") && part == "/blocks/1" {
            Err("is outside its finding's part.".to_owned())
        } else {
            Ok(())
        }
    };
    let done = apply_named(
        &two_sections(),
        json!({"edits":[
            {"finding":"f1","path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."},
            {"finding":"f2","path":"/blocks/0/body","before":"is second.","after":"is last."},
            {"finding":"f2","path":"/blocks/1/body","before":"Delta ends it.","after":"Delta closes it."}
        ]}),
        &not_f2_in_block_1,
    )?;
    assert_eq!(done.named["f2"].problems.len(), 2);
    assert!(!done.named["f2"].deferred);
    Ok(())
}
