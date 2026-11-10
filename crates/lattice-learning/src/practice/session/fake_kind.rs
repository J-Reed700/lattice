//! A kind whose piece is a JSON array of strings, so the engine's tests need no code runner and no subject.
//!
//! A case's body is `{"index": i, "expected": "x"}`; it passes when the piece holds that string at that index.

use crate::practice::session::kind::{
    Checker, Diff, Established, Gaps, Kind, Order, Parts, Showing, Truth, Wording,
};
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{
    Brief, Case, CaseResult, CaseStatus, Change, ChangeKind, CheckReport, Draft, Exercise, Form,
    KindId, MarkState, PartMark, PartRef, Piece, Spacing, Stage,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// The slots of a piece, or None when the piece is not an array of strings.
fn slots(piece: &Piece) -> Option<Vec<String>> {
    piece
        .value()
        .as_array()?
        .iter()
        .map(|slot| slot.as_str().map(str::to_owned))
        .collect()
}

fn slots_of(piece: &Piece) -> Result<Vec<String>> {
    slots(piece)
        .ok_or_else(|| AppError::InvalidState("The piece is not an array of strings.".into()))
}

fn index_of(part: &PartRef) -> Result<usize> {
    part.as_str()
        .parse()
        .map_err(|_| AppError::InvalidState(format!("{} is not a slot.", part.as_str())))
}

fn piece_of(slots: Vec<String>) -> Piece {
    Piece::new(json!(slots))
}

/// The roles of the fake kind, one struct for all of them.
pub(crate) struct Letters {
    reference: Vec<String>,
    ambiguous: Option<String>,
}

#[async_trait]
impl Checker for Letters {
    async fn check(
        &self,
        _brief: &Brief,
        piece: &Piece,
        cases: &[Case],
        _cancel: CancellationToken,
    ) -> Result<CheckReport> {
        let held = slots(piece);
        let results = cases
            .iter()
            .map(|case| {
                let index = case
                    .body
                    .get("index")
                    .and_then(Value::as_u64)
                    .and_then(|index| usize::try_from(index).ok());
                let expected = case.body.get("expected").and_then(Value::as_str);
                let status = match (&held, index, expected) {
                    (Some(held), Some(index), Some(expected)) => {
                        if held.get(index).map(String::as_str) == Some(expected) {
                            CaseStatus::Passed
                        } else {
                            CaseStatus::Failed
                        }
                    }
                    _ => CaseStatus::Error,
                };
                CaseResult {
                    case_id: case.id.clone(),
                    status,
                    detail: Value::Null,
                }
            })
            .collect();
        let marks = match &held {
            None => Vec::new(),
            Some(held) => self
                .reference
                .iter()
                .enumerate()
                .filter_map(|(index, wanted)| {
                    let got = held.get(index);
                    if got == Some(wanted) {
                        return None;
                    }
                    let state = if got.is_some_and(|got| got.is_empty()) {
                        MarkState::Empty
                    } else {
                        MarkState::Wrong
                    };
                    Some(PartMark {
                        part: PartRef::new(&index.to_string()),
                        state,
                    })
                })
                .collect(),
        };
        Ok(CheckReport { results, marks })
    }
}

impl Parts for Letters {
    fn all(&self, _brief: &Brief, piece: &Piece) -> Result<Vec<PartRef>> {
        Ok((0..slots_of(piece)?.len())
            .map(|index| PartRef::new(&index.to_string()))
            .collect())
    }

    fn holds(&self, _brief: &Brief, piece: &Piece, part: &PartRef) -> Result<bool> {
        Ok(index_of(part)? < slots_of(piece)?.len())
    }

    fn name(&self, _brief: &Brief, _piece: &Piece, part: &PartRef) -> Result<String> {
        Ok(format!("slot {}", index_of(part)?))
    }
}

impl Diff for Letters {
    fn changes(&self, _brief: &Brief, old: &Piece, new: &Piece) -> Result<Vec<Change>> {
        let (old, new) = (slots_of(old)?, slots_of(new)?);
        let mut changes = Vec::new();
        for index in 0..old.len().max(new.len()) {
            let (before, after) = (old.get(index), new.get(index));
            if before == after {
                continue;
            }
            let part = PartRef::new(&index.to_string());
            if let Some(text) = before {
                changes.push(Change {
                    part: part.clone(),
                    kind: ChangeKind::Removed,
                    text: text.clone(),
                });
            }
            if let Some(text) = after {
                changes.push(Change {
                    part,
                    kind: ChangeKind::Added,
                    text: text.clone(),
                });
            }
        }
        Ok(changes)
    }
}

impl Showing for Letters {
    fn brief(&self, brief: &Brief) -> Result<String> {
        Ok(brief.title.clone())
    }

    fn piece(&self, _brief: &Brief, piece: &Piece) -> Result<String> {
        Ok(slots_of(piece)?.join(", "))
    }

    fn result(&self, _brief: &Brief, case: &Case, result: &CaseResult) -> Result<String> {
        Ok(format!("{}: {:?}", case.title, result.status))
    }
}

impl Wording for Letters {
    fn card(&self) -> String {
        "Letters".to_owned()
    }

    fn stage_label(&self, _brief: &Brief, _form: Form, stage: Stage) -> String {
        format!("{stage:?}")
    }
}

impl Gaps for Letters {
    fn emptied(&self, _brief: &Brief, piece: &Piece, parts: &[PartRef]) -> Result<Piece> {
        let mut held = slots_of(piece)?;
        for part in parts {
            let slot = held
                .get_mut(index_of(part)?)
                .ok_or_else(|| AppError::InvalidState(format!("No slot {}.", part.as_str())))?;
            slot.clear();
        }
        Ok(piece_of(held))
    }

    fn restored(
        &self,
        _brief: &Brief,
        piece: &Piece,
        from: &Piece,
        part: &PartRef,
    ) -> Result<Piece> {
        let index = index_of(part)?;
        let mut held = slots_of(piece)?;
        let source = slots_of(from)?;
        let (slot, text) = held
            .get_mut(index)
            .zip(source.get(index))
            .ok_or_else(|| AppError::InvalidState(format!("No slot {}.", part.as_str())))?;
        *slot = text.clone();
        Ok(piece_of(held))
    }
}

impl Order for Letters {
    fn shuffled(&self, _brief: &Brief, piece: &Piece, seed: u64) -> Result<Piece> {
        let mut held = slots_of(piece)?;
        let len = u64::try_from(held.len()).unwrap_or(u64::MAX);
        if len > 1 {
            let by = usize::try_from(seed % len).unwrap_or(0).max(1);
            held.rotate_left(by);
        }
        Ok(piece_of(held))
    }
}

#[async_trait]
impl Truth for Letters {
    async fn establish(&self, draft: &Draft, cancel: CancellationToken) -> Result<Established> {
        let report = self
            .check(&draft.brief, &draft.reference, &draft.cases, cancel)
            .await?;
        if report.all_passed() {
            return Ok(Established::Right {
                cases: draft.cases.clone(),
                proof: Value::Null,
            });
        }
        let failing: Vec<&str> = report
            .results
            .iter()
            .filter(|result| result.status != CaseStatus::Passed)
            .map(|result| result.case_id.as_str())
            .collect();
        Ok(Established::Wrong(format!(
            "The reference fails the cases {}.",
            failing.join(", ")
        )))
    }

    async fn unambiguous(
        &self,
        _exercise: &Exercise,
        _cancel: CancellationToken,
    ) -> Result<Option<String>> {
        Ok(self.ambiguous.clone())
    }
}

/// Builds the fake kind with the roles and needs a test chooses.
pub(crate) struct FakeKindBuilder {
    reference: Vec<String>,
    gaps: bool,
    order: bool,
    needed: usize,
    ambiguous: Option<String>,
}

/// A fake kind whose reference piece is `reference`. It supports Build and Repair only, needs no misreadings and
/// finds every exercise unambiguous until a test says otherwise.
pub(crate) fn fake_kind(reference: &[&str]) -> FakeKindBuilder {
    FakeKindBuilder {
        reference: reference.iter().map(|slot| (*slot).to_owned()).collect(),
        gaps: false,
        order: false,
        needed: 0,
        ambiguous: None,
    }
}

impl FakeKindBuilder {
    pub(crate) fn with_gaps(mut self) -> Self {
        self.gaps = true;
        self
    }

    pub(crate) fn with_order(mut self) -> Self {
        self.order = true;
        self
    }

    pub(crate) fn needing(mut self, misreadings: usize) -> Self {
        self.needed = misreadings;
        self
    }

    pub(crate) fn ambiguous(mut self, reason: &str) -> Self {
        self.ambiguous = Some(reason.to_owned());
        self
    }

    pub(crate) fn build(self) -> Kind {
        let roles = Arc::new(Letters {
            reference: self.reference,
            ambiguous: self.ambiguous,
        });
        Kind {
            id: KindId::new("letters"),
            spacing: Spacing::Item,
            misreadings_needed: self.needed,
            wording: roles.clone(),
            checker: roles.clone(),
            parts: roles.clone(),
            diff: roles.clone(),
            showing: roles.clone(),
            truth: roles.clone(),
            gaps: self.gaps.then(|| roles.clone() as Arc<dyn Gaps>),
            order: self.order.then(|| roles as Arc<dyn Order>),
        }
    }
}
