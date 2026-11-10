//! Pure formatting helpers shared by persistence implementations, and the
//! database driver's error conversion.

mod driver_error;

pub mod embedding_encoding;
pub mod sql_like;
pub mod timestamps;
pub mod transactions;
pub use transactions::{ReadTx, Transactions, WriteTx};
#[cfg(test)]
mod transactions_tests;
