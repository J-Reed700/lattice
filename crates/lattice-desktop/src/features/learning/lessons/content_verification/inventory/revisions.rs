//! Carry a saved inventory across an edit, never asking for a fresh list
//! merely because one sentence in the section was corrected: the claims on the
//! sentences that stand are kept as they are, those on the sentences the edit
//! rewrote or cut leave, and the text the edit wrote is extracted from its own
//! words, beside the rest of the section as context. No saved statement is
//! shown to a model, so a rewritten sentence is never read as the sentence it
//! replaced. Coverage of the entire changed section is still audited
//! independently before fact checking.
use super::*;

mod locations;

/// Why a section whose every claim was taken out has none, when what the edit
/// wrote has none.
const ALL_TAKEN_OUT: &str =
    "Every statement this section made was taken out of it, and what was written since states none.";

/// What the extractor is told of the rest of a section whose changed text it
/// reads.
const REST_OF_THE_SECTION: &str = "The rest of this section, already inventoried: the text around the passages that did not change and, in a question, its stem and options. It is for reading the passages by. It is not a passage and no claim is located in it: never extract an option as a fact. The selected answer is correctIndex.";

#[derive(Serialize, Deserialize)]
struct Basis {
    content: Value,
    inventory: UnitClaims,
}

fn key(llm: &dyn LLMPort, index: usize) -> String {
    format!("inventory-basis-v1:{}", digest(&json!({"policy":INVENTORY_POLICY,"model":llm.model_name(),"context":llm.max_context_tokens(),"index":index}).to_string()))
}

/// Whether `now` is the section `before` was: of its kind and role, under its
/// title or prompt. A basis is saved by index, and once a section ahead of it
/// goes another section can sit at that index.
fn same_section(before: &Value, now: &Value) -> bool {
    let name = |section: &Value| {
        (
            section.get("kind").cloned(),
            section.get("role").cloned(),
            section
                .get("title")
                .or_else(|| section.get("prompt"))
                .cloned(),
        )
    };
    name(before) == name(now)
}

/// Moves the saved inventory of each of a lesson's `units` to the index
/// `after` gives its section once the sections that go are out (claim
/// classes design §3.10), so the next pass revises each section against its
/// own. Indexes only come down, so each basis is read before another is
/// written over it.
pub(in crate::features::learning::lessons::content_verification) async fn renumber(
    llm: &dyn LLMPort,
    units: usize,
    after: impl Fn(usize) -> Option<usize>,
) -> Result<()> {
    for index in 0..units {
        let Some(now) = after(index).filter(|now| *now != index) else {
            continue;
        };
        let Some(mut basis) =
            crate::features::learning::lesson_drafts::checkpoint(&key(llm, index))
                .await?
                .and_then(|v| serde_json::from_value::<Basis>(v).ok())
        else {
            continue;
        };
        basis.inventory.index = now;
        crate::features::learning::lesson_drafts::record_checkpoint(
            &key(llm, now),
            serde_json::to_value(basis)?,
        )
        .await?;
    }
    Ok(())
}

pub(in crate::features::learning::lessons::content_verification) async fn save(
    llm: &dyn LLMPort,
    content: &[Value],
    unit: &UnitClaims,
) -> Result<()> {
    if let Some(content) = content.get(unit.index) {
        crate::features::learning::lesson_drafts::record_checkpoint(
            &key(llm, unit.index),
            serde_json::to_value(Basis {
                content: content.clone(),
                inventory: unit.clone(),
            })?,
        )
        .await?;
    }
    Ok(())
}

/// The extractor's input for what an edit wrote into section `index`: `whole`
/// is the section's input as a full extraction would give it; its passages
/// become the spans written, cut into passages of their own, and the rest of
/// the section, with a question and its options, is its context.
fn written_input(
    (index, whole, section): (usize, &Value, &Value),
    changed: &[locations::Changed],
) -> Value {
    let mut passages: Vec<Value> = Vec::new();
    let mut context: Vec<Value> = whole["context"].as_array().cloned().unwrap_or_default();
    for (field, text) in locations::fields(section) {
        let spans = changed
            .iter()
            .find(|written| written.field == field)
            .map(|written| written.spans.as_slice())
            .unwrap_or_default();
        let mut from = 0;
        for span in spans {
            rest(
                &mut context,
                (index, field.as_str()),
                text.get(from..span.start),
            );
            let written = text.get(span.clone()).unwrap_or_default();
            for piece in partition(written, &Prose, &Chars, &PASSAGE_PLAN) {
                let id = format!("unit-{index}-change-{}", passages.len());
                passages.push(
                    json!({"id":id,"field":field,"text":written.get(piece).unwrap_or_default()}),
                );
            }
            from = span.end;
        }
        rest(&mut context, (index, field.as_str()), text.get(from..));
    }
    let mut input = whole.clone();
    if let Some(object) = input.as_object_mut() {
        object.insert("passages".into(), Value::Array(passages));
        if !context.is_empty() {
            object.insert("context".into(), Value::Array(context));
            object.insert("contextRole".into(), json!(REST_OF_THE_SECTION));
        }
    }
    input
}

/// Adds the unchanged `text` of `field` to `context`, trimmed, when it says
/// anything.
fn rest(context: &mut Vec<Value>, (index, field): (usize, &str), text: Option<&str>) {
    let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else {
        return;
    };
    let id = format!("unit-{index}-context-{}", context.len());
    context.push(json!({"id":id,"field":field,"text":text}));
}

/// Section `index`'s inventory carried across an edit, when a saved basis of
/// the same section can be: its claims on the sentences that stand as they
/// were, and the claims the extractor reads from the text the edit wrote,
/// which it is shown beside the rest of the section and never beside a saved
/// statement. `None` when the section has no basis to carry; it is extracted
/// whole.
pub(in crate::features::learning::lessons::content_verification) async fn update(
    llm: &dyn LLMPort,
    content: &[Value],
    index: usize,
) -> Result<Option<UnitClaims>> {
    let Some(section) = content.get(index) else {
        return Ok(None);
    };
    let Some(before) = crate::features::learning::lesson_drafts::checkpoint(&key(llm, index))
        .await?
        .and_then(|v| serde_json::from_value::<Basis>(v).ok())
    else {
        return Ok(None);
    };
    if !same_section(&before.content, section) {
        return Ok(None);
    }
    if before.content == *section {
        return Ok(Some(before.inventory));
    }
    // The delta is relative to a specific inventory, not just its model/index.
    // A later repair may revisit the same text with a different saved basis.
    let delta_key = format!(
        "inventory-delta-v1:{}",
        digest(&json!({"model":key(llm,index),"basis":before,"content":section}).to_string())
    );
    if let Some(saved) = crate::features::learning::lesson_drafts::checkpoint(&delta_key)
        .await?
        .and_then(|v| serde_json::from_value::<UnitClaims>(v).ok())
    {
        return Ok(Some(saved));
    }
    let mut input = inputs(content);
    let Some(whole) = input
        .get(index)
        .filter(|unit| unit["passages"].as_array().is_some_and(|p| !p.is_empty()))
        .cloned()
    else {
        return Ok(None);
    };
    let locations::Relocated {
        inventory: mut updated,
        changed,
    } = locations::relocate(&before, section);
    let mut given = String::new();
    if !changed.is_empty() {
        if let Some(slot) = input.get_mut(index) {
            *slot = written_input((index, &whole, section), &changed);
        }
        let mut asked = BTreeMap::new();
        ask(
            llm,
            (content, input.as_slice()),
            &[index],
            &mut asked,
            false,
        )
        .await?;
        let fresh = asked
            .remove(&index)
            .ok_or_else(|| invalid("The text an edit wrote was extracted without an inventory."))?;
        updated.claims.extend(fresh.claims);
        given = fresh.non_factual_reason;
    }
    if !given.trim().is_empty() {
        updated.non_factual_reason = given;
    } else if updated.claims.is_empty() && !before.inventory.claims.is_empty() {
        updated.non_factual_reason = ALL_TAKEN_OUT.into();
    }
    if updated.claims.is_empty() && updated.non_factual_reason.trim().is_empty() {
        return Ok(None);
    }
    crate::features::learning::lesson_drafts::record_checkpoint(
        &delta_key,
        serde_json::to_value(&updated)?,
    )
    .await?;
    Ok(Some(updated))
}

#[cfg(test)]
pub(in crate::features::learning) async fn live_revision_fixture(
    llm: &dyn LLMPort,
    basis: Value,
    candidate: Value,
) -> Result<Value> {
    let before: Basis = serde_json::from_value(basis)?;
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) =
        crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(&pool)
            .await?;
    crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
        let index = before.inventory.index;
        let original = before.inventory.clone();
        crate::features::learning::lesson_drafts::record_checkpoint(&key(llm,index),serde_json::to_value(before)?).await?;
        let content = units(&candidate)?;
        let revised = update(llm, &content, index).await?.ok_or_else(||invalid("Missing live revision"))?;
        let changed = revised.claims.iter().filter(|claim| !original.claims.iter().any(|old|old.statement == claim.statement)).count();
        let result = json!({"originalCount":original.claims.len(),"revisedCount":revised.claims.len(),"changedStatements":changed,"inventory":revised});
        // Reopening the same revision must consume its saved result.
        let resumed = update(llm,&content,index).await?.ok_or_else(||invalid("Missing resumed revision"))?;
        if result["inventory"] != serde_json::to_value(resumed)? {return Err(invalid("Revision changed on resume"));}
        Ok(result)
    }).await
}

#[cfg(test)]
mod capacity_tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn one_revised_fact_does_not_drop_a_dense_sections_unchanged_claims() {
        let sentences: Vec<String> = (0..64)
            .map(|i| {
                let (tens, ones) = (b'a' + (i / 8) as u8, b'a' + (i % 8) as u8);
                format!(
                    "Sample {}{} has its value.",
                    char::from(tens),
                    char::from(ones)
                )
            })
            .collect();
        let before = Basis {
            content: json!({"body":sentences.join(" ")}),
            inventory: UnitClaims {
                index: 0,
                non_factual_reason: String::new(),
                claims: sentences
                    .iter()
                    .map(|sentence| Claim {
                        quote: sentence.clone(),
                        statement: sentence.clone(),
                    })
                    .collect(),
            },
        };
        let mut revised = sentences.clone();
        revised[63] = "Sample zz has its corrected value.".into();

        let carried = locations::relocate(&before, &json!({"body":revised.join(" ")}));

        assert_eq!(carried.inventory.claims.len(), 63);
        for (kept, saved) in carried
            .inventory
            .claims
            .iter()
            .zip(&before.inventory.claims)
        {
            assert_eq!(kept.statement, saved.statement);
            assert_eq!(kept.quote, saved.quote);
        }
        assert_eq!(carried.changed.len(), 1);
    }
}

#[cfg(test)]
mod revision_tests {
    use super::*;
    use crate::features::learning::content_verification::tests::{
        fixture_completion, fixture_prompts,
    };
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use std::sync::Mutex;

    fn claims_of(index: usize, sentence: &str) -> UnitClaims {
        UnitClaims {
            index,
            non_factual_reason: String::new(),
            claims: vec![Claim {
                quote: sentence.into(),
                statement: sentence.into(),
            }],
        }
    }

    fn section(title: &str, body: &str) -> Value {
        json!({"kind":"teaching","title":title,"body":body})
    }

    #[tokio::test]
    async fn a_saved_inventory_is_no_basis_for_another_section_at_its_index() -> Result<()> {
        // Once the section ahead of it goes, the next section sits at its
        // index: it is read afresh, never as an edit of the one that went.
        let before = vec![
            section("Alpha", "Alpha holds the line."),
            section("Bravo", "Bravo stands firm today."),
        ];
        let after = vec![
            section("Alpha", "Alpha holds the line."),
            section("Charlie", "Charlie rests in the shade."),
        ];
        let model = Answering {
            reply: String::new(),
            asked: Mutex::new(Vec::new()),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            save(&model, &before, &claims_of(1, "Bravo stands firm today.")).await?;

            assert!(update(&model, &after, 1).await?.is_none());
            assert!(model.asked.lock().unwrap().is_empty());
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn a_sections_saved_inventory_moves_with_it_when_a_section_ahead_goes() -> Result<()> {
        let before = vec![
            section("Alpha", "Alpha holds the line."),
            section("Bravo", "Bravo stands firm today."),
            section("Charlie", "Charlie rests in the shade."),
        ];
        let after = vec![
            section("Alpha", "Alpha holds the line."),
            section("Charlie", "Charlie rests in the shade."),
        ];
        let model = Answering {
            reply: String::new(),
            asked: Mutex::new(Vec::new()),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            for (index, sentence) in [
                "Alpha holds the line.",
                "Bravo stands firm today.",
                "Charlie rests in the shade.",
            ]
            .into_iter()
            .enumerate()
            {
                save(&model, &before, &claims_of(index, sentence)).await?;
            }

            let places = [Some(0), None, Some(1)];
            renumber(&model, before.len(), |unit| {
                places.get(unit).copied().flatten()
            })
            .await?;

            let moved = update(&model, &after, 1).await?.unwrap();
            let kept: Vec<_> = moved
                .claims
                .iter()
                .map(|claim| (claim.quote.as_str(), claim.statement.as_str()))
                .collect();
            assert_eq!(moved.index, 1);
            assert_eq!(
                kept,
                [("Charlie rests in the shade.", "Charlie rests in the shade.")]
            );
            assert!(model.asked.lock().unwrap().is_empty());
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn a_section_whose_every_claim_was_cut_says_why_it_has_none_and_nothing_is_asked(
    ) -> Result<()> {
        let unit = UnitClaims {
            index: 0,
            non_factual_reason: String::new(),
            claims: vec![Claim {
                quote: "Bravo stands firm today.".into(),
                statement: "Bravo stands firm today.".into(),
            }],
        };
        let before = vec![
            json!({"kind":"teaching","body":"Alpha holds the line. Bravo stands firm today."}),
        ];
        // The only claim's sentence is cut, and nothing is written.
        let after = vec![json!({"kind":"teaching","body":"Alpha holds the line."})];
        let model = Answering {
            reply: String::new(),
            asked: Mutex::new(Vec::new()),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            save(&model, &before, &unit).await?;
            let updated = update(&model, &after, 0).await?.unwrap();

            assert!(updated.claims.is_empty(), "{:?}", updated.claims);
            assert!(!updated.non_factual_reason.trim().is_empty());
            assert!(model.asked.lock().unwrap().is_empty());
            Ok(())
        })
        .await
    }

    const ALPHA: &str = "Alpha holds the line.";
    const BRAVO: &str = "Bravo stands firm today.";
    const CHARLIE: &str = "Charlie rests in the shade.";

    /// A model that answers every request with `reply` and keeps the prompt
    /// and the schema of each.
    struct Answering {
        reply: String,
        asked: Mutex<Vec<(String, Value)>>,
    }

    #[async_trait::async_trait]
    impl LLMPort for Answering {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            self.asked.lock().unwrap().push((
                fixture_prompts(request).concat(),
                request.json_schema.clone().unwrap_or(Value::Null),
            ));
            Ok(fixture_completion(request, vec![self.reply.clone()]))
        }
        fn model_name(&self) -> &str {
            "answering-fixture"
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }
        fn max_context_tokens(&self) -> usize {
            128_000
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    /// The reply of an extractor that finds `statement` in passage
    /// `unit-0-change-0`.
    fn finds(statement: &str) -> String {
        fixture_response(vec![
            json!({"index":0,"claims":[{"passageId":"unit-0-change-0","statement":statement}],"nonFactualReason":""}),
        ])
        .to_string()
    }

    /// The inventory of section 0 as it was saved: a claim for each of
    /// `saved`, a sentence and what it states.
    fn holding(saved: &[(&str, &str)]) -> UnitClaims {
        UnitClaims {
            index: 0,
            non_factual_reason: String::new(),
            claims: saved
                .iter()
                .map(|(sentence, statement)| Claim {
                    quote: (*sentence).into(),
                    statement: (*statement).into(),
                })
                .collect(),
        }
    }

    /// The quote and the statement of each claim of `unit`.
    fn left(unit: &UnitClaims) -> Vec<(&str, &str)> {
        unit.claims
            .iter()
            .map(|claim| (claim.quote.as_str(), claim.statement.as_str()))
            .collect()
    }

    /// The texts of `parts`, the passages or the context of a unit.
    fn texts(parts: &Value) -> Vec<&str> {
        parts
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|part| part["text"].as_str())
            .collect()
    }

    #[tokio::test]
    async fn a_rewritten_sentence_is_read_afresh_from_its_own_text_beside_the_sentences_that_stand(
    ) -> Result<()> {
        let saved = [
            (ALPHA, "Alpha is steady."),
            (BRAVO, "Bravo is firm."),
            (CHARLIE, "Charlie is at ease."),
        ];
        let before = vec![json!({"kind":"teaching","body":[ALPHA, BRAVO, CHARLIE].join(" ")})];
        // The second sentence is rewritten.
        let rewritten = "Bravo yields at noon.";
        let after =
            vec![json!({"kind":"teaching","body":format!("{ALPHA} {rewritten} {CHARLIE}")})];
        let model = Answering {
            reply: finds("Bravo gives way at noon."),
            asked: Mutex::new(Vec::new()),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            save(&model, &before, &holding(&saved)).await?;
            let updated = update(&model, &after, 0).await?.unwrap();

            let asked = model.asked.lock().unwrap();
            let [(prompt, _)] = asked.as_slice() else {
                panic!("one request: {asked:?}");
            };
            assert!(
                prompt.starts_with("Extract lesson claims."),
                "a rewritten sentence is extracted, not revised from its saved statement: {}",
                prompt.chars().take(60).collect::<String>()
            );
            let shown: Value = serde_json::from_str(prompt.rsplit("\n\n").next().unwrap())?;
            let unit = &shown["units"][0];
            // The request carries the rewritten sentence alone as a passage,
            // and the two that stand as context.
            assert_eq!(texts(&unit["passages"]), [rewritten]);
            assert_eq!(unit["passages"][0]["id"], "unit-0-change-0");
            assert_eq!(texts(&unit["context"]), [ALPHA, CHARLIE]);
            for (_, statement) in saved {
                assert!(!prompt.contains(statement), "{statement} was shown");
            }
            // The claims of the sentences that stand are as they were saved,
            // and the sentence the edit wrote has the claim read from it.
            assert_eq!(
                left(&updated),
                [
                    (ALPHA, "Alpha is steady."),
                    (CHARLIE, "Charlie is at ease."),
                    (rewritten, "Bravo gives way at noon.")
                ]
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn a_second_edit_is_read_from_the_text_it_wrote_and_the_first_edits_sentence_is_carried(
    ) -> Result<()> {
        let saved = [
            (ALPHA, "Alpha is steady."),
            (BRAVO, "Bravo is firm."),
            (CHARLIE, "Charlie is at ease."),
        ];
        let before = vec![json!({"kind":"teaching","body":[ALPHA, BRAVO, CHARLIE].join(" ")})];
        // A repair rewrites the first sentence, and a later one the third.
        let (first, third) = ("Alpha gives ground at dawn.", "Charlie sits in the sun.");
        let once = vec![json!({"kind":"teaching","body":format!("{first} {BRAVO} {CHARLIE}")})];
        let twice = vec![json!({"kind":"teaching","body":format!("{first} {BRAVO} {third}")})];
        let model = Answering {
            reply: finds("A sentence the edit wrote states this."),
            asked: Mutex::new(Vec::new()),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            save(&model, &before, &holding(&saved)).await?;
            update(&model, &once, 0).await?.unwrap();
            update(&model, &twice, 0).await?.unwrap();

            let asked = model.asked.lock().unwrap();
            let [_, (prompt, _)] = asked.as_slice() else {
                panic!("one request for each edit: {asked:?}");
            };
            let shown: Value = serde_json::from_str(prompt.rsplit("\n\n").next().unwrap())?;
            // The first edit's sentence was read once, after the first edit;
            // the second edit's request reads only what it wrote.
            assert_eq!(texts(&shown["units"][0]["passages"]), [third]);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn an_inserted_sentence_is_extracted_alone_and_the_claims_beside_it_are_carried_with_no_request_about_them(
    ) -> Result<()> {
        let saved = [
            (ALPHA, "Alpha is steady."),
            (CHARLIE, "Charlie is at ease."),
        ];
        let before = vec![json!({"kind":"teaching","body":[ALPHA, CHARLIE].join(" ")})];
        let after = vec![json!({"kind":"teaching","body":format!("{ALPHA} {BRAVO} {CHARLIE}")})];
        let model = Answering {
            reply: finds("Bravo is firm."),
            asked: Mutex::new(Vec::new()),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            save(&model, &before, &holding(&saved)).await?;
            let updated = update(&model, &after, 0).await?.unwrap();

            let asked = model.asked.lock().unwrap();
            let [(prompt, _)] = asked.as_slice() else {
                panic!("one request: {asked:?}");
            };
            for (_, statement) in saved {
                assert!(
                    !prompt.contains(statement),
                    "the request is about {statement}, a claim that stands"
                );
            }
            assert!(prompt.starts_with("Extract lesson claims."));
            let shown: Value = serde_json::from_str(prompt.rsplit("\n\n").next().unwrap())?;
            let unit = &shown["units"][0];
            assert_eq!(texts(&unit["passages"]), [BRAVO]);
            assert_eq!(texts(&unit["context"]), [ALPHA, CHARLIE]);
            assert_eq!(
                left(&updated),
                [
                    (ALPHA, "Alpha is steady."),
                    (CHARLIE, "Charlie is at ease."),
                    (BRAVO, "Bravo is firm.")
                ]
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn a_section_whose_claims_all_left_takes_the_reason_the_extraction_gives_for_what_was_written(
    ) -> Result<()> {
        let before = vec![json!({"kind":"teaching","body":BRAVO})];
        // The only sentence is rewritten into one that asserts nothing.
        let after = vec![json!({"kind":"teaching","body":"Try it yourself."})];
        let model = Answering {
            reply: fixture_response(vec![
                json!({"index":0,"claims":[],"nonFactualReason":"an exercise"}),
            ])
            .to_string(),
            asked: Mutex::new(Vec::new()),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            save(&model, &before, &holding(&[(BRAVO, "Bravo is firm.")])).await?;
            let updated = update(&model, &after, 0).await?.unwrap();

            assert!(updated.claims.is_empty(), "{:?}", updated.claims);
            assert_eq!(updated.non_factual_reason, "an exercise");
            assert_eq!(model.asked.lock().unwrap().len(), 1);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn an_updated_inventory_is_saved_and_a_resumed_job_asks_nothing_again() -> Result<()> {
        let saved = [(ALPHA, "Alpha is steady."), (BRAVO, "Bravo is firm.")];
        let before = vec![json!({"kind":"teaching","body":format!("{ALPHA} {BRAVO}")})];
        let after =
            vec![json!({"kind":"teaching","body":format!("{ALPHA} Bravo yields at noon.")})];
        let model = Answering {
            reply: finds("Bravo gives way at noon."),
            asked: Mutex::new(Vec::new()),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            save(&model, &before, &holding(&saved)).await?;
            let first = update(&model, &after, 0).await?.unwrap();
            let again = update(&model, &after, 0).await?.unwrap();

            assert_eq!(model.asked.lock().unwrap().len(), 1);
            assert_eq!(serde_json::to_value(&first)?, serde_json::to_value(&again)?);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn a_revision_the_old_prompt_saved_is_never_read() -> Result<()> {
        let saved = [(ALPHA, "Alpha is steady."), (BRAVO, "Bravo is firm.")];
        let before = vec![json!({"kind":"teaching","body":format!("{ALPHA} {BRAVO}")})];
        let after =
            vec![json!({"kind":"teaching","body":format!("{ALPHA} Bravo yields at noon.")})];
        let model = Answering {
            reply: finds("Bravo gives way at noon."),
            asked: Mutex::new(Vec::new()),
        };
        let basis = Basis {
            content: before[0].clone(),
            inventory: holding(&saved),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            save(&model, &before, &holding(&saved)).await?;
            // What the revision of the saved statements kept for this edit: the
            // statement of the sentence that was rewritten, on its new words.
            let kept = holding(&[
                (ALPHA, "Alpha is steady."),
                ("Bravo yields at noon.", "Bravo is firm."),
            ]);
            let old_key = format!(
                "inventory-revision-v2:{}",
                digest(
                    &json!({"model":key(&model,0),"basis":basis,"content":after[0]}).to_string()
                )
            );
            crate::features::learning::lesson_drafts::record_checkpoint(
                &old_key,
                serde_json::to_value(&kept)?,
            )
            .await?;

            let updated = update(&model, &after, 0).await?.unwrap();

            assert_eq!(model.asked.lock().unwrap().len(), 1);
            assert_eq!(
                left(&updated),
                [
                    (ALPHA, "Alpha is steady."),
                    ("Bravo yields at noon.", "Bravo gives way at noon.")
                ]
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn the_text_written_into_a_question_is_read_beside_its_stem_its_options_and_the_rest_of_its_explanation(
    ) -> Result<()> {
        let question = |explanation: &str| {
            json!({"kind":"assessment","correctIndex":1,"prompt":"Which result follows?",
                "options":["The first option.","The second option."],"explanation":explanation})
        };
        let (first, third) = ("First reason holds.", "Third reason holds.");
        let before = vec![question(&format!("{first} {third}"))];
        let after = vec![question(&format!("{first} Second reason is new. {third}"))];
        let model = Answering {
            reply: finds("Second reason is so."),
            asked: Mutex::new(Vec::new()),
        };
        let pool = crate::features::learning::tests::pool().await?;
        let (repo, job, lesson) =
            crate::features::learning::content_verification::tests::batch_scheduler_tests::setup(
                &pool,
            )
            .await?;
        crate::features::learning::lesson_drafts::run(&repo, &job, &lesson, async {
            let saved = holding(&[(first, "First is so."), (third, "Third is so.")]);
            save(&model, &before, &saved).await?;
            let updated = update(&model, &after, 0).await?.unwrap();

            let asked = model.asked.lock().unwrap();
            let [(prompt, schema)] = asked.as_slice() else {
                panic!("one request: {asked:?}");
            };
            let shown: Value = serde_json::from_str(prompt.rsplit("\n\n").next().unwrap())?;
            let unit = &shown["units"][0];
            assert_eq!(texts(&unit["passages"]), ["Second reason is new."]);
            assert_eq!(unit["passages"][0]["field"], "/explanation");
            // The stem and the options are context still, beside the rest of
            // the explanation, and the answer is told.
            assert_eq!(
                texts(&unit["context"]),
                [
                    "Which result follows?",
                    "The first option.",
                    "The second option.",
                    first,
                    third
                ]
            );
            let role = unit["contextRole"].as_str().unwrap_or_default();
            assert!(role.contains("already inventoried"), "{role}");
            assert_eq!(unit["correctIndex"], 1);
            // A claim can be located in what was written and nowhere else.
            assert_eq!(
                schema.pointer(
                    "/properties/units/properties/unit-0/properties/claims/items/properties/passageId/enum"
                ),
                Some(&json!(["unit-0-change-0"]))
            );
            assert_eq!(
                left(&updated),
                [
                    (first, "First is so."),
                    (third, "Third is so."),
                    ("Second reason is new.", "Second reason is so.")
                ]
            );
            Ok(())
        })
        .await
    }
}
