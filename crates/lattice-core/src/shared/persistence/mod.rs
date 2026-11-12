//! The database and its units of work, the error they report, and pure
//! formatting helpers shared by persistence implementations.

mod driver_error;

pub mod bind_limit;
pub mod database;
pub mod embedding_encoding;
pub mod error;
pub mod page;
pub mod sql_like;
pub mod timestamps;
pub mod transactions;
mod write_turns;
pub use database::Database;
pub use error::{PersistenceError, PersistenceErrorKind};
pub use page::Page;
pub use transactions::{ReadTx, WriteTx};
#[cfg(test)]
mod database_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod page_tests;
#[cfg(test)]
mod transactions_tests;
