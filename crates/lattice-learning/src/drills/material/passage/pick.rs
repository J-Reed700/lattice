//! Which passage a drill explains (docs/design/2026-11-07-drills-in-the-course.md section 4): one the course's sources
//! hold for the drill's lesson, found by searching them for the lesson's title, objective and module, widened to whole
//! places (a source's paragraphs, a long one cut into runs of sentences) and sized to hold at least as many sentences
//! as the explanation, so that each sentence of it has something of its own to say. Of the places the first page of
//! hits overlaps, the one no drill of the course was built on that carries the most of the search's term weight
//! comes first, a tie going to the better-ranked hit; once every one was, the one whose latest drill is the oldest.
//! Before the place that comes first is taken, the passage check asks the model whether it teaches the lesson; a place
//! it turns down gives way to the next in the same order, and a verdict stored for the lesson on the same basis is not
//! asked for again. A prepared lesson draws only on the sources its blocks cite.

use super::teaches;
use crate::drills::build::SourcePassage;
use crate::drills::lesson::LessonContext;
use crate::drills::repository;
use crate::dto::LearningSourceDto;
use crate::persistence::now;
use crate::reference_collection::{ReferenceCollection, ReferencePassage};
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::DatabaseHandle;
use lattice_core::shared::text::segmentation::{
    flowed_sentences, Boundaries, Break, Flowed, Span, Strength,
};
use lattice_index::passage_index::indexed_term;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

/// What the pick came to: the passage, if a place teaches the lesson, and how many places the check was asked about.
pub(super) struct Picked {
    pub(super) passage: Option<SourcePassage>,
    pub(super) asked: usize,
}

/// The passage the next drill of the lesson of `context` explains, sized for an explanation of `wanted` sentences:
/// of the places the page of hits overlaps, the one no drill of the course was built on that carries the most of the
/// search's term weight, or, once every one was, the one whose latest drill is the oldest. The passage check is asked
/// whether that place teaches the lesson, unless a verdict for it is stored on the lesson's current basis; a place it
/// turns down gives way to the next in the same order. The ranking is read a page at a time, deeper only while every
/// passage of the page was drilled or turned down. `passage` is `None` when no place of the lesson's sources teaches
/// it.
pub(super) async fn pick(
    pool: &impl DatabaseHandle,
    llm: &dyn LLMPort,
    context: &LessonContext,
    wanted: u32,
) -> Result<Picked> {
    let owners = repository::source_owners(pool, &context.program_id).await?;
    let owner = |version: &str| -> String {
        owners
            .get(version)
            .cloned()
            .unwrap_or_else(|| version.to_owned())
    };
    let cited: Vec<String> = context
        .cited_versions
        .iter()
        .map(|version| owner(version))
        .collect();
    let sources: Vec<LearningSourceDto> = LearningSourceLibraryRepository::new(pool.database())
        .verification_sources(&context.program_id)
        .await?
        .into_iter()
        .filter(|source| !source.excerpt.trim().is_empty())
        .filter(|source| !context.prepared || cited.contains(&owner(&source.id)))
        .collect();
    if sources.is_empty() {
        return Ok(Picked {
            passage: None,
            asked: 0,
        });
    }
    let drilled = repository::drilled_passages(pool, &context.program_id).await?;
    let wanted = usize::try_from(wanted).unwrap_or(usize::MAX);
    let collection = ReferenceCollection::lexical(&sources)?;
    let query = context.query();
    let weights = collection.term_weights(&query).await?;
    let mut split: HashMap<&str, Vec<Span>> = HashMap::new();
    let mut made: HashMap<(&str, usize), Option<Candidate>> = HashMap::new();
    let basis = teaches::basis(context)?;
    let mut verdicts = teaches::stored(pool, context, &basis).await?;
    let mut asked = 0;
    let mut depth = FIRST_PAGE;
    let mut hits = collection.retrieve(&query, depth).await?;
    loop {
        let best = {
            let turned_down = turned_down(&verdicts);
            best_on_page(
                &hits,
                &sources,
                &mut split,
                &mut made,
                &weights,
                &drilled,
                &turned_down,
                wanted,
            )
        };
        let unused = best.as_ref().is_some_and(|(key, _)| key.0.is_none());
        if !unused && hits.len() >= depth {
            depth = depth.saturating_mul(2);
            hits = collection.retrieve(&query, depth).await?;
            continue;
        }
        let Some((_, passage)) = best else {
            return Ok(Picked {
                passage: None,
                asked,
            });
        };
        let place = (
            passage.version_id.clone(),
            passage.start_byte,
            passage.end_byte,
        );
        if verdicts.get(&place) == Some(&true) {
            return Ok(Picked {
                passage: Some(passage),
                asked,
            });
        }
        let taught = teaches::ask(llm, context, &passage).await?;
        asked += 1;
        teaches::record(
            pool,
            context,
            &passage,
            &basis,
            &taught,
            llm.model_name(),
            now(),
        )
        .await?;
        if taught.teaches {
            return Ok(Picked {
                passage: Some(passage),
                asked,
            });
        }
        verdicts.insert(place, false);
    }
}

/// The spans the passage check turned down, by the version they are in.
fn turned_down(
    verdicts: &HashMap<(String, usize, usize), bool>,
) -> HashMap<&str, Vec<(usize, usize)>> {
    let mut spans: HashMap<&str, Vec<(usize, usize)>> = HashMap::new();
    for ((version, start, end), teaches) in verdicts {
        if !teaches {
            spans
                .entry(version.as_str())
                .or_default()
                .push((*start, *end));
        }
    }
    spans
}

/// How many passages of the ranking the first page holds; each further page doubles it.
const FIRST_PAGE: usize = 16;

/// A weight that orders the heavier first.
#[derive(Clone, Copy, Debug)]
struct Heavier(f64);

impl PartialEq for Heavier {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Heavier {}

impl PartialOrd for Heavier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Heavier {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0)
    }
}

/// A candidate passage's place in the ranking: when a drill last used it (`None` for never), then the term weight its
/// text holds (the heavier the better), its hit's rank and its place's index, the least of which is the best.
type Key = (Option<usize>, Heavier, usize, usize);

/// A place sized into a passage: its byte span in the source and the weight of the search's terms its text holds.
#[derive(Clone, Copy)]
struct Candidate {
    start: usize,
    end: usize,
    weight: f64,
}

/// The best candidate among the places the ranked `hits` overlap, every hit of the page weighed: of the places no drill
/// used, the one carrying the most of the search's term weight, a tie going to the better-ranked hit and then to the
/// lower place. A candidate whose span is among the `turned_down` ones, by version, is skipped. The places of each
/// source are kept in `split`, and each place's candidate in `made`, across pages, so each is made once per pick.
#[allow(clippy::too_many_arguments)]
fn best_on_page<'a>(
    hits: &[ReferencePassage],
    sources: &'a [LearningSourceDto],
    split: &mut HashMap<&'a str, Vec<Span>>,
    made: &mut HashMap<(&'a str, usize), Option<Candidate>>,
    weights: &HashMap<String, f64>,
    drilled: &[SourcePassage],
    turned_down: &HashMap<&str, Vec<(usize, usize)>>,
    wanted: usize,
) -> Option<(Key, SourcePassage)> {
    let mut best: Option<(Key, SourcePassage)> = None;
    for (rank, hit) in hits.iter().enumerate() {
        let Some(source) = sources.iter().find(|source| source.id == hit.source_id) else {
            continue;
        };
        let spans: &[Span] = split
            .entry(source.id.as_str())
            .or_insert_with(|| places(&source.excerpt, wanted));
        for (index, place) in spans.iter().enumerate() {
            if place.end <= hit.start_byte || hit.end_byte <= place.start {
                continue;
            }
            let Some(Candidate { start, end, weight }) = *made
                .entry((source.id.as_str(), index))
                .or_insert_with(|| candidate_of(&source.excerpt, spans, place, wanted, weights))
            else {
                continue;
            };
            if turned_down
                .get(source.id.as_str())
                .is_some_and(|spans| spans.contains(&(start, end)))
            {
                continue;
            }
            let latest_use = drilled
                .iter()
                .enumerate()
                .filter(|(_, passage)| {
                    passage.version_id == source.id
                        && passage.start_byte < end
                        && start < passage.end_byte
                })
                .map(|(position, _)| position)
                .max();
            let key = (latest_use, Heavier(weight), rank, index);
            if best.as_ref().is_some_and(|(held, _)| *held <= key) {
                continue;
            }
            let Some(text) = source.excerpt.get(start..end) else {
                continue;
            };
            best = Some((
                key,
                SourcePassage {
                    version_id: source.id.clone(),
                    start_byte: start,
                    end_byte: end,
                    title: source.title.clone(),
                    text: text.to_owned(),
                },
            ));
        }
    }
    best
}

/// The passage `place` of `text`, whose places are `places`, is sized into, with the weight of the terms of `weights`
/// its text holds.
fn candidate_of(
    text: &str,
    places: &[Span],
    place: &Span,
    wanted: usize,
    weights: &HashMap<String, f64>,
) -> Option<Candidate> {
    let (start, end) = sized(text, places, place.start, place.end, wanted)?;
    let weight = weight_of(text.get(start..end)?, weights);
    Some(Candidate { start, end, weight })
}

/// The summed weight of the distinct terms of `weights` that `text` holds, its words read as the index reads them.
fn weight_of(text: &str, weights: &HashMap<String, f64>) -> f64 {
    if weights.is_empty() {
        return 0.0;
    }
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(indexed_term)
        .collect::<BTreeSet<_>>()
        .iter()
        .filter_map(|term| weights.get(term))
        .sum()
}

/// The places of `text`: its paragraphs, a paragraph of at least twice `wanted` sentences cut into runs of at least
/// `wanted`. Text is read as flowed: a line break that only wraps a line is no break, so each piece between breaks is a
/// sentence or a block, and a run starts where a sentence or a block starts; the last run takes the pieces left over.
/// The places tile `text`.
fn places(text: &str, wanted: usize) -> Vec<Span> {
    let mut places = Vec::new();
    if text.is_empty() {
        return places;
    }
    let mut start = 0;
    let mut inner: Vec<Break> = Vec::new();
    for cut in Flowed
        .breaks(text)
        .into_iter()
        .filter(|cut| cut.strength >= Strength::Sentence)
    {
        if cut.strength >= Strength::Paragraph {
            runs_of(text, start..cut.at, &inner, wanted, &mut places);
            inner.clear();
            start = cut.at;
        } else {
            inner.push(cut);
        }
    }
    runs_of(text, start..text.len(), &inner, wanted, &mut places);
    places
}

/// The runs of the paragraph `paragraph` of `text`, whose breaks of at least `Sentence` strength are `inner`, pushed
/// onto `out`.
fn runs_of(text: &str, paragraph: Span, inner: &[Break], wanted: usize, out: &mut Vec<Span>) {
    let ends = inner
        .iter()
        .map(|cut| cut.at)
        .chain(std::iter::once(paragraph.end));
    let mut from = paragraph.start;
    let filled: Vec<bool> = ends
        .map(|to| {
            let piece = text.get(from..to);
            from = to;
            piece.is_some_and(|piece| !piece.trim().is_empty())
        })
        .collect();
    let mut left = filled.iter().filter(|filled| **filled).count();
    let mut before = 0usize;
    let mut run = paragraph.start;
    for (cut, filled) in inner.iter().zip(&filled) {
        if *filled {
            before += 1;
            left = left.saturating_sub(1);
        }
        if wanted > 0 && before >= wanted && left >= wanted {
            out.push(run..cut.at);
            run = cut.at;
            before = 0;
        }
    }
    out.push(run..paragraph.end);
}

/// The passage of `text`, whose places are `places`, around `start..end`: the whole places it touches, then the places
/// after them and, at the end of the text, before them, until the passage holds `wanted` sentences or the whole text;
/// trimmed of the space at its ends. `None` when there is nothing but space there.
fn sized(
    text: &str,
    places: &[Span],
    start: usize,
    end: usize,
    wanted: usize,
) -> Option<(usize, usize)> {
    let mut first = places.iter().position(|place| place.end > start)?;
    let mut last = places
        .iter()
        .rposition(|place| place.start < end.max(start.saturating_add(1)))?
        .max(first);
    let span = |first: usize, last: usize| -> Option<(usize, usize)> {
        Some((places.get(first)?.start, places.get(last)?.end))
    };
    loop {
        let (from, to) = span(first, last)?;
        if flowed_sentences(text.get(from..to)?).len() >= wanted {
            break;
        }
        if last + 1 < places.len() {
            last += 1;
        } else if first > 0 {
            first -= 1;
        } else {
            break;
        }
    }
    let (from, to) = span(first, last)?;
    let piece = text.get(from..to)?;
    let trimmed = piece.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lead = piece.len() - piece.trim_start().len();
    Some((from + lead, from + lead + trimmed.len()))
}
