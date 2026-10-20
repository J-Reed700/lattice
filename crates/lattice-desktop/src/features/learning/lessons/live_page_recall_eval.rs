//! Page recall (claim evidence pages design §9): for every claim a stored
//! lesson check approved, on which page the passage its judge cited reaches
//! the judge under the page rule, and how much the claim reads up to it.
//!
//! It reads a stored run, the way `scripts/lesson_claim_pairs.py extract`
//! does: `learning_lesson_verifications.report_json` for the findings and
//! the program's saved source versions for the collection, which it ranks by
//! keyword alone (`ReferenceCollection::lexical`). The database is opened
//! read-only. A section's citations are what the stored report marks
//! `cited`: a finding's own unit's, and across the lesson the sources they
//! come from. The old rule's page is the stored page that decided the claim:
//! its index is not stored, so only its size is reported beside the new
//! rule's page index. No model is asked; the judge is assumed to read each
//! page whole and settle nothing until the cited passage is on it.
//!
//! Given a pace (`LATTICE_PAGE_RECALL_PACE`), each finding's time is
//! modelled: its pages' tokens read at the prompt rate plus one answer per
//! page written at the output rate, and the page budget the app derives
//! from that pace is reported. Pages are not yet sized by it.
use crate::features::learning::reference_collection::{ReferenceCollection, ReferencePassage};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse, Pace};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use std::collections::HashMap;

/// Counts tokens by the calibrated estimate and holds the run's configured
/// window, the page's only bound. It is never asked to answer.
struct Counter {
    window: usize,
}

#[async_trait::async_trait]
impl LLMPort for Counter {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Err(AppError::InvalidState(
            "The page-recall eval makes no model request.".into(),
        ))
    }
    fn model_name(&self) -> &str {
        "page-recall-counter"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The program's saved source versions as a lesson check reads them, read
/// without the backfill the app runs, so the stored run is never written.
async fn program_sources(
    pool: &sqlx::SqlitePool,
    program_id: &str,
) -> Result<Vec<crate::features::learning::dto::LearningSourceDto>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT v.id, v.title, v.resolved_url, v.requested_url, v.full_text AS excerpt, v.acquired_at \
         FROM learning_source_library s \
         JOIN learning_source_versions v ON v.program_id = s.program_id AND v.id = s.active_version_id \
         WHERE s.program_id = ? AND s.deleted_at IS NULL ORDER BY s.created_at, s.id",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|row| crate::features::learning::dto::LearningSourceDto {
            id: row.get("id"),
            title: row.get("title"),
            url: row
                .get::<Option<String>, _>("resolved_url")
                .or_else(|| row.get("requested_url")),
            excerpt: row.get("excerpt"),
            acquired_at: row.get("acquired_at"),
        })
        .collect())
}

fn stored_passage(value: &serde_json::Value) -> Option<ReferencePassage> {
    serde_json::from_value(value.clone()).ok()
}

/// Whether `passage` is still the text at its place in a saved source.
fn still_saved(
    sources: &[crate::features::learning::dto::LearningSourceDto],
    passage: &ReferencePassage,
) -> bool {
    sources.iter().any(|source| {
        source.id == passage.source_id
            && source.excerpt.get(passage.start_byte..passage.end_byte)
                == Some(passage.text.as_str())
    })
}

/// `LATTICE_PAGE_RECALL_PACE`: "<prompt tokens/s>,<output tokens/s>,<answer
/// tokens per claim>", the backend's pace and the judge's answer size as a
/// run's log measured them.
fn pace_from_env() -> Option<(Pace, f64)> {
    let value = std::env::var("LATTICE_PAGE_RECALL_PACE").ok()?;
    let numbers: Vec<f64> = value
        .split(',')
        .map(|number| number.trim().parse().unwrap())
        .collect();
    let [prompt, output, answer] = numbers[..] else {
        panic!("LATTICE_PAGE_RECALL_PACE takes three numbers: prompt tokens/s, output tokens/s, answer tokens per claim");
    };
    Some((
        Pace {
            prompt_tokens_per_second: prompt,
            output_tokens_per_second: output,
        },
        answer,
    ))
}

/// Modelled seconds to read `tokens` over `pages` pages, each page answered
/// once: the reading at the prompt rate plus every answer at the output rate.
fn modelled_seconds((pace, answer): (Pace, f64), tokens: usize, pages: usize) -> f64 {
    tokens as f64 / pace.prompt_tokens_per_second
        + pages as f64 * answer / pace.output_tokens_per_second
}

fn gap(clear: Option<bool>) -> &'static str {
    match clear {
        Some(true) => "clear",
        Some(false) => "none",
        None => "-",
    }
}

#[tokio::test]
#[ignore = "requires LATTICE_PAGE_RECALL_DB, LATTICE_PAGE_RECALL_WINDOW and LATTICE_PAGE_RECALL_RESULTS; reads a stored run, makes no model request"]
async fn live_page_recall_eval() -> Result<()> {
    use std::io::Write as _;
    let database = std::env::var("LATTICE_PAGE_RECALL_DB").unwrap();
    // The configured window of the run being read: it bounds ring 0 only.
    let window: usize = std::env::var("LATTICE_PAGE_RECALL_WINDOW")
        .unwrap()
        .parse()
        .unwrap();
    let only_program = std::env::var("LATTICE_PAGE_RECALL_PROGRAM").ok();
    let pace = pace_from_env();
    let budget = pace.and_then(|(pace, answer)| {
        crate::features::learning::content_verification::page_budget(Some(pace), Some(answer))
    });
    let results_path = std::env::var("LATTICE_PAGE_RECALL_RESULTS").unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&database)
                .read_only(true),
        )
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    let llm = Counter { window };
    let tokens = |page: &[ReferencePassage]| {
        page.iter()
            .map(|passage| llm.count_tokens(&passage.text))
            .sum::<usize>()
    };
    let runs: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT lesson_id, program_id, report_json FROM learning_lesson_verifications ORDER BY program_id, lesson_id",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| AppError::Database(error.to_string()))?;
    let mut results = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&results_path)?;
    let mut collections: HashMap<String, Vec<_>> = HashMap::new();
    let (mut approved, mut skipped, mut off_subject, mut never) = (0, 0, 0, 0);
    let mut on_page: std::collections::BTreeMap<usize, usize> = Default::default();
    let (mut old_tokens, mut new_tokens) = (0usize, 0usize);
    let (mut old_seconds, mut new_seconds) = (0f64, 0f64);
    for (lesson_id, program_id, report) in runs {
        if only_program
            .as_ref()
            .is_some_and(|only| *only != program_id)
        {
            continue;
        }
        if !collections.contains_key(&program_id) {
            let sources = program_sources(&pool, &program_id).await?;
            collections.insert(program_id.clone(), sources);
        }
        let sources = &collections[&program_id];
        let references = ReferenceCollection::lexical(sources)?;
        let report: serde_json::Value = serde_json::from_str(&report)?;
        let findings = report["findings"].as_array().cloned().unwrap_or_default();
        // What each section cites, as the stored pages show it.
        let mut cited: HashMap<usize, Vec<ReferencePassage>> = HashMap::new();
        for finding in &findings {
            let unit = finding["unit"].as_u64().unwrap_or_default() as usize;
            for passage in finding["evidence"].as_array().into_iter().flatten() {
                let Some(passage) = stored_passage(passage)
                    .filter(|passage| passage.retrieval_kind == "cited")
                    .filter(|passage| still_saved(sources, passage))
                else {
                    continue;
                };
                let section = cited.entry(unit).or_default();
                if !section.iter().any(|seen| {
                    seen.source_id == passage.source_id && seen.start_byte == passage.start_byte
                }) {
                    section.push(passage);
                }
            }
        }
        for finding in &findings {
            let Some(quote) = finding["supporting_quote"]
                .as_str()
                .filter(|_| finding["verdict"] == "supported")
            else {
                continue;
            };
            approved += 1;
            let statement = finding["statement"].as_str().unwrap_or_default();
            let unit = finding["unit"].as_u64().unwrap_or_default() as usize;
            let old_page: Vec<ReferencePassage> = finding["evidence"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(stored_passage)
                .filter(|passage| passage.retrieval_kind != "execution")
                .collect();
            let origin = old_page
                .iter()
                .find(|passage| passage.text == quote)
                .or_else(|| old_page.iter().find(|passage| passage.text.contains(quote)))
                .cloned();
            let mut row = serde_json::json!({
                "lesson_id": lesson_id, "program_id": program_id, "unit": unit,
                "claim": statement, "origin_kind": origin.as_ref().map(|o| o.retrieval_kind.clone()),
                "old_page": {"passages": old_page.len(), "tokens": tokens(&old_page)},
            });
            let Some(origin) = origin.filter(|origin| still_saved(sources, origin)) else {
                skipped += 1;
                row["skipped"] = "the cited passage's source changed since the run".into();
                writeln!(results, "{row}")?;
                continue;
            };
            let mentions = references.mentions_subject_of(statement, &origin.text);
            off_subject += usize::from(!mentions);
            // The stored quote is the whole widened passage, and a page under
            // the new rule may draw that passage's edges a little differently.
            // A passage of the same source over its middle holds the hit the
            // context was widened around.
            let middle = origin.start_byte + (origin.end_byte - origin.start_byte) / 2;
            let reaches = |page: &[ReferencePassage]| {
                page.iter().any(|passage| {
                    passage.source_id == origin.source_id
                        && (passage.text.contains(quote)
                            || (passage.start_byte <= middle && middle < passage.end_byte))
                })
            };
            let overlaps = |page: &[ReferencePassage]| {
                page.iter().any(|passage| {
                    passage.source_id == origin.source_id
                        && passage.start_byte < origin.end_byte
                        && origin.start_byte < passage.end_byte
                })
            };
            let pages = crate::features::learning::content_verification::walk_pages(
                &llm,
                &references,
                (unit, statement),
                cited.clone(),
                |page| reaches(page),
            )
            .await?;
            let found = pages.iter().position(|page| reaches(&page.passages));
            let read: usize = pages
                .iter()
                .take(found.map_or(pages.len(), |at| at + 1))
                .map(|page| tokens(&page.passages))
                .sum();
            match found {
                Some(at) => {
                    *on_page.entry(at + 1).or_default() += 1;
                    old_tokens += tokens(&old_page);
                    new_tokens += read;
                    if let Some(pace) = pace {
                        // The old rule's deciding page alone: a lower bound
                        // on what the old rule spent reaching it.
                        let old = modelled_seconds(pace, tokens(&old_page), 1);
                        let new = modelled_seconds(pace, read, at + 1);
                        old_seconds += old;
                        new_seconds += new;
                        row["modelled_seconds"] =
                            serde_json::json!({"old_deciding_page": old, "new_rule": new});
                    }
                }
                None => never += 1,
            }
            row["cited_mentions_subject"] = mentions.into();
            row["new_page"] = found.map(|at| at + 1).into();
            row["first_overlapping_page"] = pages
                .iter()
                .position(|page| overlaps(&page.passages))
                .map(|at| at + 1)
                .into();
            row["tokens_read_to_it"] = read.into();
            row["pages"] = pages
                .iter()
                .map(|page| {
                    serde_json::json!({
                        "stage": page.stage, "passages": page.passages.len(),
                        "tokens": tokens(&page.passages),
                        "library_gap": gap(page.gaps.library), "keyword_gap": gap(page.gaps.keyword),
                        "counter_page": page.counter,
                    })
                })
                .collect::<Vec<_>>()
                .into();
            writeln!(results, "{row}")?;
        }
    }
    let modelled = match (pace, budget) {
        (Some(_), budget) => format!(
            "; modelled time over the findings reached: old deciding pages {old_seconds:.0} s \
             (a lower bound for the old rule), new rule {new_seconds:.0} s; \
             page budget at this pace {budget:?} tokens"
        ),
        (None, _) => "; no pace given, so no time is modelled".into(),
    };
    println!(
        "{approved} approved findings; {skipped} skipped (source changed); \
         {off_subject} cited passages fail mentions_subject_of; \
         reached on page (new rule): {on_page:?}; never reached: {never}{modelled}"
    );
    println!(
        "Estimated tokens, over findings the new rule reaches: old deciding pages {old_tokens}, \
         new pages read up to the cited passage {new_tokens}"
    );
    Ok(())
}
