//! The recall set (`LATTICE_PAGE_RECALL_TARGETS`) and the `standing` order
//! that walks it (claim sources design §7, §14.2): each claim of the set
//! with the passages that should support it, walked under each order, and
//! the standing scope read from the trust store with no model.
use super::{holds, program_sources, Counter};
use crate::features::learning::content_verification::{
    walk_pages, walk_standing_pages, Judgements, Reach, WalkedPage,
};
use crate::features::learning::dto::LearningSourceDto;
use crate::features::learning::reference_collection::{ReferenceCollection, ReferencePassage};
use crate::features::learning::references::course_index::CourseIndexes;
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write as _;
use std::sync::Arc;

/// The order that walks the standing scope rather than pass B.
pub(super) const STANDING: &str = "standing";

/// The standing order reads the trust store through the app's own queries,
/// which need this branch's schema: a scratch copy of the database,
/// migrated, so the copy given is never written.
pub(super) async fn migrated_copy(
    database: &str,
    scratch: &std::path::Path,
) -> Result<sqlx::SqlitePool> {
    let copy = scratch.join("standing-order.sqlite");
    std::fs::copy(database, &copy)?;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&copy))
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(pool)
}

/// `LATTICE_PAGE_RECALL_TRUST_MODEL`, else the model that made most of the
/// course's stored ownership judgements, read with the eval's own query.
async fn trust_model(pool: &sqlx::SqlitePool, program_id: &str) -> Result<Option<String>> {
    if let Ok(model) = std::env::var("LATTICE_PAGE_RECALL_TRUST_MODEL") {
        return Ok(Some(model));
    }
    sqlx::query_scalar(
        "SELECT model FROM source_subject_authority WHERE program_id = ? GROUP BY model ORDER BY COUNT(*) DESC, model LIMIT 1",
    )
    .bind(program_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| AppError::Database(error.to_string()))
}

/// The claim's standing scope from stored judgements only (claim sources
/// design §7.3), and what the row says of it; `None` when no profile or
/// ownership judgement is stored for it. A citation alone does not prove
/// support, so this retrieval-only eval excludes passages, not documents.
async fn stored_scope(
    pool: &sqlx::SqlitePool,
    program_id: &str,
    statement: &str,
) -> Result<Option<(HashSet<String>, Value)>> {
    let Some(model) = trust_model(pool, program_id).await? else {
        return Ok(None);
    };
    let mut judgements = Judgements::cached(pool.clone(), program_id, &model).await?;
    let Some(profile) = judgements.profile_if_stored(statement).await? else {
        return Ok(None);
    };
    if !judgements.owner_judged(&profile).await? {
        return Ok(None);
    }
    let scope = judgements.standing_scope(&profile, &[]).await?;
    let mut by_level: BTreeMap<&str, usize> = BTreeMap::new();
    for level in scope.proxies.values() {
        *by_level.entry(level.as_str()).or_default() += 1;
    }
    let note = json!({
        "versions": scope.candidates.len(),
        "owner_hosts": scope.owner_hosts,
        "proxies_by_level": by_level,
        "model": model,
    });
    Ok(Some((scope.candidates, note)))
}

/// Per order: claims, held by pass A, found on page N, never found.
type OrderTally = (usize, usize, BTreeMap<usize, usize>, usize);

/// One recall-set claim's passage at `value`'s place in its saved source,
/// `None` when the source or the place is gone.
fn saved_passage(
    sources: &[LearningSourceDto],
    value: &Value,
    kind: &str,
) -> Option<ReferencePassage> {
    let source_id = value["source_version_id"].as_str()?;
    let start = usize::try_from(value["start_byte"].as_u64()?).ok()?;
    let end = usize::try_from(value["end_byte"].as_u64()?).ok()?;
    let text = sources
        .iter()
        .find(|source| source.id == source_id)?
        .excerpt
        .get(start..end)?
        .to_owned();
    Some(ReferencePassage {
        source_id: source_id.into(),
        text,
        start_byte: start,
        end_byte: end,
        retrieval_kind: kind.into(),
        score: 0.0,
    })
}

/// `LATTICE_PAGE_RECALL_TARGETS`: a recall set read in place of the stored
/// reports' approved findings. Each line is a claim with the passages that
/// should support it (`targets`, each with a `quote` it contains) and what
/// its section cites (`cited`, pass A's page). For each claim and order:
/// whether pass A holds a target, where the first target lies in pass B's
/// fused list and on which page, and what the first page held instead.
pub(super) async fn recall_set(
    path: &str,
    (pool, indexes, llm): (&sqlx::SqlitePool, &Arc<CourseIndexes>, &Counter),
    trust: Option<&sqlx::SqlitePool>,
    budget: Option<usize>,
    orders: &[(&'static str, &'static [Reach])],
    (embedding, results): (&Value, &mut std::fs::File),
) -> Result<()> {
    let rows: Vec<Value> = std::fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    let mut collections: HashMap<String, Vec<LearningSourceDto>> = HashMap::new();
    let mut searches = HashMap::new();
    let mut tallies: BTreeMap<&str, OrderTally> = BTreeMap::new();
    for row in &rows {
        let program_id = row["program_id"].as_str().unwrap_or_default().to_owned();
        if !collections.contains_key(&program_id) {
            collections.insert(
                program_id.clone(),
                program_sources(pool, &program_id).await?,
            );
            let search = indexes
                .search(&program_id, tokio_util::sync::CancellationToken::new())
                .await?;
            searches.insert(program_id.clone(), search);
        }
        let sources = &collections[&program_id];
        let references = ReferenceCollection::current(searches.get(&program_id), sources).await?;
        let statement = row["claim"].as_str().unwrap_or_default();
        let unit = row["unit"].as_u64().unwrap_or_default() as usize;
        let section: Vec<ReferencePassage> = row["cited"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|cited| saved_passage(sources, cited, "cited"))
            .collect();
        // What the lesson cites anywhere: every claim of the set from it.
        let lesson: Vec<ReferencePassage> = rows
            .iter()
            .filter(|other| other["lesson_id"] == row["lesson_id"])
            .flat_map(|other| other["cited"].as_array().cloned().unwrap_or_default())
            .filter_map(|cited| saved_passage(sources, &cited, "cited"))
            .collect();
        let targets: Vec<(ReferencePassage, String)> = row["targets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|target| {
                let quote = target["quote"].as_str()?.to_owned();
                saved_passage(sources, target, "target").map(|passage| (passage, quote))
            })
            .collect();
        let mut base = json!({
            "id": row["id"], "claim_id": row["claim_id"], "lesson_id": row["lesson_id"],
            "program_id": program_id, "unit": unit, "claim": statement,
            "retrieval": references.mode().label(), "embedding": embedding,
            "page_budget": budget, "in_research_scope": row["in_research_scope"],
        });
        if targets.is_empty() {
            base["skipped"] = "no target passage is still saved".into();
            writeln!(results, "{base}")?;
            continue;
        }
        let settled_by_a = targets
            .iter()
            .any(|(target, quote)| section.iter().any(|passage| holds(passage, target, quote)));
        for (name, reaches) in orders {
            let mut scope = Value::Null;
            let pages: Vec<WalkedPage> = match trust.filter(|_| *name == STANDING) {
                Some(trust) => match stored_scope(trust, &program_id, statement).await? {
                    Some((versions, note)) => {
                        scope = note;
                        walk_standing_pages(
                            llm,
                            &references,
                            (unit, statement),
                            section.clone(),
                            versions,
                            budget,
                        )
                        .await?
                    }
                    None => Vec::new(),
                },
                None => {
                    walk_pages(
                        llm,
                        &references,
                        (unit, statement),
                        (section.clone(), lesson.clone()),
                        reaches,
                        budget,
                    )
                    .await?
                }
            };
            let list: Vec<&ReferencePassage> =
                pages.iter().flat_map(|page| &page.passages).collect();
            let found: Vec<Value> = targets
                .iter()
                .map(|(target, quote)| {
                    let rank = list
                        .iter()
                        .position(|passage| holds(passage, target, quote))
                        .map(|at| at + 1);
                    let page = pages
                        .iter()
                        .position(|page| {
                            page.passages
                                .iter()
                                .any(|passage| holds(passage, target, quote))
                        })
                        .map(|at| at + 1);
                    json!({
                        "source_id": target.source_id, "start_byte": target.start_byte,
                        "end_byte": target.end_byte, "rank": rank, "page": page,
                    })
                })
                .collect();
            let best_page = found
                .iter()
                .filter_map(|target| target["page"].as_u64())
                .min();
            let tally = tallies.entry(name).or_default();
            tally.0 += 1;
            if settled_by_a {
                tally.1 += 1;
            } else if let Some(page) = best_page {
                *tally.2.entry(page as usize).or_default() += 1;
            } else {
                tally.3 += 1;
            }
            let mut out = base.clone();
            out["order"] = (*name).into();
            out["settled_by_a"] = settled_by_a.into();
            out["rank"] = found
                .iter()
                .filter_map(|target| target["rank"].as_u64())
                .min()
                .into();
            out["page"] = best_page.into();
            out["targets"] = found.into();
            out["first_page"] = pages
                .first()
                .map(|page| {
                    page.passages
                        .iter()
                        .map(|passage| {
                            json!({
                                "source_id": passage.source_id,
                                "start_byte": passage.start_byte,
                                "end_byte": passage.end_byte,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into();
            out["list"] = json!({"passages": list.len(), "pages": pages.len()});
            if *name == STANDING {
                out["scope"] = scope;
            }
            writeln!(results, "{out}")?;
        }
    }
    for (name, (claims, by_a, on_page, never)) in &tallies {
        println!(
            "{name}: {claims} claims; a target in pass A {by_a}; first target on page {}; never reached {never}",
            on_page
                .iter()
                .map(|(page, count)| format!("{page}: {count}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

/// The `standing` order lists only the standing scope's versions (claim
/// sources design §14.1): a passage of another course version, however
/// relevant, is never on its pages.
#[tokio::test]
async fn the_standing_order_walks_only_the_scope() -> Result<()> {
    let source = |id: &str, text: &str| LearningSourceDto {
        id: id.into(),
        title: format!("Reference {id}"),
        url: None,
        excerpt: text.into(),
        acquired_at: 1,
    };
    let claim = "Mineral alpha shows property beta under pressure.";
    let sources = vec![
        source(
            "scoped",
            "Mineral alpha shows property beta under pressure in the museum's tests.",
        ),
        source(
            "other",
            "Mineral alpha shows property beta under pressure, a blog repeats.",
        ),
    ];
    let references = ReferenceCollection::lexical(&sources)?;
    let llm = Counter { window: 8_000 };
    let pages = walk_standing_pages(
        &llm,
        &references,
        (0, claim),
        Vec::new(),
        HashSet::from(["scoped".to_owned()]),
        None,
    )
    .await?;
    assert!(!pages.is_empty());
    assert!(pages
        .iter()
        .flat_map(|page| &page.passages)
        .all(|passage| passage.source_id == "scoped"));
    assert!(pages.iter().all(|page| page.reach == Reach::Standing));
    Ok(())
}
