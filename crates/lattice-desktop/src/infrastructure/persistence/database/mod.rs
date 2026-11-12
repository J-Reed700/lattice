pub mod connection;
pub mod init;
pub mod migrate;
pub mod performance_indexes;
pub mod schema;

pub use connection::{open_to_change, open_to_read, DatabaseConnection};
pub use init::initialize_database;
pub use migrate::run_migrations;
pub use performance_indexes::get_index_stats;
pub use schema::rebuild_fts5_index;
pub mod schema_validation;

#[cfg(test)]
mod query_tests;

#[cfg(test)]
mod schema_tests;
