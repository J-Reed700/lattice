#![allow(clippy::unwrap_used)]
use super::*;
use crate::dto::LearningSourceKind;
use crate::lessons::claim_reviews::dto::SourceAddedByDto;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimId, ClaimLocation, EventProvenance, UnitKind,
};

const FIELD: &str = "It\u{2019}s owned by the caller.";

fn entry() -> ClaimLedgerEntry {
    ClaimLedgerEntry::extracted(
        ClaimId("c1".into()),
        EventProvenance {
            actor: Actor::Machine,
            at: 1,
            job_id: None,
            app_version: "test".into(),
            checker_model: None,
        },
        ClaimLocation::locate(0, UnitKind::Block, "body", FIELD, FIELD),
        FIELD.into(),
    )
    .unwrap()
}

#[test]
fn a_stored_quote_in_other_typography_is_highlighted() {
    let shown = [ShownPassage {
        source_version_id: "v1".into(),
        start_byte: 0,
        end_byte: FIELD.len(),
        text: FIELD.into(),
        retrieval_kind: "lexical".into(),
    }];
    let sources = HashMap::from([(
        "v1".to_owned(),
        SourceFacts {
            source_id: "s1".into(),
            title: "Source".into(),
            url: None,
            added_by: SourceAddedByDto::User,
            library_kind: LearningSourceKind::Pasted,
        },
    )]);
    let texts = HashMap::new();
    let standings = HashMap::new();
    let context = SupportContext {
        shown: &shown,
        texts: &texts,
        sources: &sources,
        standings: &standings,
    };
    let card = support(
        &entry(),
        &context,
        "v1",
        ClaimSupportRoleDto::Supports,
        None,
        ClaimSupportCountsDto::NotEnough,
        "It's owned by the caller".into(),
    )
    .unwrap();
    assert_eq!(card.quote_span, Some((0, 26)));
}
