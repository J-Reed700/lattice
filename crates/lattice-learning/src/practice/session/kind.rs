//! A kind of work, as a record of small roles. The engine calls these and nothing else.

use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{
    Brief, Case, CaseResult, Change, CheckReport, Draft, Exercise, Form, KindId, PartRef, Piece,
    Spacing, Stage,
};
use lattice_core::shared::error::{AppError, Result};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Decides which cases a piece passes.
#[async_trait]
pub trait Checker: Send + Sync {
    async fn check(
        &self,
        brief: &Brief,
        piece: &Piece,
        cases: &[Case],
        cancel: CancellationToken,
    ) -> Result<CheckReport>;
}

/// The addressable units of a piece.
pub trait Parts: Send + Sync {
    fn all(&self, brief: &Brief, piece: &Piece) -> Result<Vec<PartRef>>;
    fn holds(&self, brief: &Brief, piece: &Piece, part: &PartRef) -> Result<bool>;
    /// The part's display name.
    fn name(&self, brief: &Brief, piece: &Piece, part: &PartRef) -> Result<String>;
}

/// The changes between two drafts, in the kind's own units.
pub trait Diff: Send + Sync {
    fn changes(&self, brief: &Brief, old: &Piece, new: &Piece) -> Result<Vec<Change>>;
}

/// Text for a model.
pub trait Showing: Send + Sync {
    /// The brief as text. Never the reference.
    fn brief(&self, brief: &Brief) -> Result<String>;
    fn piece(&self, brief: &Brief, piece: &Piece) -> Result<String>;
    fn result(&self, brief: &Brief, case: &Case, result: &CaseResult) -> Result<String>;
}

/// The words for this work.
pub trait Wording: Send + Sync {
    fn card(&self) -> String;
    fn stage_label(&self, brief: &Brief, form: Form, stage: Stage) -> String;
}

/// Enables Complete: a piece with parts emptied, and with one part put back.
pub trait Gaps: Send + Sync {
    fn emptied(&self, brief: &Brief, piece: &Piece, parts: &[PartRef]) -> Result<Piece>;
    fn restored(&self, brief: &Brief, piece: &Piece, from: &Piece, part: &PartRef)
        -> Result<Piece>;
}

/// Enables Arrange: a piece with its parts in a seeded order.
pub trait Order: Send + Sync {
    fn shuffled(&self, brief: &Brief, piece: &Piece, seed: u64) -> Result<Piece>;
}

/// What a kind found when it established a draft's reference.
pub enum Established {
    Right {
        cases: Vec<Case>,
        proof: serde_json::Value,
    },
    /// Why the reference is not right, written for the model that drafted it.
    Wrong(String),
}

/// Where the kind's truth comes from.
#[async_trait]
pub trait Truth: Send + Sync {
    /// Whether the draft's reference is right. `cases` may come back enriched (a digest a run produced).
    async fn establish(&self, draft: &Draft, cancel: CancellationToken) -> Result<Established>;

    /// Why the exercise has more than one right answer, if it has.
    async fn unambiguous(
        &self,
        exercise: &Exercise,
        cancel: CancellationToken,
    ) -> Result<Option<String>>;
}

/// One kind of work.
pub struct Kind {
    pub id: KindId,
    pub spacing: Spacing,
    /// The misreadings a proven exercise must kill. Each kind states its reason where it sets it.
    pub misreadings_needed: usize,
    pub wording: Arc<dyn Wording>,
    pub checker: Arc<dyn Checker>,
    pub parts: Arc<dyn Parts>,
    pub diff: Arc<dyn Diff>,
    pub showing: Arc<dyn Showing>,
    pub truth: Arc<dyn Truth>,
    pub gaps: Option<Arc<dyn Gaps>>,
    pub order: Option<Arc<dyn Order>>,
}

impl Kind {
    /// Whether the kind has the roles `form` needs.
    pub fn supports(&self, form: Form) -> bool {
        match form {
            Form::Build | Form::Repair => true,
            Form::Complete => self.gaps.is_some(),
            Form::Arrange => self.order.is_some(),
        }
    }
}

/// Every kind the app offers, built once in the composition root.
pub struct Kinds(BTreeMap<KindId, Kind>);

impl Kinds {
    /// A repeated id is an error.
    pub fn new(kinds: impl IntoIterator<Item = Kind>) -> Result<Self> {
        let mut by_id = BTreeMap::new();
        for kind in kinds {
            let id = kind.id.clone();
            if by_id.insert(id.clone(), kind).is_some() {
                return Err(AppError::InvalidState(format!(
                    "The kind {} is registered twice.",
                    id.as_str()
                )));
            }
        }
        Ok(Self(by_id))
    }

    pub fn get(&self, id: &KindId) -> Result<&Kind> {
        self.0.get(id).ok_or_else(|| {
            AppError::InvalidState(format!("No kind of work is registered as {}.", id.as_str()))
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &Kind> {
        self.0.values()
    }
}
