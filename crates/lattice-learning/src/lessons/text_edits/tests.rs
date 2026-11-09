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
        {"kind":"explanation","body":"Alpha is first. Beta is second."},
        {"kind":"explanation","body":"Gamma stands here. Delta ends it."}
    ],"questions":[]})
}

#[test]
fn an_edit_adding_a_sentence_another_section_has_is_rejected_and_the_field_is_unchanged() {
    let original = two_sections();
    let repeat = json!({"path":"/blocks/1/body","before":"Delta ends it.","after":"Delta ends it. Alpha is first."});
    let alone = apply(&original, &json!({"edits":[repeat]}).to_string());
    assert!(alone.is_err(), "the sentence is in section 1 already");
    assert!(
        alone.unwrap_err().to_string().contains("Alpha is first."),
        "the reason names the sentence"
    );
    let good =
        json!({"path":"/blocks/0/body","before":"Beta is second.","after":"Beta comes second."});
    let mixed = apply(&original, &json!({"edits":[repeat, good]}).to_string()).unwrap();
    assert_eq!(mixed["blocks"][1], original["blocks"][1]);
    assert_eq!(
        mixed["blocks"][0]["body"],
        "Alpha is first. Beta comes second."
    );
}

fn cited_sections() -> Value {
    json!({"blocks":[
        {"kind":"explanation","body":"Alpha is first. Beta is second."},
        {"kind":"explanation","body":"Gamma stands here. Delta ends it.","citations":[{"sentence":"Gamma stands here.","sourceIndex":0,"quote":"Gamma is stated by the source."}]}
    ],"questions":[]})
}

#[test]
fn a_citation_sentence_may_equal_a_sentence_the_lesson_has() {
    let original = cited_sections();
    let body =
        json!({"path":"/blocks/1/body","before":"Delta ends it.","after":"Delta closes it."});
    let cite = json!({"path":"/blocks/1/citations/0/sentence","before":"Gamma stands here.","after":"Alpha is first."});
    let repaired = apply_in(
        &original,
        &json!({"edits":[body, cite]}).to_string(),
        &original,
    )
    .unwrap();
    assert_eq!(
        repaired["blocks"][1]["citations"][0]["sentence"],
        "Alpha is first."
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
    let repeat = json!({"path":"/blocks/0/body","before":"Beta is second.","after":"Beta is second. Alpha is first."});
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
        "Alpha is first. Beta is the second one."
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
