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
        matches!(
            cited.as_slice(),
            [repair::Cited::Cannot(repair::Unwritten::NotInSection)]
        ),
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
    let finding = found_in(&sources[0], GOOD, (GOOD, GOOD));
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
fn a_finding_that_quotes_several_sentences_is_not_cited_and_says_why() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let body = format!("{NONE} {GOOD}");
    let finding = found_in(&sources[0], GOOD, (body.as_str(), GOOD));
    let lesson = csv_lesson(&body);
    let mut candidate = lesson.clone();
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
    assert_eq!(candidate, lesson);
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

#[test]
fn a_sentence_that_cites_the_passage_already_is_not_cited_again_and_says_why() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = found_in(&sources[0], GOOD, (GOOD, GOOD));
    let mut lesson = csv_lesson(GOOD);
    lesson["blocks"][0]["citations"] = json!([{"sentence":GOOD,"sourceIndex":0,"quote":GOOD}]);
    let mut candidate = lesson.clone();

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
        [repair::Cited::Cannot(repair::Unwritten::AlreadyCited)]
    ));
    assert_eq!(candidate, lesson);
    Ok(())
}

#[test]
fn a_citation_written_before_and_a_finding_not_supported_elsewhere_say_why() -> Result<()> {
    let sources = [source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = found_in(&sources[0], GOOD, (GOOD, GOOD));
    let unsupported = Finding {
        verdict: ClaimVerdict::Unsupported,
        ..finding.clone()
    };
    let mut candidate = csv_lesson(GOOD);
    let mut numbered = sources.to_vec();
    let mut tried = HashSet::new();
    let mut cite = |finding: &Finding| -> Result<Vec<repair::Cited>> {
        repair::cite_where_supported(
            &mut candidate,
            std::slice::from_ref(finding),
            &[],
            &mut numbered,
            &references,
            &mut tried,
        )
    };

    assert!(matches!(
        cite(&unsupported)?.as_slice(),
        [repair::Cited::Cannot(
            repair::Unwritten::NotSupportedElsewhere
        )]
    ));
    assert!(matches!(
        cite(&finding)?.as_slice(),
        [repair::Cited::Recited(_)]
    ));
    // A second call finds the citation written, and the job's own record of it.
    assert!(matches!(
        cite(&finding)?.as_slice(),
        [repair::Cited::Cannot(repair::Unwritten::WrittenBefore)]
    ));
    Ok(())
}

const OTHER: &str = "The optional skipinitialspace setting defaults to false.";
