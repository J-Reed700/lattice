//! Rule 6: every statement runs in a unit of work. (a) A statement whose SQL writes runs on
//! a write unit: a `begin_write()` local or a `WriteTx` parameter. SQL the checker cannot read
//! is left to the read unit, which refuses to commit once it changed rows. (b) Only the
//! openers name a pool or its options, connect, or create a `Database`.
mod openers;
mod sql;
mod statements;
mod words;

use super::{Violation, POSTGRES};

const WRITES: &str = "a statement that writes runs outside a write unit; run it on begin_write() or a &mut WriteTx parameter";
const OPENS: &str = "opens a database outside the openers; take the Database you are given instead";

/// `(path prefix, reason)`: files whose statements run outside the units of work.
pub const STATEMENT_EXCEPTIONS: &[(&str, &str)] = &[(
    "crates/lattice-core/src/shared/persistence/",
    "the units themselves, and VACUUM, which SQLite refuses inside one",
)];

/// `(path prefix, reason)`: the places that create a pool or a `Database`.
pub const OPENERS: &[(&str, &str)] = &[
    (
        "crates/lattice-core/src/shared/persistence/",
        "the Database and its units",
    ),
    (
        "crates/lattice-desktop/src/infrastructure/persistence/database/",
        "the live database and the openers of other database files",
    ),
    (
        "crates/lattice-index/src/passage_index/store.rs",
        "the passage index's own database file",
    ),
];

fn listed(path: &str, list: &[(&str, &str)]) -> bool {
    list.iter().any(|(prefix, _)| path.starts_with(prefix))
}

pub fn violations(path: &str, ast: &syn::File) -> Vec<Violation> {
    let mut found = Vec::new();
    if path.starts_with(POSTGRES) {
        return found;
    }
    if !listed(path, STATEMENT_EXCEPTIONS) {
        found.extend(statements::violations(path, ast));
    }
    if !listed(path, OPENERS) {
        found.extend(openers::violations(path, ast));
    }
    found
}
