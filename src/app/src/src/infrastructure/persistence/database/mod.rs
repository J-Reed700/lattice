pub mod connection;
pub mod init;
pub mod migrate;
pub mod performance_indexes;
pub mod schema;
// Vertical-slice migration (stats): database stats adapter lives in features/stats/.
#[path = "../../../features/stats/database_stats.rs"]
pub mod stats;
pub mod utils;

pub use connection::{query_with_quick_timeout, query_with_timeout, DatabaseConnection};
pub use init::initialize_database;
pub use migrate::run_migrations;
pub use performance_indexes::{benchmark_indexes, create_performance_indexes, get_index_stats};
pub use schema::rebuild_fts5_index;
pub use stats::DatabaseStatsAdapter;
pub use utils::DatabaseUtils;
pub mod schema_validation;

#[cfg(test)]
mod query_tests;
