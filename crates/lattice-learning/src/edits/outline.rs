use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::edits::{Change, Consequence, EditReactor, EditRegistry, Reaction};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::{Reading, WriteTx};

use super::keys;
use crate::planning::curriculum_repository::corrections::lessons_being_prepared;

pub(crate) struct OutlineReactor;

pub(crate) fn register(registry: &mut EditRegistry) -> Result<()> {
    registry.add_reactor(Arc::new(OutlineReactor))
}

async fn preparation_consequences(
    unit: &mut Reading<'_>,
    program_id: &str,
) -> Result<Vec<Consequence>> {
    let count = lessons_being_prepared(unit, program_id).await?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let message = if count == 1 {
        "1 lesson is being prepared from the outline as it is now. Saving stops it, and it will need to be started again.".to_owned()
    } else {
        format!("{count} lessons are being prepared from the outline as it is now. Saving stops them, and they will need to be started again.")
    };
    Ok(vec![Consequence {
        code: "outline.preparations_stop".into(),
        message,
        related: Vec::new(),
    }])
}

#[async_trait]
impl EditReactor for OutlineReactor {
    fn name(&self) -> &'static str {
        keys::OUTLINE_REACTOR
    }

    fn kinds(&self) -> &'static [&'static str] {
        &[keys::LESSON, keys::MODULE]
    }

    async fn preview(
        &self,
        unit: &mut Reading<'_>,
        change: &Change<'_>,
    ) -> Result<Vec<Consequence>> {
        preparation_consequences(unit, &change.scope.container).await
    }

    async fn apply(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<Reaction> {
        let consequences =
            preparation_consequences(&mut tx.reading(), &change.scope.container).await?;
        Ok(Reaction {
            receipt: None,
            affected: Vec::new(),
            consequences,
            follow_up: None,
        })
    }
}
