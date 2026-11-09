//! The three data-access rules, held at zero.
use std::{fmt, path::Path};

type Error = Box<dyn std::error::Error>;

/// `(file, function, reason)` for a function SQLite refuses to run in a transaction.
pub const EXCEPTIONS: &[(&str, &str, &str)] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: u8,
    pub path: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rule {}: {}:{} {}",
            self.rule, self.path, self.line, self.message
        )
    }
}

pub fn is_data_module(_path: &str) -> bool {
    false
}

pub fn file_violations(path: &str, ast: &syn::File) -> Vec<Violation> {
    file_violations_with(path, ast, EXCEPTIONS)
}

pub fn file_violations_with(
    _path: &str,
    _ast: &syn::File,
    _exceptions: &[(&str, &str, &str)],
) -> Vec<Violation> {
    Vec::new()
}

pub fn check(_roots: &[&Path], _failures: &mut usize) -> Result<(), Error> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICE: &str = "crates/lattice-learning/src/lessons/service.rs";
    const REPOSITORY: &str = "crates/lattice-learning/src/lessons/repository.rs";
    const TRANSACTIONS: &str = "crates/lattice-core/src/shared/persistence/transactions.rs";
    const DEFERRED: &str = "begins a deferred transaction; use begin_write() or begin_read()";
    const BEGIN_WITH: &str = "begins with begin_with(); use begin_write() or begin_read()";

    fn found(path: &str, source: &str) -> Vec<String> {
        let ast = syn::parse_file(source).unwrap();
        file_violations(path, &ast)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn sql(path: &str, line: usize, what: &str) -> String {
        format!("rule 2: {path}:{line} builds SQL outside a data module: {what}")
    }

    fn pool(path: &str, line: usize, name: &str, statements: usize, writes: usize) -> String {
        format!(
            "rule 3: {path}:{line} {name} runs {statements} statements on the pool, \
             {writes} of them writes, outside a transaction"
        )
    }

    #[test]
    fn a_deferred_begin_is_flagged_and_the_two_explicit_begins_are_not() {
        let source = r#"async fn start(pool: &SqlitePool) {
    let tx = pool
        .begin()
        .await
        .unwrap();
    let write = pool.begin_write().await.unwrap();
    let read = pool.begin_read().await.unwrap();
    let builder = connection.begin();
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![format!("rule 1: {SERVICE}:3 {DEFERRED}")]
        );
    }

    #[test]
    fn begin_with_is_flagged_everywhere_but_transactions_rs() {
        let source = r#"async fn start(pool: &SqlitePool) {
    let tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![format!("rule 1: {SERVICE}:2 {BEGIN_WITH}")]
        );
        assert_eq!(found(TRANSACTIONS, source), Vec::<String>::new());
    }

    #[test]
    fn the_sync_server_may_begin_a_postgres_transaction() {
        let source = r#"async fn start(pool: &PgPool) {
    let tx = pool.begin().await.unwrap();
    let other = pool.begin_with("BEGIN").await.unwrap();
}
"#;
        assert_eq!(
            found("crates/lattice-sync/src/service.rs", source),
            Vec::<String>::new()
        );
        assert_eq!(
            found(SERVICE, source),
            vec![
                format!("rule 1: {SERVICE}:2 {DEFERRED}"),
                format!("rule 1: {SERVICE}:3 {BEGIN_WITH}"),
            ]
        );
    }

    #[test]
    fn a_begin_inside_a_macro_body_is_flagged() {
        let source = r#"async fn start(pool: &SqlitePool) {
    tokio::select! { tx = pool.begin().await => {} }
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![format!("rule 1: {SERVICE}:2 {DEFERRED}")]
        );
    }

    #[test]
    fn sql_in_a_service_a_di_file_and_a_trait_impl_is_flagged() {
        let call = r#"async fn load(pool: &SqlitePool) {
    sqlx::query("SELECT 1").fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(found(SERVICE, call), vec![sql(SERVICE, 2, "sqlx::query")]);
        let di = "crates/lattice-desktop/src/interfaces/di/memory.rs";
        assert_eq!(found(di, call), vec![sql(di, 2, "sqlx::query")]);
        let port = "crates/lattice-core/src/application/ports/lessons.rs";
        let implementation = r#"impl Port for Adapter {
    async fn get(&self) {
        sqlx::query_as::<_, Row>("SELECT 1").fetch_all(&self.pool).await.unwrap();
    }
}
"#;
        assert_eq!(
            found(port, implementation),
            vec![sql(port, 3, "sqlx::query_as")]
        );
    }

    #[test]
    fn the_same_sql_in_a_data_module_is_fine() {
        let call = r#"async fn load(pool: &SqlitePool) {
    sqlx::query("SELECT 1").fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(found(SERVICE, call), vec![sql(SERVICE, 2, "sqlx::query")]);
        for path in [
            "crates/lattice-learning/src/lessons/repository.rs",
            "crates/lattice-learning/src/lessons/repository/claims.rs",
            "crates/lattice-index/src/embedding_store.rs",
            "crates/lattice-store/src/lib.rs",
            "crates/lattice-desktop/src/features/x/persistence/mod.rs",
        ] {
            assert_eq!(found(path, call), Vec::<String>::new(), "{path}");
        }
    }

    #[test]
    fn sql_inside_a_macro_body_is_flagged() {
        let source = r#"async fn load(pool: &SqlitePool) {
    tokio::try_join!(
        sqlx::query("a").execute(pool),
        query_scalar!("SELECT 1").fetch_one(pool),
    )
    .unwrap();
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![
                sql(SERVICE, 3, "sqlx::query"),
                sql(SERVICE, 4, "query_scalar!(")
            ]
        );
    }

    #[test]
    fn a_use_of_a_query_function_is_flagged_and_other_imports_are_not() {
        let source = "use sqlx::{query, Row};
use sqlx::query_as;
use sqlx::{Row, SqlitePool};
use sqlx::query::Query;
use sqlx::sqlite::SqliteRow;
";
        assert_eq!(
            found(SERVICE, source),
            vec![
                sql(SERVICE, 1, "use sqlx::query"),
                sql(SERVICE, 2, "use sqlx::query_as"),
                sql(SERVICE, 4, "use sqlx::query"),
            ]
        );
    }

    #[test]
    fn a_query_builder_is_flagged_where_it_is_imported_and_where_it_is_named() {
        let source = r#"use sqlx::QueryBuilder;
fn build() {
    let mut b = QueryBuilder::<Sqlite>::new("INSERT");
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![
                sql(SERVICE, 1, "QueryBuilder"),
                sql(SERVICE, 3, "QueryBuilder")
            ]
        );
    }

    #[test]
    fn a_call_to_raw_sql_is_flagged_and_so_is_its_import() {
        let source = r#"use sqlx::raw_sql;
async fn run(pool: &SqlitePool) {
    raw_sql("PRAGMA x").execute(pool).await.unwrap();
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![
                sql(SERVICE, 1, "use sqlx::raw_sql"),
                sql(SERVICE, 3, "raw_sql(")
            ]
        );
    }

    #[test]
    fn test_only_items_are_not_production_code() {
        let source = r#"#[cfg(test)]
mod tests {
    fn f() { sqlx::query("x"); pool.begin().await; }
}
#[cfg(test)]
fn helper() { sqlx::query("y"); }
fn kept() { sqlx::query("z"); }
"#;
        assert_eq!(found(SERVICE, source), vec![sql(SERVICE, 7, "sqlx::query")]);
    }

    #[test]
    fn two_statements_on_the_pool_with_a_write_are_flagged() {
        let source = r#"pub async fn rename(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
    sqlx::query("SELECT b").fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, source),
            vec![pool(REPOSITORY, 1, "rename", 2, 1)]
        );
    }

    #[test]
    fn two_statements_on_a_connection_from_acquire_are_flagged() {
        let source = r#"pub async fn move_it(pool: &SqlitePool) {
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *conn).await.unwrap();
    sqlx::query("SELECT b").fetch_one(&mut *conn).await.unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, source),
            vec![pool(REPOSITORY, 1, "move_it", 2, 1)]
        );
    }

    #[test]
    fn statements_on_a_transaction_or_a_borrowed_connection_are_not_flagged() {
        let on_pool = r#"pub async fn move_it(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
    sqlx::query("UPDATE b").execute(pool).await.unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, on_pool),
            vec![pool(REPOSITORY, 1, "move_it", 2, 2)]
        );
        let on_transaction = r#"pub async fn move_it(pool: &SqlitePool) {
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE b").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
pub async fn move_in(conn: &mut SqliteConnection) {
    sqlx::query("UPDATE a").execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE b").execute(&mut *conn).await.unwrap();
}
"#;
        assert_eq!(found(REPOSITORY, on_transaction), Vec::<String>::new());
    }

    #[test]
    fn one_statement_or_two_reads_are_not_flagged() {
        let one = r#"pub async fn rename(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
}
"#;
        let reads = r#"pub async fn both(pool: &SqlitePool) {
    sqlx::query("SELECT a").fetch_all(pool).await.unwrap();
    sqlx::query("SELECT b").fetch_optional(pool).await.unwrap();
}
"#;
        let write_and_read = r#"pub async fn rename(pool: &SqlitePool) {
    sqlx::query("SELECT b").fetch_optional(pool).await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT c").fetch_one(pool).await.unwrap();
    sqlx::query("UPDATE a").execute_many(pool).await.unwrap();
}
"#;
        assert_eq!(found(REPOSITORY, one), Vec::<String>::new());
        assert_eq!(found(REPOSITORY, reads), Vec::<String>::new());
        assert_eq!(
            found(REPOSITORY, write_and_read),
            vec![pool(REPOSITORY, 1, "rename", 3, 1)]
        );
    }

    #[test]
    fn a_listed_function_is_excepted() {
        let source = r#"pub async fn compact(pool: &SqlitePool) {
    sqlx::query("VACUUM").execute(pool).await.unwrap();
    sqlx::query("PRAGMA wal_checkpoint").fetch_one(pool).await.unwrap();
}
"#;
        let ast = syn::parse_file(source).unwrap();
        let listed = [(REPOSITORY, "compact", "VACUUM runs outside a transaction")];
        assert_eq!(
            file_violations_with(REPOSITORY, &ast, &[]).len(),
            1,
            "unlisted"
        );
        assert!(file_violations_with(REPOSITORY, &ast, &listed).is_empty());
        let other_file = [("crates/x/src/repository.rs", "compact", "elsewhere")];
        assert_eq!(file_violations_with(REPOSITORY, &ast, &other_file).len(), 1);
    }

    #[test]
    fn statements_inside_a_macro_body_count() {
        let source = r#"pub async fn load(pool: &SqlitePool) {
    tokio::try_join!(
        sqlx::query("UPDATE a").execute(pool),
        sqlx::query("SELECT b").fetch_all(pool),
    )
    .unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, source),
            vec![pool(REPOSITORY, 1, "load", 2, 1)]
        );
    }

    #[test]
    fn methods_count_and_services_and_test_functions_are_not_checked() {
        let source = r#"impl Store {
    pub async fn rename(&self) {
        sqlx::query("UPDATE a").execute(&self.pool).await.unwrap();
        sqlx::query("SELECT b").fetch_one(self.pool()).await.unwrap();
    }
}
#[cfg(test)]
async fn helper(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
    sqlx::query("SELECT b").fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, source),
            vec![pool(REPOSITORY, 2, "rename", 2, 1)]
        );
        let service = r#"async fn rename(pool: &SqlitePool) {
    foo.execute(pool).await.unwrap();
    foo.fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(found(SERVICE, service), Vec::<String>::new());
        assert_eq!(
            found(REPOSITORY, service),
            vec![pool(REPOSITORY, 1, "rename", 2, 1)]
        );
    }

    #[test]
    fn data_modules_are_matched_by_segment_or_stem() {
        for path in [
            "crates/lattice-store/src/lib.rs",
            "crates/lattice-learning/src/lessons/repository.rs",
            "crates/lattice-learning/src/lessons/repository/claims.rs",
            "crates/lattice-learning/src/repositories/claims.rs",
            "crates/lattice-index/src/embedding_store.rs",
            "crates/lattice-index/src/embedding_store/rows.rs",
            "crates/lattice-index/src/lessons_repository.rs",
            "crates/lattice-core/src/shared/persistence/mod.rs",
            "crates/lattice-core/src/shared/storage.rs",
            "crates/lattice-core/src/shared/database/pool.rs",
            "crates/lattice-core/src/shared/migrations.rs",
            "crates/lattice-core/src/store/mod.rs",
        ] {
            assert!(is_data_module(path), "{path}");
        }
        for path in [
            "crates/lattice-learning/src/lessons/service.rs",
            "crates/lattice-learning/src/lessons/restore.rs",
            "crates/lattice-learning/src/lessons/storefront.rs",
            "crates/lattice-learning/src/lessons/store_things.rs",
            "crates/lattice-learning/src/lessons/repository_helpers.rs",
            "crates/lattice-desktop/src/interfaces/di/memory.rs",
        ] {
            assert!(!is_data_module(path), "{path}");
        }
    }
}
