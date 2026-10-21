//! Page recall on the course index (course evidence index §7.1–§7.2): for
//! every claim a stored lesson check approved, whether pass A would have
//! settled it, and otherwise where the passage its judge cited lies in pass
//! B's fused list, on which page, and what reading up to it costs.
//!
//! It reads a stored run, the way `scripts/lesson_claim_pairs.py extract`
//! does: `learning_lesson_verifications.report_json` for the findings and
//! the program's saved source versions, the database opened read-only. It
//! builds a temporary course index in a scratch directory from those
//! versions, with the configured embedding model when it is given one
//! (`LATTICE_PAGE_RECALL_EMBEDDING`) and by words alone otherwise. Given
//! `LATTICE_PAGE_RECALL_INDEXES`, it keeps the indexes there instead: a copy
//! of the app's `course-index` directory, built by the same model, is
//! brought current rather than embedded again. It
//! walks each claim's pass B pages to the end of its lists under each order
//! of §5.2. No model is asked: the judge is assumed to read each page whole
//! and settle nothing until the cited passage is on it.
//!
//! Given `LATTICE_PAGE_RECALL_RESEARCH_SECONDS`, what researching one claim
//! on the web costs, it reads every claim pass A left unsupported under each
//! depth rule of D5 (first page, break-even, list end) by the app's own rule
//! (`Depth::reads_next`, `page_seconds`): how many the course supports, how
//! many go to web research, and the minutes that takes.
//!
//! A section's citations are what the stored report marks `cited`: a
//! finding's own unit's for pass A, and across the lesson the sources they
//! come from for the lesson-sources orders. The stored quote is the whole
//! widened passage the judge cited, and the index may draw that passage's
//! edges a little differently, so a passage of the same source that
//! contains it, or covers its middle byte, holds it.
use crate::features::learning::content_verification::{
    page_budget, page_seconds, walk_pages, Depth, Reach, WalkedPage, ORDERS,
};
use crate::features::learning::dto::LearningSourceDto;
use crate::features::learning::reference_collection::{ReferenceCollection, ReferencePassage};
use crate::features::learning::references::course_index::CourseIndexes;
use crate::infrastructure::passage_index::EmbedderSource;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse, Pace};
use lattice_core::application::ports::{EmbeddingPort, LLMPort};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::sync::Arc;

/// Counts tokens by the calibrated estimate and holds the run's configured
/// window, which bounds a page and a check. It is never asked to answer.
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

/// The embedding model the eval was given, loaded once, or none. The course
/// eval hands its recipe's model to the course indexes through it too.
pub(super) struct GivenModel(pub(super) Option<Arc<dyn EmbeddingPort>>);

#[async_trait::async_trait]
impl EmbedderSource for GivenModel {
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>> {
        self.0.clone()
    }
}

/// `LATTICE_PAGE_RECALL_EMBEDDING`: the configured embedding model's
/// directory, loaded through the app's own loader, which names the model by
/// its artifact identity. `None` without the variable: the index is
/// searched by words alone.
async fn embedding_model() -> Result<Option<Arc<dyn EmbeddingPort>>> {
    let Some(dir) = std::env::var_os("LATTICE_PAGE_RECALL_EMBEDDING") else {
        return Ok(None);
    };
    let model = tokio::task::spawn_blocking(move || {
        crate::features::embedding::candle_service::CandleEmbeddingService::open_unregistered(
            std::path::Path::new(&dir),
        )
    })
    .await
    .map_err(|error| AppError::InternalError(format!("Embedding model load failed: {error}")))??;
    Ok(Some(Arc::new(model)))
}

/// The program's saved source versions as a lesson check reads them, read
/// without the backfill the app runs, so the stored run is never written.
async fn program_sources(
    pool: &sqlx::SqlitePool,
    program_id: &str,
) -> Result<Vec<LearningSourceDto>> {
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
        .map(|row| LearningSourceDto {
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

fn stored_passage(value: &Value) -> Option<ReferencePassage> {
    serde_json::from_value(value.clone()).ok()
}

/// Whether `passage` is still the text at its place in a saved source.
fn still_saved(sources: &[LearningSourceDto], passage: &ReferencePassage) -> bool {
    sources.iter().any(|source| {
        source.id == passage.source_id
            && source.excerpt.get(passage.start_byte..passage.end_byte)
                == Some(passage.text.as_str())
    })
}

/// Whether `passage` holds the judge-cited `origin`: a passage of its
/// source that contains its quote, or covers its middle byte, the hit its
/// context was widened around.
fn holds(passage: &ReferencePassage, origin: &ReferencePassage, quote: &str) -> bool {
    let middle = origin.start_byte + (origin.end_byte - origin.start_byte) / 2;
    passage.source_id == origin.source_id
        && (passage.text.contains(quote)
            || (passage.start_byte <= middle && middle < passage.end_byte))
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

/// `LATTICE_PAGE_RECALL_RESEARCH_SECONDS`: what researching one claim on
/// the web costs, in seconds, as a run's `Researched` events measured it
/// (`round_ms_per_claim`). `None` without it: no depth rule is compared.
fn research_from_env() -> Option<f64> {
    let value = std::env::var("LATTICE_PAGE_RECALL_RESEARCH_SECONDS").ok()?;
    Some(value.trim().parse().unwrap())
}

/// The depth rules of D5, by the names the eval prints.
const DEPTHS: [(&str, Depth); 3] = [
    ("first page", Depth::FirstPage),
    ("break-even", Depth::BreakEven),
    ("list end", Depth::ListEnd),
];

/// How far `depth` reads a claim pass A left unsupported, its pages costing
/// `costs` seconds each (`None` without a pace) and its support on page
/// `found`, counted from 0, if on any: whether the course supports it, and
/// the seconds pass B spends on it. Every claim reads its first page (D1);
/// past it, the app's rule decides with `research`, what researching the
/// claim on the web costs.
fn read_under(
    depth: Depth,
    costs: &[Option<f64>],
    found: Option<usize>,
    research: Option<f64>,
) -> (bool, f64) {
    let (mut first, mut spent) = (0.0, 0.0);
    for (at, cost) in costs.iter().enumerate() {
        if at == 0 {
            first = cost.unwrap_or_default();
        } else if depth.reads_next(spent, *cost, research) {
            spent += cost.unwrap_or_default();
        } else {
            break;
        }
        if found == Some(at) {
            return (true, first + spent);
        }
    }
    (false, first + spent)
}

/// One depth rule's totals under one order, over the claims pass A did not
/// settle: each is found in the course or goes to web research.
#[derive(Default)]
struct DepthTally {
    found: usize,
    to_web: usize,
    pass_b_seconds: f64,
    /// The most pass B spent on one claim.
    worst_seconds: f64,
}

impl DepthTally {
    fn summary(&self, (order, depth): (&str, &str), research: f64, paced: bool) -> String {
        let minutes = |seconds: f64| {
            if paced {
                format!("{:.1} min", seconds / 60.0)
            } else {
                "no pace given".into()
            }
        };
        format!(
            "[{order}, {depth}] found in the course {}; to web research {}; modelled {} with web research at {research} s a claim (pass B {}); worst claim's pass B {}",
            self.found,
            self.to_web,
            minutes(self.pass_b_seconds + self.to_web as f64 * research),
            minutes(self.pass_b_seconds),
            minutes(self.worst_seconds),
        )
    }
}

/// `LATTICE_PAGE_RECALL_ORDER`: the orders of §5.2 to walk, by name, or
/// `all` (the default) for every one.
fn orders_from_env() -> Vec<(&'static str, &'static [Reach])> {
    let wanted = std::env::var("LATTICE_PAGE_RECALL_ORDER").unwrap_or_else(|_| "all".into());
    let orders: Vec<_> = ORDERS
        .iter()
        .copied()
        .filter(|(name, _)| wanted == "all" || *name == wanted)
        .collect();
    assert!(
        !orders.is_empty(),
        "LATTICE_PAGE_RECALL_ORDER is relevance, lesson-sources, tie-break or all"
    );
    orders
}

/// One order's totals.
#[derive(Default)]
struct Tally {
    approved: usize,
    settled_by_a: usize,
    on_page: BTreeMap<usize, usize>,
    never: usize,
    /// Tokens read to the support, per finding reached in either pass.
    to_support: Vec<usize>,
    modelled_seconds: f64,
    list_passages: Vec<usize>,
    list_tokens: Vec<usize>,
}

/// The value at `share` of the way up `values`, sorted.
fn quantile(values: &[usize], share: f64) -> Option<usize> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let at = ((sorted.len().saturating_sub(1)) as f64 * share).round() as usize;
    sorted.get(at).copied()
}

impl Tally {
    fn summary(&self, name: &str, off_subject: usize, paced: bool) -> String {
        let spread = |values: &[usize]| {
            format!(
                "median {:?}, p90 {:?}, max {:?}",
                quantile(values, 0.5),
                quantile(values, 0.9),
                values.iter().max()
            )
        };
        format!(
            "[{name}] {} approved; settled by pass A {}; reached in pass B on page {:?}; never reached {}; \
             tokens to support {} total {}; modelled {}; run-out list passages {}; run-out list tokens {}; \
             cited passages failing the subject test {off_subject}",
            self.approved,
            self.settled_by_a,
            self.on_page,
            self.never,
            spread(&self.to_support),
            self.to_support.iter().sum::<usize>(),
            if paced {
                format!("{:.1} min", self.modelled_seconds / 60.0)
            } else {
                "no pace given".into()
            },
            spread(&self.list_passages),
            spread(&self.list_tokens),
        )
    }
}

#[tokio::test]
#[ignore = "requires LATTICE_PAGE_RECALL_DB, LATTICE_PAGE_RECALL_WINDOW and LATTICE_PAGE_RECALL_RESULTS; reads a stored run, makes no model request"]
async fn live_page_recall_eval() -> Result<()> {
    let database = std::env::var("LATTICE_PAGE_RECALL_DB").unwrap();
    // The configured window of the run being read: it bounds every page.
    let window: usize = std::env::var("LATTICE_PAGE_RECALL_WINDOW")
        .unwrap()
        .parse()
        .unwrap();
    let only_program = std::env::var("LATTICE_PAGE_RECALL_PROGRAM").ok();
    let results_path = std::env::var("LATTICE_PAGE_RECALL_RESULTS").unwrap();
    let pace = pace_from_env();
    let budget = pace.and_then(|(pace, answer)| page_budget(Some(pace), Some(answer)));
    let research = research_from_env();
    let orders = orders_from_env();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&database)
                .read_only(true),
        )
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    let scratch = tempfile::tempdir()?;
    let index_dir = std::env::var_os("LATTICE_PAGE_RECALL_INDEXES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| scratch.path().to_path_buf());
    let model = embedding_model().await?;
    let identity = model.as_ref().map(|model| model.model_identity());
    let indexes = CourseIndexes::new(
        index_dir,
        pool.clone(),
        Arc::new(GivenModel(model)),
        Arc::new(crate::features::search::trait_def::HybridSearchUseCaseFactory),
        lattice_core::shared::runtime::jobs::JobRuntime::new(pool.clone()),
        Arc::new(lattice_core::shared::runtime::background::BackgroundTasks::default()),
        Arc::default(),
    );
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
    println!(
        "Retrieval: {}; page budget: {}; web research: {}",
        match &identity {
            Some(identity) => format!("hybrid, embedding model {identity}"),
            None => "text only (no LATTICE_PAGE_RECALL_EMBEDDING)".into(),
        },
        match budget {
            Some(tokens) => format!("{tokens} tokens"),
            None => "unavailable: no pace given, pages are bounded by the window alone".into(),
        },
        match research {
            Some(seconds) => format!("{seconds} s a claim"),
            None =>
                "not given (no LATTICE_PAGE_RECALL_RESEARCH_SECONDS), no depth rule compared".into(),
        }
    );
    let mut tallies: HashMap<&str, Tally> = HashMap::new();
    let mut depth_tallies: HashMap<(&str, &str), DepthTally> = HashMap::new();
    let (mut skipped, mut off_subject) = (0, 0);
    let mut searches = HashMap::new();
    let mut collections: HashMap<String, Vec<LearningSourceDto>> = HashMap::new();
    for (lesson_id, program_id, report) in runs {
        if only_program
            .as_ref()
            .is_some_and(|only| *only != program_id)
        {
            continue;
        }
        if !collections.contains_key(&program_id) {
            collections.insert(
                program_id.clone(),
                program_sources(&pool, &program_id).await?,
            );
            let search = indexes
                .search(&program_id, tokio_util::sync::CancellationToken::new())
                .await?;
            searches.insert(program_id.clone(), search);
        }
        let sources = &collections[&program_id];
        let references = ReferenceCollection::current(searches.get(&program_id), sources).await?;
        println!(
            "Program {program_id}, lesson {lesson_id}: {}",
            references.mode().describe()
        );
        let report: Value = serde_json::from_str(&report)?;
        let findings = report["findings"].as_array().cloned().unwrap_or_default();
        // What each section cites, and the lesson across its sections, as the
        // stored pages show it.
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
        let lesson: Vec<ReferencePassage> = cited.values().flatten().cloned().collect();
        for finding in &findings {
            let Some(quote) = finding["supporting_quote"]
                .as_str()
                .filter(|_| finding["verdict"] == "supported")
            else {
                continue;
            };
            let statement = finding["statement"].as_str().unwrap_or_default();
            let unit = finding["unit"].as_u64().unwrap_or_default() as usize;
            let stored: Vec<ReferencePassage> = finding["evidence"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(stored_passage)
                .filter(|passage| passage.retrieval_kind != "execution")
                .collect();
            let origin = stored
                .iter()
                .find(|passage| passage.text == quote)
                .or_else(|| stored.iter().find(|passage| passage.text.contains(quote)))
                .cloned();
            let base = json!({
                "lesson_id": lesson_id, "program_id": program_id, "unit": unit,
                "claim": statement, "retrieval": references.mode().label(),
                "embedding": identity, "page_budget": budget, "research_seconds": research,
            });
            let Some(origin) = origin.filter(|origin| still_saved(sources, origin)) else {
                skipped += 1;
                let mut row = base.clone();
                row["skipped"] = "the cited passage's source changed since the run".into();
                writeln!(results, "{row}")?;
                continue;
            };
            let mentions = references
                .subject_of(statement)
                .await?
                .mentioned_in(&origin.text);
            off_subject += usize::from(!mentions);
            let section = cited.get(&unit).cloned().unwrap_or_default();
            let a_tokens = tokens(&section);
            let settled_by_a = section.iter().any(|passage| holds(passage, &origin, quote));
            for (name, reaches) in &orders {
                let pages: Vec<WalkedPage> = walk_pages(
                    &llm,
                    &references,
                    (unit, statement),
                    (section.clone(), lesson.clone()),
                    reaches,
                    budget,
                )
                .await?;
                let list: Vec<&ReferencePassage> =
                    pages.iter().flat_map(|page| &page.passages).collect();
                let rank = list
                    .iter()
                    .position(|passage| holds(passage, &origin, quote))
                    .map(|at| at + 1);
                let found = pages
                    .iter()
                    .position(|page| page.passages.iter().any(|p| holds(p, &origin, quote)));
                let read_to_it: Option<usize> = found.map(|at| {
                    pages
                        .iter()
                        .take(at + 1)
                        .map(|page| tokens(&page.passages))
                        .sum()
                });
                let list_tokens: usize = pages.iter().map(|page| tokens(&page.passages)).sum();
                // Pass A's answer, when it has a page, and one answer per
                // pass B page read up to the support.
                let answers = usize::from(!section.is_empty()) + found.map_or(0, |at| at + 1);
                let seconds = pace.zip(read_to_it).map(|((pace, answer), read)| {
                    (a_tokens + read) as f64 / pace.prompt_tokens_per_second
                        + answers as f64 * answer / pace.output_tokens_per_second
                });
                // Each depth rule over the same pages, for a claim pass A
                // left unsupported.
                let depths = research.filter(|_| !settled_by_a).map(|research| {
                    let costs: Vec<Option<f64>> = pages
                        .iter()
                        .map(|page| {
                            page_seconds(
                                pace.map(|(pace, _)| pace),
                                tokens(&page.passages),
                                pace.map(|(_, answer)| answer),
                            )
                        })
                        .collect();
                    let mut depths = serde_json::Map::new();
                    for (rule, depth) in DEPTHS {
                        let (supported, seconds) = read_under(depth, &costs, found, Some(research));
                        let tally = depth_tallies.entry((*name, rule)).or_default();
                        if supported {
                            tally.found += 1;
                        } else {
                            tally.to_web += 1;
                        }
                        tally.pass_b_seconds += seconds;
                        tally.worst_seconds = tally.worst_seconds.max(seconds);
                        depths.insert(
                            rule.into(),
                            json!({"found": supported, "pass_b_seconds": seconds}),
                        );
                    }
                    Value::Object(depths)
                });
                let tally = tallies.entry(name).or_default();
                tally.approved += 1;
                tally.list_passages.push(list.len());
                tally.list_tokens.push(list_tokens);
                if settled_by_a {
                    tally.settled_by_a += 1;
                    tally.to_support.push(a_tokens);
                } else if let (Some(at), Some(read)) = (found, read_to_it) {
                    *tally.on_page.entry(at + 1).or_default() += 1;
                    tally.to_support.push(a_tokens + read);
                    tally.modelled_seconds += seconds.unwrap_or_default();
                } else {
                    tally.never += 1;
                }
                let mut row = base.clone();
                row["order"] = (*name).into();
                row["settled_by_a"] = settled_by_a.into();
                row["cited_mentions_subject"] = mentions.into();
                row["rank"] = rank.into();
                row["page"] = found.map(|at| at + 1).into();
                row["reach"] = found
                    .and_then(|at| pages.get(at))
                    .map(|page| format!("{:?}", page.reach))
                    .into();
                row["attribution_tokens"] = a_tokens.into();
                row["tokens_read_to_it"] = read_to_it.into();
                row["modelled_seconds"] = seconds.into();
                if let Some(depths) = depths {
                    row["depths"] = depths;
                }
                row["list"] =
                    json!({"passages": list.len(), "tokens": list_tokens, "pages": pages.len()});
                row["pages"] = pages
                    .iter()
                    .map(|page| {
                        json!({
                            "reach": format!("{:?}", page.reach), "passages": page.passages.len(),
                            "tokens": tokens(&page.passages), "list_end": page.list_end,
                            "counter": page.counter,
                        })
                    })
                    .collect::<Vec<_>>()
                    .into();
                writeln!(results, "{row}")?;
            }
        }
    }
    println!(
        "{skipped} findings skipped (source changed); orders: {}",
        orders
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    );
    for (name, _) in &orders {
        if let Some(tally) = tallies.get(name) {
            println!("{}", tally.summary(name, off_subject, pace.is_some()));
        }
    }
    if let Some(research) = research {
        for (name, _) in &orders {
            for (rule, _) in DEPTHS {
                if let Some(tally) = depth_tallies.get(&(*name, rule)) {
                    println!("{}", tally.summary((name, rule), research, pace.is_some()));
                }
            }
        }
    }
    Ok(())
}
