//! Retrieval over a program's immutable source snapshots. A course's
//! collection reads its course index (`references::course_index`), ranked by
//! the library's hybrid search, or by words alone without an embedding model
//! ([`ReferenceCollection::mode`] says which). A collection without an index
//! is ranked by an in-memory FTS5 store. Every hit is widened to sentence
//! edges of the snapshot's own bytes, so evidence is always an exact substring.
use crate::dto::LearningSourceDto;
use crate::references::course_index::{
    CourseProgress, CourseSearch, IndexMode, IndexView, CHUNK_CHARS,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::same_text;
use lattice_core::shared::text::segmentation::{widen, Prose, Span, Strength};
use lattice_index::passage_index::PassageStore;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

/// How much context a retrieved passage gains on each side.
const WIDEN_CHARS: usize = 300;
/// How far verification reads around a passage for nearby conditions.
const VERIFICATION_REACH_CHARS: usize = 1700;
/// The breaks a passage starts and ends on, strongest first.
const LADDER: [Strength; 2] = [Strength::Sentence, Strength::Word];
/// About the longest passage `retrieve` returns: one indexed passage widened
/// on both sides. Callers size retrieval to a token budget with it.
pub(crate) const PASSAGE_CHARS: usize = CHUNK_CHARS + 2 * WIDEN_CHARS;
/// How many passages a filled retrieval asks for first; it pages deeper,
/// doubling, while everything found still fits.
const FIRST_PAGE: usize = 16;

/// How a passage came to be evidence; decided from the label it is saved under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassageKind {
    Cited,
    Researched,
    UserSelected,
    UserAttached,
    SavedVersion,
    Run,
    Retrieved(IndexMode),
}

impl PassageKind {
    /// The label a passage of this kind is saved under.
    pub fn label(self) -> &'static str {
        match self {
            Self::Cited => "cited",
            Self::Researched => "researched",
            Self::UserSelected => "user_selected",
            Self::UserAttached => "user_attached",
            Self::SavedVersion => "source_version",
            Self::Run => "execution",
            Self::Retrieved(mode) => mode.label(),
        }
    }

    /// The kind `label` names; a search mode's label, or any other, is a retrieved passage.
    pub fn of(label: &str) -> Self {
        match label {
            "cited" => Self::Cited,
            "researched" => Self::Researched,
            "user_selected" => Self::UserSelected,
            "user_attached" => Self::UserAttached,
            "source_version" => Self::SavedVersion,
            "execution" => Self::Run,
            "hybrid" => Self::Retrieved(IndexMode::Hybrid),
            "partial" => Self::Retrieved(IndexMode::Partial {
                embedded: 0,
                total: 0,
            }),
            _ => Self::Retrieved(IndexMode::Lexical),
        }
    }
}

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

impl ReferencePassage {
    /// The passage of `source` at `span`, its text read from the source's bytes; `None` when `span` is not a char range of the source.
    pub fn of(
        source: &LearningSourceDto,
        span: Span,
        kind: PassageKind,
        score: f64,
    ) -> Option<ReferencePassage> {
        Some(ReferencePassage {
            source_id: source.id.clone(),
            text: source.excerpt.get(span.clone())?.to_owned(),
            start_byte: span.start,
            end_byte: span.end,
            retrieval_kind: kind.label().into(),
            score,
        })
    }

    pub fn kind(&self) -> PassageKind {
        PassageKind::of(&self.retrieval_kind)
    }

    /// Whether the passage is an example's run, not a source.
    pub fn is_run(&self) -> bool {
        self.kind() == PassageKind::Run
    }

    /// Whether `other` is this passage: the same bytes of the same source version.
    pub fn same_place(&self, other: &ReferencePassage) -> bool {
        self.source_id == other.source_id
            && self.start_byte == other.start_byte
            && self.end_byte == other.end_byte
    }

    /// Whether `other` shares a byte of the same source version.
    pub fn overlaps(&self, other: &ReferencePassage) -> bool {
        self.source_id == other.source_id
            && same_text::overlaps(
                (self.start_byte, self.end_byte),
                (other.start_byte, other.end_byte),
            )
    }
}

/// What `item` adds to a JSON list in a prompt: its tokens and one separator.
pub(crate) fn item_tokens(
    llm: &dyn lattice_core::application::ports::LLMPort,
    item: &serde_json::Value,
) -> usize {
    llm.count_tokens(&item.to_string()) + 1
}

/// A ranked passage, with what the fusion recorded about where it came from.
/// `passage.score` is the fused score (the `bm25` relevance when the search is
/// by words alone). The branch ranks are diagnostics: no comparison key,
/// selection digest or receipt reads them, as none reads a score.
#[derive(Clone, Debug)]
pub(crate) struct Ranked {
    pub passage: ReferencePassage,
    /// The passage's best rank in a dense list, and its similarity there.
    pub dense: Option<(usize, f32)>,
    /// The passage's best rank in a full-text list, and its score there.
    pub lexical: Option<(usize, f32)>,
}

/// Which of the collection's sources a ranking reads.
#[derive(Clone, Debug, Default)]
pub(crate) enum Scope {
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

/// The span verification reads for the passage at `span` of `text`: the
/// passage and its surroundings, whole sentences.
pub(crate) fn verification_window(text: &str, span: Span) -> Span {
    widen(text, &Prose, span, VERIFICATION_REACH_CHARS, &LADDER)
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
        crate::lesson_progress::stage(
            "Waiting for the course's reference index, which a background build is updating",
        );
    }

    fn indexing(&mut self, references: usize) {
        self.references = references;
        crate::lesson_progress::stage(format!("Indexing {references} new references"));
    }

    fn embedding(&mut self, _already: usize, _total: usize) {
        self.started = Some(std::time::Instant::now());
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
        crate::lesson_progress::stage(format!("{what} · {done} of {total} passages{pace}"));
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
    pub async fn current(
        course: Option<&'a CourseSearch>,
        sources: &[LearningSourceDto],
    ) -> Result<Self> {
        use crate::lesson_progress::stage;
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
    /// command: new versions are chunked in but nothing is embedded, so what is
    /// not embedded yet is searched by words (the mode says so) while a background
    /// build embeds it.
    pub async fn searchable_now(
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
    pub(crate) async fn reopen(&self, sources: &[LearningSourceDto]) -> Result<Self> {
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

    /// `query`'s best passages, at most `limit`, widened to sentence edges
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
    pub(crate) async fn retrieve_within(
        &self,
        query: &str,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<Vec<ReferencePassage>> {
        self.retrieve_in(query, &Scope::All, fits).await
    }

    /// The next page of verification evidence for `query`: ranked passages,
    /// each widened to its surrounding source text (see
    /// `verification_context`), that overlap none of `shown`, best first,
    /// while `fits` accepts them. Also returns the ranked hits the page was
    /// built from, so a quote can be located inside a widened passage.
    pub(crate) async fn verification_page(
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
    pub(crate) async fn verification_page_within(
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
                .filter(|passage| !shown.iter().any(|seen| seen.overlaps(passage)))
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
    pub(crate) async fn retrieve_from(
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

    /// [`Self::retrieve_within`] over the sources `scope` keeps.
    pub(crate) async fn retrieve_in(
        &self,
        query: &str,
        scope: &Scope,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<Vec<ReferencePassage>> {
        let mut depth = FIRST_PAGE;
        loop {
            let found: Vec<ReferencePassage> = self
                .rank(&[query], scope, depth)
                .await?
                .into_iter()
                .map(|ranked| ranked.passage)
                .collect();
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
    /// in the first snapshot that contains it, apart from typography, spacing
    /// and letter case, widened as verification widens a retrieved passage. A
    /// quote no snapshot contains is skipped.
    pub(crate) fn cited_passages(&self, quotes: &[&str]) -> Result<Vec<ReferencePassage>> {
        let mut found = Vec::new();
        for quote in quotes
            .iter()
            .map(|quote| quote.trim())
            .filter(|q| !q.is_empty())
        {
            found.extend(self.sources.iter().find_map(|source| {
                let (start, end) = same_text::find(&source.excerpt, quote)?;
                ReferencePassage::of(source, start..end, PassageKind::Cited, 0.0)
            }));
        }
        self.verification_context(found)
    }

    /// The passages around the byte `spans` of source `source_id`, widened as
    /// verification widens a retrieved passage.
    pub(crate) fn source_passages(
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
                ReferencePassage::of(source, start..end, PassageKind::Cited, 0.0)
            })
            .collect();
        self.verification_context(passages)
    }

    /// Each passage, in ranked order, widened to its verification window and cut to the bytes
    /// no earlier passage of its source holds: every uncovered run is a passage of its own hit.
    fn verification_context(
        &self,
        passages: Vec<ReferencePassage>,
    ) -> Result<Vec<ReferencePassage>> {
        let mut selected: Vec<ReferencePassage> = Vec::new();
        for passage in passages {
            let source = self
                .sources
                .iter()
                .find(|source| source.id == passage.source_id)
                .ok_or_else(|| invalid("Unknown verification source."))?;
            let window = verification_window(&source.excerpt, passage.start_byte..passage.end_byte);
            let mut held: Vec<(usize, usize)> = selected
                .iter()
                .filter(|held| held.source_id == passage.source_id)
                .map(|held| (held.start_byte, held.end_byte))
                .collect();
            held.sort_unstable();
            let mut runs = Vec::new();
            let mut next = window.start;
            for (start, end) in held {
                if start >= window.end {
                    break;
                }
                if start > next {
                    runs.push(next..start);
                }
                next = next.max(end);
            }
            if next < window.end {
                runs.push(next..window.end);
            }
            selected.extend(runs.into_iter().filter_map(|run| {
                ReferencePassage::of(source, run, passage.kind(), passage.score)
            }));
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
            let base = item_tokens(llm, &entry);
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
                let cost = item_tokens(llm, &serde_json::json!(title));
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

mod pages;
mod ranking;
mod snapshots;
mod subject;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub(crate) use snapshots::{snapshot_passages, snapshot_room};
pub(crate) use subject::shared_terms;

#[cfg(test)]
pub(crate) mod tests;
