use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::edits::{Change, Consequence, EditReactor, EditRegistry, Reaction};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::{Reading, WriteTx};
use serde_json::Value;

use super::keys;
use crate::recall::recall_repository::{card_text, suggest_duplicates_in};

pub(crate) struct DuplicatesReactor;

pub(crate) fn register(registry: &mut EditRegistry) -> Result<()> {
    registry.add_reactor(Arc::new(DuplicatesReactor))
}

#[async_trait]
impl EditReactor for DuplicatesReactor {
    fn name(&self) -> &'static str {
        keys::DUPLICATES_REACTOR
    }

    fn kinds(&self) -> &'static [&'static str] {
        &[keys::CARD]
    }

    async fn preview(
        &self,
        _unit: &mut Reading<'_>,
        _change: &Change<'_>,
    ) -> Result<Vec<Consequence>> {
        Ok(Vec::new())
    }

    async fn apply(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<Reaction> {
        let Some(profile) = card_text::card_row(&mut tx.reading(), &change.content.key)
            .await?
            .and_then(|row| row.profile)
        else {
            return Ok(Reaction::default());
        };
        let before_question = change.before.get("question").and_then(Value::as_str);
        let after_question = change.after.get("question").and_then(Value::as_str);
        if before_question != after_question {
            if let Some(new_question) = after_question {
                suggest_duplicates_in(
                    tx,
                    &profile.program_id,
                    &change.content.key,
                    new_question,
                    change.at,
                )
                .await?;
            }
        }
        Ok(Reaction {
            receipt: None,
            affected: vec![keys::affected::RECALL.into()],
            consequences: Vec::new(),
            follow_up: None,
        })
    }
}
