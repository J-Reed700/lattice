use lattice_core::shared::edits::EditRegistry;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

pub(crate) fn register(_registry: &mut EditRegistry, _database: &Database) -> Result<()> {
    Ok(())
}
