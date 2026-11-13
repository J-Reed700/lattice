//! The recite remedy (course evidence index §5.1): a claim a page of the
//! course supports on a passage its section does not cite keeps its text, and
//! the citation of its sentence moves to the passage found.
use super::evidence_paging_tests::{marker_collection, PagingModel, CLAIM, MARKER};
use super::readings_tests::{citing_the_wrong_passage, read_one, world_of, wrongly_cited};
use super::*;
use lattice_core::domain::learning::acceptance::Reading;

/// The citation events `cite_where_supported` made, one for each finding it
/// cited, with `kept` the live claims' sentences that are not being cited.
fn recited(
    candidate: &mut Value,
    findings: &[Finding],
    kept: &[(usize, String)],
    numbered: &mut Vec<LearningSourceDto>,
    references: &ReferenceCollection<'_>,
    tried: &mut HashSet<(usize, String, String)>,
) -> Result<Vec<lattice_core::domain::learning::claim_review::ClaimEvent>> {
    let cited =
        repair::cite_where_supported(candidate, findings, kept, numbered, references, tried)?;
    Ok(cited
        .into_iter()
        .filter_map(|cited| match cited {
            repair::Cited::Recited(event) => Some(*event),
            _ => None,
        })
        .collect())
}

/// The claim's finding when the section cites the wrong passage and the
/// course's page finds the marker's.
async fn supported_elsewhere(references: &ReferenceCollection<'_>) -> Result<Finding> {
    let model = PagingModel::new(32_000);
    let world = world_of(references, citing_the_wrong_passage(references)?);
    Ok(read_one(Reading::Page, &model, &world, CLAIM)
        .await?
        .finding()
        .clone())
}

#[tokio::test]
async fn a_claim_supported_elsewhere_has_its_citation_repaired() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = supported_elsewhere(&references).await?;
    assert_eq!(finding.verdict, ClaimVerdict::Supported);
    assert_eq!(finding.two_pass.pass, Some(Pass::Corroboration));
    assert_eq!(finding.two_pass.page, Some(1));
    let found = finding.two_pass.supported_by.clone().unwrap();
    assert_eq!(found.source_id, "marker");
    assert_eq!(found.cited, vec!["related-3".to_owned()]);

    // The lesson numbers both sources: related-3 as 0, the marker's as 1.
    let source = |id: &str| {
        sources
            .iter()
            .find(|source| source.id == id)
            .unwrap()
            .clone()
    };
    let mut numbered = vec![source("related-3"), source("marker")];
    let mut candidate = wrongly_cited();
    let mut tried = HashSet::new();
    let repairs = recited(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut numbered,
        &references,
        &mut tried,
    )?;
    assert_eq!(repairs.len(), 1);
    let block = &candidate["blocks"][0];
    assert_eq!(block["sourceIndex"], 0, "the section's own citation stands");
    assert_eq!(
        numbered.len(),
        2,
        "the marker's source was numbered already"
    );
    // The sentence is cited by an entry of its own, at the marker's source.
    assert_eq!(block["citations"][0]["sentence"], CLAIM);
    assert_eq!(block["citations"][0]["sourceIndex"], 1);
    let quote = block["citations"][0]["quote"].as_str().unwrap();
    assert!(source("marker").excerpt.contains(quote));
    assert_eq!(
        quote, "Mineral alpha notes 99: property was logged.",
        "the run of the found passage that best matches the claim"
    );
    let lattice_core::domain::learning::claim_review::ClaimEvent::CitationRepaired {
        cited,
        source_version_id,
        added_source,
        ..
    } = &repairs[0]
    else {
        panic!("{:?}", repairs[0]);
    };
    assert_eq!(cited, &vec!["related-3".to_owned()]);
    assert_eq!(source_version_id, "marker");
    assert!(!added_source);
    // The next pass reads it as the section's own citation: the citation's
    // page holds the passage the course's page found.
    let next = section_citations(&candidate, &references)?;
    assert!(next
        .of(0)
        .iter()
        .any(|passage| passage.source_id == "marker" && passage.text.contains(MARKER)));
    assert_eq!(next.source_at(1), Some("marker"));
    // Written once: the same citation is not written again.
    assert!(recited(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut numbered,
        &references,
        &mut tried,
    )?
    .is_empty());
    Ok(())
}

#[tokio::test]
async fn a_source_the_lesson_does_not_number_is_added_and_cited() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = supported_elsewhere(&references).await?;
    // The writer was given related-3 only.
    let related = sources
        .iter()
        .find(|source| source.id == "related-3")
        .unwrap();
    let mut numbered = vec![related.clone()];
    let mut candidate = wrongly_cited();
    let repairs = recited(
        &mut candidate,
        &[finding],
        &[],
        &mut numbered,
        &references,
        &mut HashSet::new(),
    )?;
    // The marker's source joins the lesson's sources, with its title and
    // URL as the writer's are, and the section cites it for the sentence.
    assert_eq!(numbered.len(), 2);
    assert_eq!(numbered[1].id, "marker");
    assert_eq!(numbered[1].title, "Reference marker");
    assert_eq!(candidate["blocks"][0]["citations"][0]["sourceIndex"], 1);
    assert!(matches!(
        repairs[0],
        lattice_core::domain::learning::claim_review::ClaimEvent::CitationRepaired {
            added_source: true,
            ..
        }
    ));
    // A lesson schema's source indices then allow the new source.
    let policy = crate::lessons::shape::policy();
    let schema = policy
        .schema(crate::lessons::shape::Phase::Kept, 1)
        .numbered(policy, numbered.len());
    assert_eq!(
        schema.value()["properties"]["blocks"]["items"]["properties"]["sourceIndex"]["maximum"],
        1
    );
    Ok(())
}

#[test]
fn the_repaired_quote_is_the_run_of_sentences_that_best_matches_the_claim() {
    let filler: String = (0..40)
        .map(|line| format!("Archive line {line} is filed away. "))
        .collect();
    let stated = "Mineral alpha shows property beta under pressure in the lab.";
    let passage = format!("{filler}{stated} A closing remark follows.");
    let quote = repair::quotation(CLAIM, &passage, 0..passage.len()).unwrap();
    assert_eq!(quote, stated, "the matching sentence near the end, alone");
    assert!(quote.chars().count() <= crate::generation::MAX_QUOTE_CHARS);
    // Two runs that match as well: the earliest.
    let twice = format!("{stated} Then a pause comes here. {stated}");
    let quote = repair::quotation(CLAIM, &twice, 0..twice.len()).unwrap();
    assert_eq!(twice.find(quote), Some(0));
    // No run of whole sentences short enough to cite: none.
    let endless = format!("{} pressure", "mineral alpha ".repeat(200));
    assert_eq!(repair::quotation(CLAIM, &endless, 0..endless.len()), None);
}

/// A study's words are in the prose that discusses it and its figures in a
/// table further on than verification reads around a citation. The check
/// that decides the repair reads the citation's passage, so the citation is
/// one whose passage shows the claim's figures.
#[test]
fn a_repaired_citation_shows_its_check_the_figures_the_claim_states() -> Result<()> {
    let claim = "In a study of stance classification, tuned models scored an F1 of 0.94 while zero-shot models scored only 0.51.";
    let prose = "This study compared tuned models with zero-shot models on stance classification, and the tuned models scored higher on F1 in every comparison.";
    let filler: String = (0..40)
        .map(|line| format!("Kiln log {line} records glaze batches and cooling ramps. "))
        .collect();
    let table = [
        "Table 2: Stance classification results",
        "",
        "| Model | Accuracy | F1 |",
        "|---|---|---|",
        "| Encoder-A | 0.95 | 0.94 |",
        "| Encoder-B | 0.91 | 0.90 |",
        "| Zero-shot-A | 0.58 | 0.51 |",
    ]
    .join("\n");
    let text = format!("{prose}\n\n{filler}\n\n{table}\n\n{filler}");
    let references = ReferenceCollection::lexical(&[LearningSourceDto {
        id: "study".into(),
        title: "Stance classification study".into(),
        url: None,
        excerpt: text.clone(),
        acquired_at: 0,
    }])?;

    let quote = repair::quotation(claim, &text, 0..text.len()).unwrap();
    assert!(quote.chars().count() <= crate::generation::MAX_QUOTE_CHARS);
    let start = text.find(quote).unwrap();
    let read: String = references
        .source_passages("study", &[(start, start + quote.len())])?
        .iter()
        .map(|passage| passage.text.as_str())
        .collect();
    for figure in ["0.94", "0.51"] {
        assert!(
            read.contains(figure),
            "the check of {quote:?} cannot read {figure}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_claim_supported_elsewhere_has_the_citation_of_its_own_sentence_repaired() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = supported_elsewhere(&references).await?;
    let source = |id: &str| {
        sources
            .iter()
            .find(|source| source.id == id)
            .unwrap()
            .clone()
    };
    let mut numbered = vec![source("related-3"), source("marker")];
    let mut candidate = wrongly_cited();
    let wrong = candidate["blocks"][0]["quote"].clone();
    candidate["blocks"][0]["citations"] = json!([{"sentence":CLAIM,"sourceIndex":0,"quote":wrong}]);

    let repairs = recited(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut numbered,
        &references,
        &mut HashSet::new(),
    )?;

    assert_eq!(repairs.len(), 1);
    let block = &candidate["blocks"][0];
    assert_eq!(block["sourceIndex"], 0, "the section's own citation stands");
    assert_eq!(block["quote"], wrong);
    assert_eq!(block["citations"][0]["sourceIndex"], 1);
    assert_eq!(
        block["citations"][0]["quote"],
        "Mineral alpha notes 99: property was logged."
    );
    // The next pass reads the claim against the passage found.
    let next = section_citations(&candidate, &references)?;
    let read = next.of_claim(0, CLAIM);
    assert!(!read.is_empty() && read.iter().all(|passage| passage.source_id == "marker"));
    Ok(())
}

/// The lesson's numbered sources: the related one the section cites, as 0,
/// and the marker's, as 1.
fn numbered(sources: &[LearningSourceDto]) -> Vec<LearningSourceDto> {
    ["related-3", "marker"]
        .iter()
        .filter_map(|id| sources.iter().find(|source| source.id == *id).cloned())
        .collect()
}

/// The entry the recite writes for the claim's sentence: the marker's passage.
fn marker_entry() -> Value {
    json!({
        "sentence": CLAIM,
        "sourceIndex": 1,
        "quote": "Mineral alpha notes 99: property was logged.",
    })
}

#[tokio::test]
async fn a_statement_with_no_entry_of_its_own_leaves_its_sections_citation_and_gains_an_entry_for_its_sentence(
) -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = supported_elsewhere(&references).await?;
    let mut candidate = wrongly_cited();
    let section = candidate["blocks"][0].clone();

    let repairs = recited(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut numbered(&sources),
        &references,
        &mut HashSet::new(),
    )?;

    assert_eq!(repairs.len(), 1);
    let block = &candidate["blocks"][0];
    assert_eq!(
        block["quote"], section["quote"],
        "the quotation every other statement of the section is read against stands"
    );
    assert_eq!(block["sourceIndex"], section["sourceIndex"]);
    assert_eq!(block["citations"], json!([marker_entry()]));
    Ok(())
}

#[tokio::test]
async fn a_section_with_no_quotation_of_its_own_can_be_cited() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = supported_elsewhere(&references).await?;
    let mut candidate = json!({"blocks":[{"kind":"explanation","title":"Alpha","body":CLAIM,"rubric":[]}],"questions":[]});

    let repairs = recited(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut numbered(&sources),
        &references,
        &mut HashSet::new(),
    )?;

    assert_eq!(
        repairs.len(),
        1,
        "the sentence is cited though its section has no quotation"
    );
    let block = &candidate["blocks"][0];
    assert_eq!(block["citations"], json!([marker_entry()]));
    assert!(block.get("quote").is_none(), "no quotation is written");
    Ok(())
}

const NONE: &str = "Missing values in nonblank rows default to None.";

/// A sentence whose statement names context the sentence leaves implicit: the
/// sentence holds 8 of the statement's 22 terms, under the half the locator
/// asks for.
pub(super) const IMPLICIT_SENTENCE: &str = "Writing a new system erases the card, so back up first, then write the image again with remote login enabled in customisation.";
pub(super) const IMPLICIT_STATEMENT: &str = "Acme Installer lets users turn on remote login from its customisation screen, which is offered whenever a new image is written to the card.";

/// A lesson of one section that says `body` and cites an unrelated quotation.
fn csv_lesson(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Rows","body":body,"rubric":[],
        "sourceIndex":0,"quote":"An unrelated quotation."}],"questions":[]})
}

/// A finding that a page of the course supports `statement`, which the
/// section says in `quote`, on `sentence` of `source`.
fn found_in(
    source: &LearningSourceDto,
    sentence: &str,
    (quote, statement): (&str, &str),
) -> Finding {
    let at = source.excerpt.find(sentence).unwrap();
    Finding {
        unit: 0,
        quote: quote.into(),
        statement: statement.into(),
        verdict: ClaimVerdict::Supported,
        reason: "Stated elsewhere.".into(),
        evidence: vec![],
        supporting_quote: None,
        resolution: None,
        trust: None,
        label: None,
        class: None,
        claim_id: None,
        two_pass: TwoPass {
            supported_by: Some(SupportedElsewhere {
                source_id: source.id.clone(),
                start_byte: at,
                end_byte: at + sentence.len(),
                cited: vec![],
            }),
            ..Default::default()
        },
    }
}

#[test]
fn an_entry_a_live_statement_shares_stays_and_a_second_entry_for_the_sentence_is_appended(
) -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = found_in(&sources[0], GOOD, (GOOD, GOOD));
    let shared = json!({"sentence":GOOD,"sourceIndex":0,"quote":NONE});
    let own = json!({"sentence":GOOD,"sourceIndex":0,"quote":GOOD});
    let lesson = {
        let mut lesson = csv_lesson(GOOD);
        lesson["blocks"][0]["citations"] = json!([shared]);
        lesson
    };
    // Another live claim stands on the sentence, so it reads against that entry too.
    let kept = [(0, GOOD.to_owned())];

    let mut candidate = lesson.clone();
    let cited = recited(
        &mut candidate,
        std::slice::from_ref(&finding),
        &kept,
        &mut sources.to_vec(),
        &references,
        &mut HashSet::new(),
    )?;
    assert_eq!(cited.len(), 1);
    assert_eq!(
        candidate["blocks"][0]["citations"],
        json!([shared, own]),
        "the shared entry stays as it was, and the sentence gains one of its own"
    );

    // With no live statement beside it, the same entry moves.
    let mut candidate = lesson;
    recited(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut sources.to_vec(),
        &references,
        &mut HashSet::new(),
    )?;
    assert_eq!(candidate["blocks"][0]["citations"], json!([own]));
    Ok(())
}

#[test]
fn claims_on_two_sentences_of_a_section_are_both_cited_in_one_call() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let body = format!("{GOOD} {NONE}");
    let findings = [
        found_in(&sources[0], GOOD, (GOOD, GOOD)),
        found_in(&sources[0], NONE, (NONE, NONE)),
    ];
    let mut candidate = csv_lesson(&body);

    let cited = repair::cite_where_supported(
        &mut candidate,
        &findings,
        &[],
        &mut sources.to_vec(),
        &references,
        &mut HashSet::new(),
    )?;

    assert!(
        matches!(
            cited.as_slice(),
            [repair::Cited::Recited(_), repair::Cited::Recited(_)]
        ),
        "both are cited"
    );
    let block = &candidate["blocks"][0];
    assert_eq!(block["quote"], "An unrelated quotation.");
    assert_eq!(
        block["citations"],
        json!([
            {"sentence":GOOD,"sourceIndex":0,"quote":GOOD},
            {"sentence":NONE,"sourceIndex":0,"quote":NONE},
        ])
    );
    Ok(())
}

#[test]
fn a_second_claim_on_a_sentence_cited_in_the_call_is_deferred() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let findings = [
        found_in(&sources[0], GOOD, (GOOD, GOOD)),
        found_in(
            &sources[0],
            GOOD,
            (GOOD, "DictReader preserves whitespace by default."),
        ),
    ];
    let mut candidate = csv_lesson(GOOD);

    let cited = repair::cite_where_supported(
        &mut candidate,
        &findings,
        &[],
        &mut sources.to_vec(),
        &references,
        &mut HashSet::new(),
    )?;

    assert!(
        matches!(
            cited.as_slice(),
            [repair::Cited::Recited(_), repair::Cited::Deferred]
        ),
        "the second waits for the next round"
    );
    assert_eq!(
        candidate["blocks"][0]["citations"],
        json!([{"sentence":GOOD,"sourceIndex":0,"quote":GOOD}])
    );
    Ok(())
}

#[test]
fn a_claim_that_draws_on_several_sentences_of_its_quote_cannot_be_cited() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    // The claim's quote holds four sentences, so none of them is its own.
    let passage = "The Peace of Westphalia was signed in 1648 in the cities of Osnabrück and Münster. It ended the Thirty Years' War in the Holy Roman Empire. It also closed the Eighty Years' War between Spain and the Dutch Republic. Historians often date the modern order of sovereign states from these treaties.";
    let statement = "Signed in 1648, the Peace of Westphalia ended the Thirty Years' War and is often seen by historians as the start of the order of sovereign states.";
    let finding = found_in(&sources[0], GOOD, (passage, statement));
    let mut candidate = csv_lesson(passage);
    let before = candidate.clone();

    let cited = repair::cite_where_supported(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut sources.to_vec(),
        &references,
        &mut HashSet::new(),
    )?;

    assert!(matches!(
        cited.as_slice(),
        [repair::Cited::Cannot(repair::Unwritten::SeveralSentences)]
    ));
    assert_eq!(candidate, before);
    Ok(())
}

#[cfg(test)]
#[path = "recite_entry_tests.rs"]
mod entry_tests;
