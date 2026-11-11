#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::session::fake_kind::{
    fake_kind, letters_exercise, letters_observation, rung_name, ScriptedModel,
};
use lattice_core::domain::learning::practice::work::{PartRef, Rung, Setting};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

const LADDER: [Rung; 4] = [Rung::Nudge, Rung::Idea, Rung::Plan, Rung::Point];

#[test]
fn the_next_rung_is_the_one_above_the_highest_given() {
    assert_eq!(next_rung([]), Rung::Nudge);
    assert_eq!(next_rung([Rung::Nudge]), Rung::Idea);
    assert_eq!(next_rung([Rung::Idea]), Rung::Plan);
    assert_eq!(next_rung([Rung::Plan]), Rung::Point);
    assert_eq!(next_rung([Rung::Point]), Rung::Point);
}

#[test]
fn the_highest_rung_decides_whatever_the_order() {
    assert_eq!(
        next_rung([Rung::Plan, Rung::Nudge, Rung::Idea]),
        Rung::Point
    );
    assert_eq!(
        next_rung([Rung::Nudge, Rung::Idea, Rung::Plan]),
        Rung::Point
    );
    assert_eq!(
        next_rung([Rung::Idea, Rung::Nudge, Rung::Nudge]),
        Rung::Plan
    );
    assert_eq!(next_rung([Rung::Point, Rung::Nudge]), Rung::Point);
}

async fn given(kind: &Kind, rung: Rung) -> Result<Hint> {
    give(
        &ScriptedModel::new([]),
        kind,
        &letters_exercise(),
        &letters_observation(Setting::Coached),
        rung,
        false,
        CancellationToken::new(),
    )
    .await
}

#[tokio::test]
async fn the_asked_rung_comes_back_when_the_kind_can_make_it() {
    let kind = fake_kind(&["a", "b", "c"]).build();
    for rung in LADDER {
        let hint = given(&kind, rung).await.unwrap();
        assert_eq!(hint.rung, rung);
        assert_eq!(hint.text, format!("A {} hint.", rung_name(rung)));
        assert_eq!(hint.parts, vec![PartRef::new("1")]);
    }
}

#[tokio::test]
async fn a_rung_the_kind_cannot_make_gives_the_next_one_and_says_so() {
    let kind = fake_kind(&["a", "b", "c"])
        .without_rung(Rung::Nudge)
        .build();
    let hint = given(&kind, Rung::Nudge).await.unwrap();
    assert_eq!(hint.rung, Rung::Idea);
    assert_eq!(hint.text, "A idea hint.");

    let kind = fake_kind(&["a", "b", "c"])
        .without_rung(Rung::Nudge)
        .without_rung(Rung::Idea)
        .build();
    assert_eq!(given(&kind, Rung::Nudge).await.unwrap().rung, Rung::Plan);
    assert_eq!(given(&kind, Rung::Idea).await.unwrap().rung, Rung::Plan);
    assert_eq!(given(&kind, Rung::Plan).await.unwrap().rung, Rung::Plan);
}

#[tokio::test]
async fn nothing_from_the_asked_rung_up_is_an_error() {
    let kind = fake_kind(&["a", "b", "c"])
        .without_rung(Rung::Plan)
        .without_rung(Rung::Point)
        .build();
    let result = given(&kind, Rung::Plan).await;
    assert!(
        matches!(&result, Err(AppError::InvalidState(why)) if why == "No hint can be made for this exercise."),
        "{result:?}"
    );
    assert!(given(&kind, Rung::Point).await.is_err());
    assert_eq!(given(&kind, Rung::Idea).await.unwrap().rung, Rung::Idea);

    let nothing = LADDER
        .into_iter()
        .fold(fake_kind(&["a", "b", "c"]), |kind, rung| {
            kind.without_rung(rung)
        })
        .build();
    assert!(given(&nothing, Rung::Nudge).await.is_err());
}

#[tokio::test]
async fn the_kind_is_handed_the_observation_as_the_kind_renders_it() {
    let (kind, observed) = fake_kind(&["a", "b", "c"])
        .without_rung(Rung::Nudge)
        .build_recording();
    given(&kind, Rung::Nudge).await.unwrap();
    let expected = letters_observation(Setting::Coached).render(&kind).unwrap();
    let observed = observed.lock().unwrap().clone();
    assert_eq!(observed, vec![expected.clone(), expected]);
}
