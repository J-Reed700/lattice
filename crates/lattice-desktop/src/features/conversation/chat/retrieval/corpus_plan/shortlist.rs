//! Which catalog documents the planner is shown.
//!
//! The planner used to be shown the whole catalog in filename order, cut off
//! wherever the window ran out. On a 379-document vault that was the first
//! eighty-odd files alphabetically, whatever the question, at 15k tokens that
//! a 9B could not prefill in time. It is now shown the documents a local search
//! ranks for the request, each with its reading-order neighbours, and for a
//! request to learn or start at the beginning, the front matter of the books
//! those documents belong to. The plan is still validated against the whole
//! catalog, so reading order is worked out from every document in the book,
//! not just the ones the planner saw.

use std::collections::{HashMap, HashSet};

use tracing::warn;

use lattice_core::application::contracts::search::CorpusDocument;

use super::super::{select_informative_terms, tokenize_keyword_terms};
use super::{
    fused_search, navigation_only, numbered_series_prefix, whole_document_intent,
    HybridSearchTrait, SearchResultDto,
};

/// Passages the candidate search reads. Fusion keeps at most four passages a
/// document, so this reaches at least ten documents and usually twenty or more.
const CANDIDATE_SEARCH_PASSAGES: usize = 40;

/// Informative terms carried from the conversation into the follow-up search.
const CONVERSATION_TERMS: usize = 12;

/// Documents a local search ranks for this request, best first.
///
/// The request alone, and, when there is conversation, the request with the
/// conversation's informative terms: "is there any way round it?" names
/// nothing a search can find, and the turn before it does. The two rankings
/// are interleaved so neither crowds the other out. A failed search leaves an
/// empty ranking, and the planner then sees the catalog in its own order.
pub(in super::super) async fn candidate_documents(
    hybrid: &dyn HybridSearchTrait,
    question: &str,
    history: Option<&str>,
    catalog: &[CorpusDocument],
    document_ids: &HashSet<String>,
) -> Vec<String> {
    let mut queries = vec![question.to_string()];
    if let Some(history) = history.filter(|history| !history.trim().is_empty()) {
        let terms = select_informative_terms(tokenize_keyword_terms(history), CONVERSATION_TERMS);
        if !terms.is_empty() {
            queries.push(format!("{question} {}", terms.join(" ")));
        }
    }
    let mut rankings = Vec::new();
    let mut bridge = None;
    for query in &queries {
        match fused_search(hybrid, query, document_ids, CANDIDATE_SEARCH_PASSAGES).await {
            Ok(results) => {
                if bridge.is_none() {
                    bridge = find_entity_bridge(&results, catalog, document_ids, question);
                }
                rankings.push(documents_in_rank_order(&results));
            }
            Err(error) => {
                warn!(%error, "Planner shortlist search failed; showing the catalog in its own order")
            }
        }
    }
    if let Some(bridge) = bridge {
        let predicate_terms: Vec<String> = tokenize_keyword_terms(question)
            .into_iter()
            .filter(|w| !bridge.source_heading_terms.contains(w))
            .collect();
        let expanded = if !predicate_terms.is_empty() {
            predicate_terms.join(" ")
        } else {
            question.to_string()
        };
        if let Ok(results) = fused_search(
            hybrid,
            &expanded,
            &bridge.target_document_ids,
            CANDIDATE_SEARCH_PASSAGES,
        )
        .await
        {
            rankings.push(documents_in_rank_order(&results));
        }
    }
    interleave(rankings)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::features::conversation::chat::retrieval) struct EntityBridge {
    pub source_result_id: String,
    pub source_document_id: String,
    pub source_heading_terms: HashSet<String>,
    pub target_document_ids: HashSet<String>,
}

/// The first prose paragraph in an opening chunk.
///
/// Markdown headings and a short plain-text title are skipped. Structured
/// transcripts and tables are not prose openings and deliberately return no
/// paragraph: speaker names and column labels are poor document links.
pub(in crate::features::conversation::chat::retrieval) fn first_body_paragraph(
    content: &str,
) -> String {
    let mut paragraphs: Vec<Vec<&str>> = Vec::new();
    let mut current = Vec::new();
    for line in content.lines().map(str::trim) {
        if line.starts_with('#') {
            continue;
        }
        if line.is_empty() {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
            continue;
        }
        if line.starts_with('|') || line.starts_with('[') {
            if current.is_empty() && paragraphs.is_empty() {
                return String::new();
            }
            break;
        }
        current.push(line);
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }
    let mut paragraphs = paragraphs.into_iter();
    let first = paragraphs.next().unwrap_or_default();
    let first_is_plain_title = first.len() == 1
        && first[0].chars().count() <= 100
        && !first[0].contains(['.', '!', '?', ':']);
    let chosen = if first_is_plain_title {
        paragraphs.next().unwrap_or(first)
    } else {
        first
    };
    chosen.join(" ")
}

/// Multi-token proper names and explicit identifiers in prose.
///
/// Matching whole phrases rather than any capitalised word avoids turning a
/// common title word into a corpus-wide second-hop search. A lone token is
/// accepted only when its spelling makes it an explicit identifier.
pub(in crate::features::conversation::chat::retrieval) fn referenced_entity_phrases(
    paragraph: &str,
) -> Vec<Vec<String>> {
    fn finish(run: &mut Vec<String>, identifier: bool, out: &mut Vec<Vec<String>>) {
        if run.len() >= 2 || identifier {
            out.push(std::mem::take(run));
        } else {
            run.clear();
        }
    }

    let mut phrases = Vec::new();
    for sentence in paragraph.split(['.', '!', '?', '\n']) {
        let mut run = Vec::new();
        let mut run_has_identifier = false;
        for raw in sentence.split_whitespace() {
            let clean = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
            if clean.is_empty() {
                finish(&mut run, run_has_identifier, &mut phrases);
                run_has_identifier = false;
                continue;
            }
            let has_digit = clean.chars().any(|c| c.is_numeric());
            let has_internal_upper = clean.chars().skip(1).any(|c| c.is_uppercase());
            let is_all_caps = clean.chars().any(|c| c.is_alphabetic())
                && clean
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .all(|c| c.is_uppercase());
            let proper = clean.chars().next().is_some_and(|c| c.is_uppercase());
            let lower = clean.to_lowercase();
            let sentence_leader = matches!(
                lower.as_str(),
                "a" | "an" | "the" | "this" | "that" | "these" | "those"
            );
            if run.is_empty()
                && sentence_leader
                && !has_digit
                && !has_internal_upper
                && !is_all_caps
            {
                continue;
            }
            if has_digit || has_internal_upper || is_all_caps || proper {
                run.push(lower);
                run_has_identifier |= has_digit || has_internal_upper || is_all_caps;
            } else {
                finish(&mut run, run_has_identifier, &mut phrases);
                run_has_identifier = false;
            }
        }
        finish(&mut run, run_has_identifier, &mut phrases);
    }
    let mut seen = HashSet::new();
    phrases.retain(|phrase| seen.insert(phrase.join(" ")));
    phrases
}

pub(in crate::features::conversation::chat::retrieval) fn find_entity_bridge(
    results: &[SearchResultDto],
    catalog: &[CorpusDocument],
    allowed_document_ids: &HashSet<String>,
    question: &str,
) -> Option<EntityBridge> {
    let question_terms: HashSet<String> = tokenize_keyword_terms(question).into_iter().collect();
    // The relationship-bearing opening can rank below several passages that
    // match the requested predicate (for example, generic provisions). Search
    // the complete bounded candidate set instead of making entity traversal
    // depend on an arbitrary raw-rank cutoff. The opening, question-overlap,
    // and whole-phrase title checks below remain the precision gates.
    for result in results {
        let Some(source_document_id) = result.document_id.as_ref() else {
            continue;
        };
        let Some(source_document) = catalog
            .iter()
            .find(|document| document.id == *source_document_id)
        else {
            continue;
        };
        // Search branches can carry the true within-document index in either
        // `position` or enrichment metadata. Fusion currently strips both, so
        // compare against the catalog's bounded copy of the real first chunk
        // as the production fallback. This still rejects an arbitrary later
        // passage that happens to mention a capitalised name.
        let chunk_index = result.position.or_else(|| {
            result
                .metadata
                .get("chunk_index")
                .and_then(serde_json::Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
        });
        let content = result.content.trim();
        let catalog_opening = source_document.opening.trim();
        let matches_catalog_opening = !content.is_empty()
            && !catalog_opening.is_empty()
            && (content.starts_with(catalog_opening) || catalog_opening.starts_with(content));
        if chunk_index.is_some_and(|index| index != 0)
            || (chunk_index.is_none() && !matches_catalog_opening)
        {
            continue;
        }
        let paragraph = first_body_paragraph(&result.content);
        if paragraph.is_empty() {
            continue;
        }
        let mut heading_terms: HashSet<String> =
            tokenize_keyword_terms(&result.title).into_iter().collect();
        for line in result
            .content
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('#'))
        {
            heading_terms.extend(tokenize_keyword_terms(line));
        }
        let content_terms: HashSet<String> = tokenize_keyword_terms(&result.content)
            .into_iter()
            .collect();
        if !heading_terms
            .iter()
            .any(|term| question_terms.contains(term))
            && !content_terms
                .iter()
                .any(|term| question_terms.contains(term))
        {
            continue;
        }

        let phrases: Vec<Vec<String>> = referenced_entity_phrases(&paragraph)
            .into_iter()
            .filter(|phrase| {
                !phrase.iter().all(|term| heading_terms.contains(term))
                    && !phrase.iter().all(|term| question_terms.contains(term))
            })
            .collect();
        if phrases.is_empty() {
            continue;
        }
        let predicate_terms = select_informative_terms(
            tokenize_keyword_terms(question)
                .into_iter()
                .filter(|term| !heading_terms.contains(term))
                .collect(),
            24,
        );
        let mut matching_targets: Vec<(&CorpusDocument, usize)> = catalog
            .iter()
            .filter(|document| {
                allowed_document_ids.contains(&document.id)
                    && document.id != *source_document_id
                    && {
                        let title_terms: HashSet<String> =
                            tokenize_keyword_terms(&document.name).into_iter().collect();
                        phrases
                            .iter()
                            .any(|phrase| phrase.iter().all(|term| title_terms.contains(term)))
                    }
            })
            .map(|document| {
                let document_terms: HashSet<String> = tokenize_keyword_terms(&format!(
                    "{} {} {}",
                    document.name,
                    document.opening,
                    document.sections.join(" ")
                ))
                .into_iter()
                .collect();
                let predicate_overlap = predicate_terms
                    .iter()
                    .filter(|term| document_terms.contains(*term))
                    .count();
                (document, predicate_overlap)
            })
            .collect();
        let best_overlap = matching_targets
            .iter()
            .map(|(_, overlap)| *overlap)
            .max()
            .unwrap_or(0);
        if best_overlap > 0 {
            matching_targets.retain(|(_, overlap)| *overlap == best_overlap);
        }
        let target_document_ids: HashSet<String> = matching_targets
            .into_iter()
            .map(|(document, _)| document.id.clone())
            .collect();
        if !target_document_ids.is_empty() {
            return Some(EntityBridge {
                source_result_id: result.id.clone(),
                source_document_id: source_document_id.clone(),
                source_heading_terms: heading_terms,
                target_document_ids,
            });
        }
    }
    None
}

fn documents_in_rank_order(results: &[SearchResultDto]) -> Vec<String> {
    let mut seen = HashSet::new();
    results
        .iter()
        .filter_map(|result| result.document_id.clone())
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn interleave(rankings: Vec<Vec<String>>) -> Vec<String> {
    let longest = rankings.iter().map(Vec::len).max().unwrap_or(0);
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for rank in 0..longest {
        for ranking in &rankings {
            if let Some(id) = ranking.get(rank) {
                if seen.insert(id.as_str()) {
                    merged.push(id.clone());
                }
            }
        }
    }
    merged
}

/// What a document is read in sequence with: its book when it has one,
/// otherwise a filename series such as "Manual 01", "Manual 02".
#[derive(Clone, PartialEq, Eq, Hash)]
enum Structure {
    Group(String),
    Series(String),
}

fn structure_of(document: &CorpusDocument) -> Option<Structure> {
    match &document.source_context {
        Some(context) => Some(Structure::Group(context.group.id.clone())),
        None => numbered_series_prefix(&document.name).map(Structure::Series),
    }
}

fn is_introduction(document: &CorpusDocument) -> bool {
    document.name.to_lowercase().contains("introduction")
        || document
            .opening
            .trim()
            .to_lowercase()
            .starts_with("introduction")
}

/// The documents a request to start at the beginning would open with: the
/// introduction, the first chapter, and the first document that is more than
/// a contents page or change log. `members` is in reading order.
fn front_matter<'a>(members: &[&'a CorpusDocument]) -> Vec<&'a CorpusDocument> {
    let introduction = members.iter().find(|d| is_introduction(d)).copied();
    let first_chapter = members
        .iter()
        .filter(|d| d.chapter_number.is_some())
        .min_by_key(|d| d.chapter_number)
        .copied();
    let first_substantive = members.iter().find(|d| !navigation_only(d)).copied();
    [introduction, first_chapter, first_substantive]
        .into_iter()
        .flatten()
        .collect()
}

/// Catalog documents in the order the planner should be offered them, best
/// first. The prompt takes rows from the front until its token budget runs
/// out; the budget is what the planner's window holds (see `health`), so a
/// planner with a larger window is shown more of the catalog.
///
/// Each search hit brings its reading-order neighbours, the documents either
/// side of it in its book or series, since a passage often continues into the
/// next file. A request to learn or start at the beginning also brings the
/// front matter of each book a hit belongs to. The rest of the catalog follows
/// in its own order, so any document the budget has room for can be opened.
/// With no ranking at all the catalog keeps its own order, which is how the
/// planner always saw it.
pub(super) fn shortlist<'a>(
    catalog: &'a [CorpusDocument],
    candidates: &[String],
    question: &str,
) -> Vec<&'a CorpusDocument> {
    if candidates.is_empty() {
        return catalog.iter().collect();
    }
    let by_id: HashMap<&str, &CorpusDocument> = catalog
        .iter()
        .map(|document| (document.id.as_str(), document))
        .collect();
    // The catalog is already in reading order: grouped by book, by explicit
    // position within an ordered book, and by name otherwise.
    let mut structures: HashMap<Structure, Vec<&CorpusDocument>> = HashMap::new();
    for document in catalog {
        if let Some(structure) = structure_of(document) {
            structures.entry(structure).or_default().push(document);
        }
    }
    let learning = whole_document_intent(question);
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    let mut push = |document: &'a CorpusDocument, ordered: &mut Vec<&'a CorpusDocument>| {
        if seen.insert(document.id.as_str()) {
            ordered.push(document);
        }
    };
    let mut introduced = HashSet::new();
    for id in candidates {
        let Some(document) = by_id.get(id.as_str()).copied() else {
            continue;
        };
        push(document, &mut ordered);
        let Some(structure) = structure_of(document) else {
            continue;
        };
        let Some(members) = structures.get(&structure) else {
            continue;
        };
        if learning && introduced.insert(structure.clone()) {
            for document in front_matter(members) {
                push(document, &mut ordered);
            }
        }
        if let Some(index) = members.iter().position(|member| member.id == document.id) {
            let before = index.checked_sub(1).and_then(|before| members.get(before));
            for neighbour in [before, members.get(index + 1)].into_iter().flatten() {
                push(neighbour, &mut ordered);
            }
        }
    }
    for document in catalog {
        push(document, &mut ordered);
    }
    ordered
}

#[cfg(test)]
mod entity_bridge_tests {
    use super::*;

    fn result(id: &str, document_id: &str, title: &str, content: &str) -> SearchResultDto {
        SearchResultDto {
            id: id.into(),
            title: title.into(),
            content: content.into(),
            score: 1.0,
            path: None,
            document_id: Some(document_id.into()),
            position: Some(0),
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata: HashMap::new(),
        }
    }

    fn document(id: &str, name: &str) -> CorpusDocument {
        CorpusDocument {
            id: id.into(),
            name: name.into(),
            opening: String::new(),
            sections: Vec::new(),
            chapter_number: None,
            source_context: None,
        }
    }

    #[test]
    fn prose_openings_work_for_markdown_and_plain_text_but_not_transcripts() {
        assert_eq!(
            first_body_paragraph(
                "# Project\n\n## Supply\nAcme Field Services handles visits.\n\nLater."
            ),
            "Acme Field Services handles visits."
        );
        assert_eq!(
            first_body_paragraph(
                "Project overview\n\nAcme Field Services handles visits.\n\nLater."
            ),
            "Acme Field Services handles visits."
        );
        assert!(first_body_paragraph("# Review\n\n[09:00] Ada: hello").is_empty());
        assert!(first_body_paragraph("# Parts\n\n| Part | Qty |\n| --- | --- |").is_empty());
    }

    #[test]
    fn entity_extraction_keeps_names_and_identifiers_not_title_case_noise() {
        let phrases = referenced_entity_phrases(
            "Logistics run through Northwind Field Services. The Supplier uses cabinet T-2194.",
        );
        assert!(phrases.contains(&vec!["northwind".into(), "field".into(), "services".into()]));
        assert!(phrases.contains(&vec!["t-2194".into()]));
        assert!(!phrases.iter().any(|phrase| phrase == &["supplier"]));
        assert!(!phrases.iter().any(|phrase| phrase == &["the", "supplier"]));
        assert!(!phrases.iter().any(|phrase| phrase == &["logistics"]));
    }

    #[test]
    fn entity_extraction_keeps_a_multiword_name_at_the_start_of_a_sentence() {
        let phrases = referenced_entity_phrases(
            "Acme Field Services handles every site visit. Logistics remain unchanged.",
        );

        assert!(phrases.contains(&vec!["acme".into(), "field".into(), "services".into()]));
        assert!(!phrases.iter().any(|phrase| phrase == &["logistics"]));
    }

    #[test]
    fn bridge_requires_the_whole_referenced_name_in_a_target_title() {
        let results = vec![result(
            "p0",
            "brief",
            "Project brief: North pilot",
            "# Project brief: North pilot\n\nAcme Field Services handles every site visit.",
        )];
        let catalog = vec![
            document("brief", "Project brief: North pilot"),
            document("agreement", "Master agreement: Acme Field Services"),
            document("wrong", "Acme advisory notes"),
        ];
        let allowed = catalog.iter().map(|doc| doc.id.clone()).collect();

        let bridge =
            find_entity_bridge(&results, &catalog, &allowed, "Who handles the North pilot?")
                .expect("named target");

        assert_eq!(bridge.source_result_id, "p0");
        assert_eq!(
            bridge.target_document_ids,
            HashSet::from(["agreement".into()])
        );
    }

    #[test]
    fn bridge_reads_the_opening_index_from_hybrid_enrichment_metadata() {
        let mut opening = result(
            "p0",
            "brief",
            "Project brief: North pilot",
            "# Project brief: North pilot\n\nAcme Field Services handles every site visit.",
        );
        opening.position = None;
        opening
            .metadata
            .insert("chunk_index".into(), serde_json::json!(0));
        let catalog = vec![
            document("brief", "Project brief: North pilot"),
            document("agreement", "Master agreement: Acme Field Services"),
        ];
        let allowed = catalog.iter().map(|doc| doc.id.clone()).collect();

        let bridge = find_entity_bridge(
            &[opening],
            &catalog,
            &allowed,
            "Who handles the North pilot?",
        )
        .expect("hybrid opening names a target");

        assert_eq!(bridge.source_result_id, "p0");
        assert_eq!(
            bridge.target_document_ids,
            HashSet::from(["agreement".into()])
        );
    }

    #[test]
    fn bridge_recognizes_a_fused_opening_from_the_catalog_prefix() {
        let mut opening = result(
            "p0",
            "brief",
            "Project brief: North pilot",
            "# Project brief: North pilot\n\nAcme Field Services handles every site visit. More details follow.",
        );
        opening.position = None;
        let mut brief = document("brief", "Project brief: North pilot");
        brief.opening =
            "# Project brief: North pilot\n\nAcme Field Services handles every site visit.".into();
        let catalog = vec![
            brief,
            document("agreement", "Master agreement: Acme Field Services"),
        ];
        let allowed = catalog.iter().map(|doc| doc.id.clone()).collect();

        let bridge = find_entity_bridge(
            &[opening],
            &catalog,
            &allowed,
            "Who handles the North pilot?",
        )
        .expect("fused result still matches the catalog opening");

        assert_eq!(bridge.source_result_id, "p0");
        assert_eq!(
            bridge.target_document_ids,
            HashSet::from(["agreement".into()])
        );
    }

    #[test]
    fn bridge_can_follow_a_question_related_opening_below_early_predicate_hits() {
        let mut results: Vec<SearchResultDto> = (0..8)
            .map(|index| {
                result(
                    &format!("noise-{index}"),
                    &format!("noise-doc-{index}"),
                    "Generic notice terms",
                    "# Generic notice terms\n\nEither party may provide written notice.",
                )
            })
            .collect();
        results.push(result(
            "project-opening",
            "brief",
            "Project brief: North pilot",
            "# Project brief: North pilot\n\nAcme Field Services handles every site visit.",
        ));
        let mut catalog: Vec<CorpusDocument> = (0..8)
            .map(|index| {
                document(
                    &format!("noise-doc-{index}"),
                    &format!("Generic notice terms {index}"),
                )
            })
            .collect();
        catalog.extend([
            document("brief", "Project brief: North pilot"),
            document("agreement", "Master agreement: Acme Field Services"),
        ]);
        let allowed = catalog.iter().map(|doc| doc.id.clone()).collect();

        let bridge = find_entity_bridge(
            &results,
            &catalog,
            &allowed,
            "How much notice applies to the North pilot?",
        )
        .expect("later relationship-bearing opening is still considered");

        assert_eq!(bridge.source_result_id, "project-opening");
        assert_eq!(
            bridge.target_document_ids,
            HashSet::from(["agreement".into()])
        );
    }

    #[test]
    fn bridge_uses_the_question_predicate_to_disambiguate_same_entity_documents() {
        let results = vec![result(
            "project-opening",
            "brief",
            "Project brief: North pilot",
            "# Project brief: North pilot\n\nNorthwind Field Services handles every site visit.",
        )];
        let mut agreement = document("agreement", "Master agreement: Northwind Field Services");
        agreement.opening =
            "The agreement renews annually unless either party gives written notice.".into();
        let mut calibration =
            document("calibration", "Northwind Field Services calibration record");
        calibration.opening = "Probe tolerances and the most recent inspection date.".into();
        let catalog = vec![
            document("brief", "Project brief: North pilot"),
            calibration,
            agreement,
        ];
        let allowed = catalog.iter().map(|doc| doc.id.clone()).collect();

        let bridge = find_entity_bridge(
            &results,
            &catalog,
            &allowed,
            "How much notice applies to the North pilot?",
        )
        .expect("the relationship-bearing opening names candidate documents");

        assert_eq!(
            bridge.target_document_ids,
            HashSet::from(["agreement".into()])
        );
    }
}
