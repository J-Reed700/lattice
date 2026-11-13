//! A citation the repair adds is an entry of the section's own sentence, or none.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

#[test]
fn a_sentence_the_section_does_not_say_is_not_cited() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = found_in(&sources[0], GOOD, (GOOD, GOOD));
    let lesson = csv_lesson("Alpha holds the line.");
    let mut candidate = lesson.clone();
    let mut numbered = Vec::new();
    let cited = repair::cite_where_supported(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut numbered,
        &references,
        &mut HashSet::new(),
    )?;
    assert!(
        matches!(cited.as_slice(), [repair::Cited::Cannot]),
        "nothing is cited"
    );
    assert_eq!(candidate, lesson);
    assert!(numbered.is_empty(), "no source is numbered");
    Ok(())
}

#[test]
fn the_entry_holds_the_exact_sentence_of_the_body() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let body = format!("{NONE} {GOOD}");
    let finding = found_in(&sources[0], GOOD, (body.as_str(), GOOD));
    let mut candidate = csv_lesson(&body);
    let cited = repair::cite_where_supported(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut sources.to_vec(),
        &references,
        &mut HashSet::new(),
    )?;
    assert!(matches!(cited.as_slice(), [repair::Cited::Recited(_)]));
    assert_eq!(
        candidate["blocks"][0]["citations"],
        json!([{"sentence":GOOD,"sourceIndex":0,"quote":GOOD}])
    );
    Ok(())
}

#[test]
fn a_statement_naming_context_its_sentence_leaves_implicit_is_cited_at_its_sentence() -> Result<()>
{
    assert!(lattice_core::shared::text::claim_sentence(
        IMPLICIT_SENTENCE,
        (0, IMPLICIT_SENTENCE.len()),
        IMPLICIT_STATEMENT
    )
    .is_none());
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = found_in(&sources[0], GOOD, (IMPLICIT_SENTENCE, IMPLICIT_STATEMENT));
    let mut candidate = csv_lesson(IMPLICIT_SENTENCE);

    let cited = repair::cite_where_supported(
        &mut candidate,
        std::slice::from_ref(&finding),
        &[],
        &mut sources.to_vec(),
        &references,
        &mut HashSet::new(),
    )?;

    assert!(
        matches!(cited.as_slice(), [repair::Cited::Recited(_)]),
        "the sentence is cited"
    );
    assert_eq!(
        candidate["blocks"][0]["citations"],
        json!([{"sentence":IMPLICIT_SENTENCE,"sourceIndex":0,"quote":GOOD}])
    );
    Ok(())
}

#[test]
fn two_sentences_of_a_section_are_both_cited_at_one_passage() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let body = format!("{NONE} {OTHER}");
    let findings = [
        found_in(&sources[0], GOOD, (NONE, NONE)),
        found_in(&sources[0], GOOD, (OTHER, OTHER)),
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
        "the second sentence is cited at the passage the first was"
    );
    assert_eq!(
        candidate["blocks"][0]["citations"],
        json!([
            {"sentence":NONE,"sourceIndex":0,"quote":GOOD},
            {"sentence":OTHER,"sourceIndex":0,"quote":GOOD},
        ])
    );
    Ok(())
}

const OTHER: &str = "The optional skipinitialspace setting defaults to false.";
