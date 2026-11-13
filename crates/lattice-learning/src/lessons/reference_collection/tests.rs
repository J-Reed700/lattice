use super::test_support::collection_fixture;
pub(crate) use super::test_support::FakeLibrary;
use super::*;
use crate::tests;
use lattice_core::shared::persistence::Database;
use std::collections::HashSet;

#[test]
fn verification_windows_restore_conditions_without_changing_source_bytes() -> Result<()> {
    let before = "This procedure applies only while the local setting is active.\n";
    let target = "The selected value follows the local setting.";
    let after = "\nAn explicit override takes precedence over that setting.";
    let text = format!(
        "{before}{}{target}{}{after}",
        "背景🌱 ".repeat(150),
        " context ".repeat(100)
    );
    let source = LearningSourceDto {
        id: "reference".into(),
        title: "Procedure".into(),
        url: None,
        excerpt: text.clone(),
        acquired_at: 1,
    };
    let collection = ReferenceCollection::lexical(&[source])?;
    let start = text.find(target).unwrap();
    let hit = ReferencePassage {
        source_id: "reference".into(),
        text: target.into(),
        start_byte: start,
        end_byte: start + target.len(),
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
    };
    let expanded = collection.verification_context(vec![hit.clone(), hit.clone()])?;
    assert_eq!(
        expanded.len(),
        1,
        "Overlapping context must not crowd out independent sources"
    );
    assert!(expanded[0].text.contains(before));
    assert!(expanded[0].text.contains(after));
    assert_eq!(
        &text[expanded[0].start_byte..expanded[0].end_byte],
        expanded[0].text
    );
    assert_eq!(expanded[0].score, hit.score);
    let mut invented = hit;
    invented.text = "A fabricated quotation".into();
    let rebuilt = collection.verification_context(vec![invented])?;
    assert_eq!(
        &text[rebuilt[0].start_byte..rebuilt[0].end_byte],
        rebuilt[0].text
    );
    Ok(())
}

#[test]
fn merging_context_keeps_both_partly_overlapping_hits() -> Result<()> {
    let text = format!(
        "{}first matching fact. {}second matching caveat. {}",
        "préfixe ".repeat(400),
        "Background sentence here. ".repeat(105),
        "suffix ".repeat(400)
    );
    let source = LearningSourceDto {
        id: "reference".into(),
        title: "Procedure".into(),
        url: None,
        excerpt: text.clone(),
        acquired_at: 1,
    };
    let collection = ReferenceCollection::lexical(&[source])?;
    let passages = ["first matching fact", "second matching caveat"].map(|target| {
        let start = text.find(target).unwrap();
        ReferencePassage {
            source_id: "reference".into(),
            text: target.into(),
            start_byte: start,
            end_byte: start + target.len(),
            retrieval_kind: "lexical_fallback".into(),
            score: 1.0,
        }
    });
    let expanded = collection.verification_context(passages.into())?;
    assert_eq!(expanded.len(), 1);
    assert!(expanded[0].text.contains("first matching fact"));
    assert!(expanded[0].text.contains("second matching caveat"));
    assert_eq!(
        &text[expanded[0].start_byte..expanded[0].end_byte],
        expanded[0].text
    );
    Ok(())
}

fn source_of(id: &str, markers: &[&str]) -> LearningSourceDto {
    let filler = "Background sentence here. ".repeat(58);
    let mut excerpt = filler.clone();
    for marker in markers {
        excerpt.push_str(&format!("{marker} {filler}"));
    }
    LearningSourceDto {
        id: id.into(),
        title: format!("Reference {id}"),
        url: None,
        excerpt,
        acquired_at: 1,
    }
}

fn hit_on(source: &LearningSourceDto, target: &str) -> ReferencePassage {
    let start = source.excerpt.find(target).unwrap();
    ReferencePassage {
        source_id: source.id.clone(),
        text: target.into(),
        start_byte: start,
        end_byte: start + target.len(),
        retrieval_kind: "lexical_fallback".into(),
        score: 1.0,
    }
}

fn window_of(source: &LearningSourceDto, hit: &ReferencePassage) -> std::ops::Range<usize> {
    verification_window(&source.excerpt, hit.start_byte..hit.end_byte)
}

#[test]
fn adjacent_hits_of_one_source_keep_to_their_own_windows() -> Result<()> {
    let targets = [
        "alpha marker fact.",
        "beta marker fact.",
        "gamma marker fact.",
        "delta marker fact.",
        "epsilon marker fact.",
    ];
    let source = source_of("long", &targets);
    let collection = ReferenceCollection::lexical(std::slice::from_ref(&source))?;
    let hits: Vec<_> = targets.iter().map(|t| hit_on(&source, t)).collect();
    let windows: Vec<_> = hits.iter().map(|hit| window_of(&source, hit)).collect();
    assert!(windows.windows(2).all(|w| w[1].start < w[0].end));
    let passages = collection.verification_context(hits)?;
    assert_eq!(passages.len(), targets.len());
    for (passage, window) in passages.iter().zip(&windows) {
        assert!(passage.start_byte >= window.start && passage.end_byte <= window.end);
        assert_eq!(
            &source.excerpt[passage.start_byte..passage.end_byte],
            passage.text
        );
    }
    for (i, one) in passages.iter().enumerate() {
        for other in &passages[i + 1..] {
            assert!(!one.overlaps(other));
        }
    }
    let covered = |spans: &mut dyn Iterator<Item = (usize, usize)>| {
        spans
            .flat_map(|(start, end)| start..end)
            .collect::<HashSet<_>>()
    };
    assert_eq!(
        covered(&mut passages.iter().map(|p| (p.start_byte, p.end_byte))),
        covered(&mut windows.iter().map(|w| (w.start, w.end)))
    );
    Ok(())
}

#[test]
fn a_second_sources_hit_keeps_its_place_between_a_first_sources_hits() -> Result<()> {
    let first = source_of(
        "a",
        &[
            "alpha marker fact.",
            "beta marker fact.",
            "gamma marker fact.",
        ],
    );
    let second = source_of("b", &["other marker fact."]);
    let collection = ReferenceCollection::lexical(&[first.clone(), second.clone()])?;
    let a1 = hit_on(&first, "alpha marker fact.");
    let b1 = hit_on(&second, "other marker fact.");
    let a2 = hit_on(&first, "beta marker fact.");
    let a3 = hit_on(&first, "gamma marker fact.");
    let (w1, w2, w3) = (
        window_of(&first, &a1),
        window_of(&first, &a2),
        window_of(&first, &a3),
    );
    assert!(w2.start < w1.end && w3.start < w2.end);
    let passages = collection.verification_context(vec![a1, b1.clone(), a2, a3])?;
    let spans: Vec<_> = passages
        .iter()
        .map(|p| (p.source_id.as_str(), p.start_byte..p.end_byte))
        .collect();
    assert_eq!(
        spans,
        vec![
            ("a", w1.clone()),
            ("b", window_of(&second, &b1)),
            ("a", w1.end..w2.end),
            ("a", w2.end..w3.end),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn without_a_course_index_every_snapshot_is_ranked_by_its_words() -> Result<()> {
    let db = Database::new(tests::pool().await?);
    let (_, sources) = collection_fixture(&db, ["doc-ownership", "doc-plants"]).await?;
    let lexical = ReferenceCollection::current(None, &sources).await?;
    assert!(lexical.retrieve("sunlight conversion", 3).await?.is_empty());
    let hits = lexical.retrieve("photosynthesis", 3).await?;
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| h.retrieval_kind == "lexical"));
    assert_eq!(lexical.mode().label(), "lexical");
    assert_eq!(lexical.embedding_model(), None);
    Ok(())
}

#[tokio::test]
async fn a_ranking_keeps_to_its_scope_and_records_its_branches() -> Result<()> {
    let db = Database::new(tests::pool().await?);
    let (_, sources) = collection_fixture(&db, ["doc-ownership", "doc-plants"]).await?;
    let collection = ReferenceCollection::lexical(&sources)?;
    let plants = sources
        .iter()
        .find(|source| source.excerpt.contains("chloroplasts"))
        .unwrap()
        .id
        .clone();
    let scope = Scope::Keys(HashSet::from([plants.clone()]));
    let ranked = collection
        .rank(&["photosynthesis", "borrowing"], &scope, 4)
        .await?;
    assert!(!ranked.is_empty());
    assert!(ranked.iter().all(|hit| hit.passage.source_id == plants));
    assert!(ranked
        .iter()
        .all(|hit| hit.lexical.is_some() && hit.dense.is_none()));
    assert!(collection
        .rank(
            &["photosynthesis"],
            &Scope::Keys(HashSet::from(["elsewhere".to_string()])),
            4
        )
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn large_collection_keeps_evidence_after_the_former_twenty_million_character_boundary(
) -> Result<()> {
    let suffix = "Unique nebular measurement equals forty seven units.";
    let source = LearningSourceDto {
        id: "large-reference".into(),
        title: "Large reference".into(),
        url: None,
        excerpt: format!("{}\n{suffix}", "background ".repeat(1_820_000)),
        acquired_at: 0,
    };
    assert!(source.excerpt.len() > 20_000_000);
    let collection = ReferenceCollection::lexical(&[source])?;
    let hits = collection.retrieve("nebular measurement", 1).await?;
    assert!(hits[0].text.contains(suffix));
    assert_eq!(
        &collection.sources[0].excerpt[hits[0].start_byte..hits[0].end_byte],
        hits[0].text
    );
    Ok(())
}

#[test]
fn unicode_passage_offsets_remain_lossless_without_a_whole_document_offset_array() {
    use crate::references::course_index::{chunks, CHUNK_CHARS};
    let text = "🧪é中sample\n".repeat(400);
    let passages = chunks(&text);
    assert!(passages.len() > 1);
    for passage in &passages {
        assert_eq!(
            text.get(passage.start..passage.end),
            Some(passage.text.as_str())
        );
        assert!(passage.text.chars().count() <= CHUNK_CHARS);
    }
    assert!(passages.last().unwrap().end == text.trim_end().len());
}

#[tokio::test]
async fn deeper_evidence_must_mention_what_the_claim_is_about() -> Result<()> {
    let sources: Vec<LearningSourceDto> = [
        "ASP.NET Core hosts the app. ASP.NET Core configures logging.",
        "ASP.NET Core runs middleware in order for each request.",
        "ASP.NET Core minimal APIs support endpoint filters.",
    ]
    .iter()
    .enumerate()
    .map(|(i, text)| LearningSourceDto {
        id: format!("s{i}"),
        title: format!("Reference {i}"),
        url: None,
        excerpt: (*text).into(),
        acquired_at: 1,
    })
    .collect();
    let collection = ReferenceCollection::lexical(&sources)?;
    let claim = "ASP.NET Core endpoint filters run before the handler.";
    let subject = collection.subject_of(claim).await?;
    // Shares only the subject's common words.
    assert!(!subject.mentioned_in("ASP.NET Core runs middleware in order."));
    // Mentions what the claim is about.
    assert!(subject.mentioned_in("Endpoint filters wrap a minimal API handler."));
    // A page sharing nothing with the claim stops the walk.
    assert!(!collection
        .subject_of("ASP.NET Core")
        .await?
        .mentioned_in("Vacuum reclaims storage."));
    Ok(())
}

#[tokio::test]
async fn keyword_ranking_is_the_same_on_every_build_and_call() -> Result<()> {
    // Near-tied passages: the same terms, in different amounts.
    let words = ["schema", "tool", "enum", "value", "model", "call", "field"];
    let sources: Vec<_> = (0..24)
        .map(|index| LearningSourceDto {
            id: format!("source-{index}"),
            title: format!("Source {index}"),
            url: None,
            excerpt: words
                .iter()
                .enumerate()
                .map(|(at, word)| format!("{word} ").repeat(1 + (index + at) % 4))
                .collect(),
            acquired_at: 1,
        })
        .collect();
    let query = words.join(" ");
    let order = |hits: Vec<ReferencePassage>| {
        hits.into_iter()
            .map(|hit| (hit.source_id, hit.start_byte))
            .collect::<Vec<_>>()
    };
    let first = order(
        ReferenceCollection::lexical(&sources)?
            .retrieve(&query, 24)
            .await?,
    );
    for _ in 0..16 {
        let rebuilt = ReferenceCollection::lexical(&sources)?;
        assert_eq!(order(rebuilt.retrieve(&query, 24).await?), first);
    }
    Ok(())
}

#[tokio::test]
async fn retrieve_from_reads_one_source_version_only() -> Result<()> {
    let source = |id: &str, text: &str| LearningSourceDto {
        id: id.into(),
        title: id.into(),
        url: None,
        excerpt: text.into(),
        acquired_at: 1,
    };
    let collection = ReferenceCollection::lexical(&[
        source(
            "course-notes",
            "Water boils at 100 degrees Celsius at sea level, the notes say.",
        ),
        source(
            "added-by-you",
            "At sea level pure water boils at 100 degrees Celsius; on a mountain it boils lower.",
        ),
    ])?;
    let passages = collection
        .retrieve_from("added-by-you", "water boils at sea level", |_| true)
        .await?;
    assert!(!passages.is_empty());
    assert!(passages
        .iter()
        .all(|passage| passage.source_id == "added-by-you"));
    assert!(collection
        .retrieve_from("not-in-the-course", "water boils", |_| true)
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn the_first_pace_counts_from_when_embedding_began() {
    let ((), stages) = crate::lesson_progress::staged(async {
        let mut indexing = IndexingStages::default();
        indexing.embedding(0, 64);
        std::thread::sleep(std::time::Duration::from_millis(200));
        indexing.embedded(32, 64);
    })
    .await;
    let text = stages.last().expect("a stage was shown");
    let pace: Option<f64> = text
        .rsplit(" · ")
        .next()
        .and_then(|segment| segment.strip_suffix(" passages/s"))
        .and_then(|number| number.parse().ok());
    assert!(pace.is_some_and(|pace| pace <= 320.0), "{text:?}");
}

#[tokio::test]
async fn term_weights_are_the_bm25_idf_of_each_query_term_over_the_passages() -> Result<()> {
    let sources: Vec<LearningSourceDto> = [
        "Alpha beta gamma.",
        "Alpha beta delta.",
        "Alpha epsilon zeta.",
        "Alpha eta theta.",
        "Alpha iota kappa.",
    ]
    .iter()
    .enumerate()
    .map(|(i, text)| LearningSourceDto {
        id: format!("s{i}"),
        title: format!("Reference {i}"),
        url: None,
        excerpt: (*text).into(),
        acquired_at: 1,
    })
    .collect();
    let collection = ReferenceCollection::lexical(&sources)?;
    let weights = collection
        .term_weights("Alpha beta GAMMA omega x gamma")
        .await?;
    let idf = |passages: f64, holding: f64| ((passages - holding + 0.5) / (holding + 0.5)).ln();
    let close = |found: Option<&f64>, expected: f64| {
        found.is_some_and(|found| (found - expected).abs() < 1e-12)
    };
    assert!(close(weights.get("gamma"), idf(5.0, 1.0)), "{weights:?}");
    assert!(close(weights.get("beta"), idf(5.0, 2.0)), "{weights:?}");
    assert!(close(weights.get("alpha"), 1e-6), "{weights:?}");
    assert!(!weights.contains_key("omega"), "{weights:?}");
    assert_eq!(weights.len(), 3, "{weights:?}");
    Ok(())
}

/// 120 sentences with no line break and irregular lengths; sentence 60 is the
/// only one that says `zebra`. Also their start and end offsets.
fn sentence_source() -> (String, Vec<usize>, Vec<usize>, String) {
    use lattice_core::shared::text::segmentation::sentences;
    let said: Vec<String> = (0..120)
        .map(|i| {
            let word = if i == 60 { "zebra" } else { "plainly" };
            format!("Sentence {i} records fact {} {word}.", "x".repeat(i % 7))
        })
        .collect();
    let excerpt = said.join(" ");
    let spans = sentences(&excerpt);
    let starts = spans.iter().map(|span| span.0).collect();
    let ends = spans.iter().map(|span| span.1).collect();
    (excerpt, starts, ends, said[60].clone())
}

fn single(excerpt: &str) -> LearningSourceDto {
    LearningSourceDto {
        id: "reference".into(),
        title: "Reference".into(),
        url: None,
        excerpt: excerpt.into(),
        acquired_at: 1,
    }
}

#[tokio::test]
async fn a_retrieved_passage_starts_and_ends_on_sentence_breaks() -> Result<()> {
    let (excerpt, starts, ends, _) = sentence_source();
    let collection = ReferenceCollection::lexical(&[single(&excerpt)])?;
    let found = collection.retrieve_within("zebra", |_| true).await?;
    let holding = found
        .iter()
        .find(|passage| passage.text.contains("zebra"))
        .expect("the passage holding zebra");
    assert!(
        starts.contains(&holding.start_byte),
        "{}",
        holding.start_byte
    );
    assert!(ends.contains(&holding.end_byte), "{}", holding.end_byte);
    Ok(())
}

#[test]
fn a_verification_window_is_whole_sentences() -> Result<()> {
    let (excerpt, starts, ends, sentence) = sentence_source();
    let collection = ReferenceCollection::lexical(&[single(&excerpt)])?;
    let cited = collection.cited_passages(&[sentence.as_str()])?;
    assert_eq!(cited.len(), 1);
    assert!(
        starts.contains(&cited[0].start_byte),
        "{}",
        cited[0].start_byte
    );
    assert!(ends.contains(&cited[0].end_byte), "{}", cited[0].end_byte);
    Ok(())
}

#[test]
fn a_quote_in_other_letter_case_cites_its_passage() -> Result<()> {
    let excerpt = "The borrow checker rejects two mutable borrows of one value at once.";
    let collection = ReferenceCollection::lexical(&[single(excerpt)])?;
    let cited = collection
        .cited_passages(&["the Borrow Checker rejects two mutable borrows of one value"])?;
    assert_eq!(cited.len(), 1);
    assert!(cited[0]
        .text
        .contains("The borrow checker rejects two mutable borrows"));
    Ok(())
}
