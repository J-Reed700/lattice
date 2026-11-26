//! Learning and study adapters for the shared content-edit pipeline.

pub mod card;
pub mod duplicates;
pub mod keys;
pub mod lesson;
pub mod module;
pub mod outline;
pub mod question;
pub mod quiz_bank;
pub mod section;

use std::sync::Arc;

use lattice_core::shared::edits::{ContentEdits, EditRegistry};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

pub const BEING_PREPARED: &str =
    "This lesson is still being prepared. You can edit it once it is ready.";

pub fn register(registry: &mut EditRegistry, database: &Database) -> Result<()> {
    section::register(registry)?;
    question::register(registry)?;
    lesson::register(registry)?;
    module::register(registry)?;
    card::register(registry)?;
    crate::lessons::claim_reviews::edit_reactor::register(registry, database)?;
    quiz_bank::register(registry)?;
    outline::register(registry)?;
    duplicates::register(registry)?;
    Ok(())
}

pub fn content_edits(database: &Database) -> Result<Arc<ContentEdits>> {
    let mut registry = EditRegistry::new();
    register(&mut registry, database)?;
    Ok(Arc::new(ContentEdits::new(
        database.clone(),
        Arc::new(registry),
        env!("CARGO_PKG_VERSION").to_owned(),
    )))
}

#[cfg(test)]
mod one_writer_tests;
