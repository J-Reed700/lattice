//! Retrieval over a program's immutable source snapshots.
//!
//! A course's collection reads the course's index
//! (`references::course_index`): every active version of every source, web
//! pages and library documents alike, cut into the same byte spans and
//! ranked by the one hybrid search the library and Explorer use, dense and
//! full-text fused. Without an embedding model, or while some passages are
//! not embedded yet, the index is searched by its words, and
//! [`ReferenceCollection::mode`] says so; so does the lesson's stage text and
//! its verification report.
//!
//! A collection of sources a caller holds without a course index (outline
//! planning, assessments, a course whose index cannot be brought up to date,
//! tests) is ranked by an in-memory full-text store built from those sources
//! on first use: FTS5's `bm25`, the same as a course's text branch,
//! deterministic and needing no model.
//!
//! Every hit is mapped onto the snapshot's own bytes and widened to paragraph
//! edges, so evidence is always an exact substring of what was captured.
use crate::features::learning::dto::LearningSourceDto;
use crate::features::learning::references::course_index::{
    CourseProgress, CourseSearch, IndexMode, IndexView, CHUNK_CHARS,
};
use crate::infrastructure::passage_index::PassageStore;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

/// How far `retrieve` widens a ranked passage on each side, toward
/// paragraph edges.
const WIDEN_BYTES: usize = 300;
/// About the longest passage `retrieve` returns: one indexed passage widened
/// on both sides. Callers size retrieval to a token budget with it.
pub(in crate::features::learning) const PASSAGE_CHARS: usize = CHUNK_CHARS + 2 * WIDEN_BYTES;
/// How many passages a filled retrieval asks for first; it pages deeper,
/// doubling, while everything found still fits.
const FIRST_PAGE: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferencePassage {
    /// Immutable source version ID, also used by lesson citations.
    pub source_id: String,
    pub text: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub retrieval_kind: String,
    pub score: f64,
}

/// A ranked passage, with what the fusion recorded about where it came from.
/// `passage.score` is the fused score (the `bm25` relevance when the search is
/// by words alone). The branch ranks are diagnostics: no comparison key,
/// selection digest or receipt reads them, as none reads a score.
#[derive(Clone, Debug)]
pub(in crate::features::learning) struct Ranked {
    pub passage: ReferencePassage,
    /// The passage's best rank in a dense list, and its similarity there.
    pub dense: Option<(usize, f32)>,
    /// The passage's best rank in a full-text list, and its score there.
    pub lexical: Option<(usize, f32)>,
}

/// Which of the collection's sources a ranking reads.
#[derive(Clone, Debug, Default)]
pub(in crate::features::learning) enum Scope {
    /// Every source of the collection.
    #[default]
    All,
    /// Only these source versions: the ones a lesson cites, or a research
    /// round added.
    Keys(HashSet<String>),
}

/// What a collection searches.
enum Index<'a> {
    /// A course's index, as its reader's last `ensure_current` left it.
    Course {
        course: &'a CourseSearch,
        view: IndexView,
    },
    /// The collection's own sources in memory, built on first use.
    Memory(tokio::sync::OnceCell<Arc<PassageStore>>),
}

pub struct ReferenceCollection<'a> {
    pub sources: Vec<LearningSourceDto>,
    index: Index<'a>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
/// The longest prefix of `passages` that `fits` accepts; `fits` holds for
/// every shorter prefix of one it accepts.
fn longest_fitting_prefix(
    passages: &[ReferencePassage],
    fits: &impl Fn(&[ReferencePassage]) -> bool,
) -> usize {
    let (mut low, mut high) = (0, passages.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        if fits(passages.get(..middle).unwrap_or_default()) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

/// A lesson job's stage text while a course's index is brought up to date.
/// Outside a lesson job the stage goes nowhere, so a command can use it too.
#[derive(Default)]
struct IndexingStages {
    references: usize,
    started: Option<std::time::Instant>,
    /// The last whole percentage told, so the text moves once per percent.
    told: Option<usize>,
}

impl CourseProgress for IndexingStages {
    fn waiting(&mut self) {
        crate::features::learning::lesson_progress::stage(
            "Waiting for the course's reference index, which a background build is updating",
        );
    }

    fn indexing(&mut self, references: usize) {
        self.references = references;
        crate::features::learning::lesson_progress::stage(format!(
            "Indexing {references} new references"
        ));
    }

    fn embedded(&mut self, done: usize, total: usize) {
        let started = *self.started.get_or_insert_with(std::time::Instant::now);
        let percent = done.saturating_mul(100) / total.max(1);
        if self.told == Some(percent) {
            return;
        }
        self.told = Some(percent);
        let seconds = started.elapsed().as_secs_f64();
        let pace = if seconds > 0.0 {
            format!(" · {:.0} passages/s", done as f64 / seconds)
        } else {
            String::new()
        };
        let what = if self.references > 0 {
            format!("Indexing {} new references", self.references)
        } else {
            "Embedding the course's references".to_string()
        };
        crate::features::learning::lesson_progress::stage(format!(
            "{what} · {done} of {total} passages{pace}"
        ));
    }
}

impl<'a> ReferenceCollection<'a> {
    /// The sources searched by their words, from an in-memory store built
    /// on first use: for sources held without a course index.
    pub fn lexical(sources: &[LearningSourceDto]) -> Result<Self> {
        Ok(Self {
            sources: sources.to_vec(),
            index: Index::Memory(tokio::sync::OnceCell::new()),
        })
    }

    /// `sources` searched through the course's index, as `index` was last
    /// brought up to date. Run `ensure_current` first, or use
    /// [`Self::current`], which does.
    pub fn open(index: &'a CourseSearch, sources: &[LearningSourceDto]) -> Self {
        Self {
            sources: sources.to_vec(),
            index: Index::Course {
                course: index,
                view: index.view(),
            },
        }
    }

    /// The course's collection, its index first brought up to date (and its
    /// pending passages embedded), saying what it does in the lesson's stage
    /// text. When there is no index, or it cannot be brought up to date, the
    /// sources are searched by their words from memory, and the stage text
    /// says why.
    pub(in crate::features::learning) async fn current(
        course: Option<&'a CourseSearch>,
        sources: &[LearningSourceDto],
    ) -> Result<Self> {
        use crate::features::learning::lesson_progress::stage;
        let Some(course) = course else {
            stage("The course's reference index is not available, so references are searched by their words only");
            return Self::lexical(sources);
        };
        match course.ensure_current(&mut IndexingStages::default()).await {
            Ok(IndexMode::Hybrid) => {}
            Ok(IndexMode::Lexical) => stage(
                "No embedding model is active, so the course's references are searched by their words only",
            ),
            Ok(mode @ IndexMode::Partial { .. }) => {
                stage(format!("{} until the next indexing", mode.describe()));
            }
            Err(error) => {
                tracing::warn!(%error, program = course.program(), "Course index could not be brought up to date; searching references by their words");
                stage(format!(
                    "The course's reference index could not be brought up to date ({error}), so references are searched by their words only"
                ));
                return Self::lexical(sources);
            }
        }
        Ok(Self::open(course, sources))
    }

    /// The course's collection as it can be searched at once, for a user's
    /// command: new versions are chunked in and retired ones dropped, but
    /// nothing is embedded, so a command never waits for a course to embed.
    /// What is embedded already is searched by meaning; the rest by words,
    /// which the mode says (`partial`, or `lexical` without a model), while a
    /// background build embeds it. Without an index, or when the rows cannot
    /// be brought up to date, the sources are searched by words from memory.
    pub(in crate::features::learning) async fn searchable_now(
        course: Option<&'a CourseSearch>,
        sources: &[LearningSourceDto],
    ) -> Result<Self> {
        let Some(course) = course else {
            return Self::lexical(sources);
        };
        match course.bring_text_current().await {
            Ok(_) => Ok(Self::open(course, sources)),
            Err(error) => {
                tracing::warn!(%error, program = course.program(), "Course index rows could not be brought up to date; searching references by their words");
                Self::lexical(sources)
            }
        }
    }

    /// This collection again over `sources`: a course's brought up to date
    /// first (research may have saved references to it), any other rebuilt
    /// from `sources`.
    pub(in crate::features::learning) async fn reopen(
        &self,
        sources: &[LearningSourceDto],
    ) -> Result<Self> {
        match &self.index {
            Index::Course { course, .. } => Self::current(Some(*course), sources).await,
            Index::Memory(_) => Self::lexical(sources),
        }
    }

    /// How this collection searches; one mode for its whole life.
    pub fn mode(&self) -> IndexMode {
        match &self.index {
            Index::Course { view, .. } => view.mode,
            Index::Memory(_) => IndexMode::Lexical,
        }
    }

    /// The embedding model whose vectors this collection searches, if any.
    pub fn embedding_model(&self) -> Option<String> {
        match &self.index {
            Index::Course { view, .. } => view.identity.clone(),
            Index::Memory(_) => None,
        }
    }

    /// `query`'s best passages, at most `limit`, widened to paragraph edges
    /// and never overlapping. Scores rank evidence only; they are never a
    /// factual-correctness confidence.
    pub async fn retrieve(&self, query: &str, limit: usize) -> Result<Vec<ReferencePassage>> {
        Ok(self
            .rank(&[query], &Scope::All, limit.max(1))
            .await?
            .into_iter()
            .map(|ranked| ranked.passage)
            .collect())
    }

    /// Passages for `query`, best first, for as long as `fits` accepts them:
    /// the caller's window decides how many, not a count. Retrieval pages
    /// deeper while every passage found so far still fits.
    pub(in crate::features::learning) async fn retrieve_within(
        &self,
        query: &str,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<Vec<ReferencePassage>> {
        self.fill(query, fits).await
    }

    /// The next page of verification evidence for `query`: ranked passages,
    /// each widened to its surrounding source text (see
    /// `verification_context`), that overlap none of `shown`, best first,
    /// while `fits` accepts them. Also returns the ranked hits the page was
    /// built from, so a quote can be located inside a widened passage.
    pub(in crate::features::learning) async fn verification_page(
        &self,
        query: &str,
        shown: &[ReferencePassage],
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<(Vec<ReferencePassage>, Vec<ReferencePassage>)> {
        self.verification_page_within(query, shown, |_| true, fits)
            .await
    }

    /// [`Self::verification_page`] built only from the ranked hits `within`
    /// accepts, in their ranked order.
    pub(in crate::features::learning) async fn verification_page_within(
        &self,
        query: &str,
        shown: &[ReferencePassage],
        within: impl Fn(&ReferencePassage) -> bool,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<(Vec<ReferencePassage>, Vec<ReferencePassage>)> {
        self.verification_page_in(query, &Scope::All, shown, within, fits)
            .await
    }

    /// [`Self::verification_page_within`] over the sources `scope` keeps.
    async fn verification_page_in(
        &self,
        query: &str,
        scope: &Scope,
        shown: &[ReferencePassage],
        within: impl Fn(&ReferencePassage) -> bool,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<(Vec<ReferencePassage>, Vec<ReferencePassage>)> {
        let mut depth = FIRST_PAGE;
        loop {
            let hits: Vec<ReferencePassage> = self
                .rank(&[query], scope, depth)
                .await?
                .into_iter()
                .map(|ranked| ranked.passage)
                .collect();
            let exhausted = hits.len() < depth;
            let mut page: Vec<_> = self
                .verification_context(hits.iter().filter(|hit| within(hit)).cloned().collect())?
                .into_iter()
                .filter(|passage| {
                    !shown.iter().any(|seen| {
                        seen.source_id == passage.source_id
                            && seen.start_byte < passage.end_byte
                            && passage.start_byte < seen.end_byte
                    })
                })
                .collect();
            let fitting = longest_fitting_prefix(&page, &fits);
            if fitting < page.len() || exhausted {
                page.truncate(fitting);
                return Ok((page, hits));
            }
            depth = depth.saturating_mul(2);
        }
    }

    /// [`Self::verification_page`] from one source version only: the
    /// passages of `source_version_id` for `query`, best first, while `fits`
    /// accepts them. A source the user attaches to a claim is checked this
    /// way (claim review §5.4).
    pub(in crate::features::learning) async fn retrieve_from(
        &self,
        source_version_id: &str,
        query: &str,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<Vec<ReferencePassage>> {
        let scope = Scope::Keys(HashSet::from([source_version_id.to_string()]));
        let (page, _) = self
            .verification_page_in(query, &scope, &[], |_| true, fits)
            .await?;
        Ok(page)
    }

    async fn fill(
        &self,
        query: &str,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<Vec<ReferencePassage>> {
        let mut depth = FIRST_PAGE;
        loop {
            let found = self.retrieve(query, depth).await?;
            let exhausted = found.len() < depth;
            let mut passages = found;
            let fitting = longest_fitting_prefix(&passages, &fits);
            if fitting < passages.len() || exhausted {
                passages.truncate(fitting);
                return Ok(passages);
            }
            depth = depth.saturating_mul(2);
        }
    }

    /// The source passages `quotes` copy, in quote order: each quote's span
    /// in the first snapshot that contains it, exactly or apart from
    /// typography and spacing, widened as verification widens a retrieved
    /// passage. A quote no snapshot contains is skipped.
    pub(in crate::features::learning) fn cited_passages(
        &self,
        quotes: &[&str],
    ) -> Result<Vec<ReferencePassage>> {
        let span = |source: &LearningSourceDto, start: usize, end: usize| ReferencePassage {
            source_id: source.id.clone(),
            text: source
                .excerpt
                .get(start..end)
                .unwrap_or_default()
                .to_owned(),
            start_byte: start,
            end_byte: end,
            retrieval_kind: "cited".into(),
            score: 0.0,
        };
        let mut found = Vec::new();
        for quote in quotes
            .iter()
            .map(|quote| quote.trim())
            .filter(|q| !q.is_empty())
        {
            let exact = self.sources.iter().find_map(|source| {
                let start = source.excerpt.find(quote)?;
                Some(span(source, start, start + quote.len()))
            });
            let restored = || {
                self.sources.iter().find_map(|source| {
                    let copied = crate::features::learning::outline_citations::exact_source_quote(
                        &source.excerpt,
                        quote,
                    )?;
                    let start = source.excerpt.find(copied)?;
                    Some(span(source, start, start + copied.len()))
                })
            };
            found.extend(exact.or_else(restored));
        }
        self.verification_context(found)
    }

    /// The passages around the byte `spans` of source `source_id`, widened as
    /// verification widens a retrieved passage.
    pub(in crate::features::learning) fn source_passages(
        &self,
        source_id: &str,
        spans: &[(usize, usize)],
    ) -> Result<Vec<ReferencePassage>> {
        let source = self
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .ok_or_else(|| invalid("Unknown verification source."))?;
        let passages = spans
            .iter()
            .filter_map(|&(start, end)| {
                Some(ReferencePassage {
                    source_id: source.id.clone(),
                    text: source.excerpt.get(start..end)?.to_owned(),
                    start_byte: start,
                    end_byte: end,
                    retrieval_kind: "cited".into(),
                    score: 0.0,
                })
            })
            .collect();
        self.verification_context(passages)
    }

    /// Verification needs nearby conditions as well as the matching paragraph.
    /// Keep retrieval ranking unchanged, then restore surrounding source bytes
    /// and remove overlapping windows so repeated hits do not crowd out sources.
    fn verification_context(
        &self,
        passages: Vec<ReferencePassage>,
    ) -> Result<Vec<ReferencePassage>> {
        const SURROUNDING_CHARS: usize = 1700;
        let mut selected: Vec<ReferencePassage> = Vec::new();
        for mut passage in passages {
            let source = self
                .sources
                .iter()
                .find(|source| source.id == passage.source_id)
                .ok_or_else(|| invalid("Unknown verification source."))?;
            let text = &source.excerpt;
            if text.get(passage.start_byte..passage.end_byte) != Some(passage.text.as_str()) {
                return Err(invalid("Verification passage does not match its source."));
            }
            let mut start = text[..passage.start_byte]
                .char_indices()
                .rev()
                .nth(SURROUNDING_CHARS - 1)
                .map_or(0, |(index, _)| index);
            let mut end = text[passage.end_byte..]
                .char_indices()
                .nth(SURROUNDING_CHARS)
                .map_or(text.len(), |(index, _)| passage.end_byte + index);
            if let Some(index) = text[start..passage.start_byte]
                .find('\n')
                .filter(|index| *index <= 300)
            {
                // Preserve the start of a short source instead of trimming its
                // first line merely because the entire prefix fits.
                if start != 0 {
                    start += index + 1;
                }
            }
            if let Some(index) = text[passage.end_byte..end]
                .rfind('\n')
                .filter(|index| end - (passage.end_byte + *index) <= 300)
            {
                if end != text.len() {
                    end = passage.end_byte + index;
                }
            }
            if let Some(previous) = selected.iter_mut().find(|previous| {
                previous.source_id == passage.source_id
                    && previous.start_byte < end
                    && start < previous.end_byte
            }) {
                // Retain both original hits when their expanded windows only
                // partly overlap; dropping the later hit could lose a caveat.
                previous.start_byte = previous.start_byte.min(start);
                previous.end_byte = previous.end_byte.max(end);
                previous.text = text[previous.start_byte..previous.end_byte].to_owned();
                continue;
            }
            passage.start_byte = start;
            passage.end_byte = end;
            passage.text = text[start..end].to_owned();
            selected.push(passage);
        }
        Ok(selected)
    }

    /// A bounded structural view is supplied alongside passages, so the writer
    /// can see which books/sections exist without treating it as factual evidence.
    pub fn catalog(
        &self,
        llm: &dyn lattice_core::application::ports::LLMPort,
    ) -> serde_json::Value {
        let budget = llm.max_context_tokens() / 16;
        let mut entries = Vec::new();
        let mut used = 64;
        for source in &self.sources {
            let mut entry = serde_json::json!({
                "title":source.title,"versionId":source.id,
                "characters":source.excerpt.chars().count(),
                "headings":[],"additionalHeadingsOmitted":false,
            });
            let base = llm.count_tokens(&entry.to_string()) + 1;
            if used + base > budget {
                continue;
            }
            let mut headings = Vec::new();
            let mut omitted = false;
            for line in source
                .excerpt
                .lines()
                .filter(|line| line.trim_start().starts_with('#'))
            {
                // Whole headings, as many as the catalog's share of the window holds.
                let title = line.trim().to_owned();
                let cost = llm.count_tokens(&serde_json::json!(title).to_string()) + 1;
                if used + base + cost > budget {
                    omitted = true;
                    continue;
                }
                used += cost;
                headings.push(title);
            }
            used += base;
            if let Some(fields) = entry.as_object_mut() {
                fields.insert("headings".into(), serde_json::json!(headings));
                fields.insert(
                    "additionalHeadingsOmitted".into(),
                    serde_json::json!(omitted),
                );
            }
            entries.push(entry);
        }
        serde_json::json!({"totalSources":self.sources.len(),
            "omittedFromThisPrompt":self.sources.len()-entries.len(),"sources":entries,
            "scope":"A partial catalog for orientation. Retrieval searches all saved sources."})
    }
}

mod ranking;
mod snapshots;
mod subject;
pub(in crate::features::learning) use snapshots::{snapshot_passages, snapshot_room};

#[cfg(test)]
pub(in crate::features::learning) mod tests;
