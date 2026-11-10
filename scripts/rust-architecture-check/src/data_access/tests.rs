use super::*;

const SERVICE: &str = "crates/lattice-learning/src/lessons/service.rs";
const REPOSITORY: &str = "crates/lattice-learning/src/lessons/repository.rs";
const TRANSACTIONS: &str = "crates/lattice-core/src/shared/persistence/transactions.rs";
const DEFERRED: &str = "begins a deferred transaction; use begin_write() or begin_read()";
const BEGIN_WITH: &str = "begins with begin_with(); use begin_write() or begin_read()";

/// Rules 1 to 3; rules 4 and 5 are covered by `driver_tests` and `unit_of_work_tests`.
fn found(path: &str, source: &str) -> Vec<String> {
    let ast = syn::parse_file(source).unwrap();
    file_violations(path, &ast)
        .iter()
        .filter(|violation| violation.rule < 4)
        .map(ToString::to_string)
        .collect()
}

fn sql(path: &str, line: usize, what: &str) -> String {
    format!("rule 2: {path}:{line} builds SQL outside a data module: {what}")
}

#[test]
fn a_deferred_begin_is_flagged_and_the_two_explicit_begins_are_not() {
    let source = r#"async fn start(pool: &SqlitePool) {
    let tx = pool
        .begin()
        .await
        .unwrap();
    let read = pool.begin_read().await.unwrap();
    let write = pool.begin_write().await.unwrap();
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

fn on_pool(
    path: &str,
    line: usize,
    name: &str,
    pool: usize,
    total: usize,
    through: &str,
) -> String {
    let through = if through.is_empty() {
        String::new()
    } else {
        format!(" (through {through})")
    };
    format!(
        "rule 3: {path}:{line} {name} runs {pool} of its {total} statements on the pool{through}; \
         run them in one transaction"
    )
}

fn begins(path: &str, line: usize, name: &str, writes: usize, reads: usize) -> String {
    format!(
        "rule 3: {path}:{line} {name} begins {writes} write and {reads} read transactions; \
         begin at most one of each"
    )
}

fn reads_late(path: &str, line: usize, name: &str) -> String {
    format!(
        "rule 3: {path}:{line} {name} reads in a read transaction after beginning its write \
         transaction; do the reads first"
    )
}

fn after_commit(path: &str, line: usize, name: &str) -> String {
    format!(
        "rule 3: {path}:{line} {name} runs a statement after committing its write \
         transaction; run it in the transaction"
    )
}

fn awaits(path: &str, line: usize, name: &str, what: &str) -> String {
    format!(
        "rule 3: {path}:{line} {name} awaits {what} while its write transaction is open; \
         pass the transaction to it, or move the await before begin_write() or after commit()"
    )
}

#[test]
fn two_statements_on_the_pool_are_flagged() {
    let source = r#"pub async fn rename(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
    sqlx::query("SELECT b").fetch_one(pool).await.unwrap();
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![on_pool(REPOSITORY, 1, "rename", 2, 2, "")]
    );
}

#[test]
fn two_reads_on_the_pool_in_a_read_only_function_are_flagged() {
    let source = r#"pub async fn both(pool: &SqlitePool) {
    sqlx::query("SELECT a").fetch_all(pool).await.unwrap();
    sqlx::query("SELECT b").fetch_optional(pool).await.unwrap();
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![on_pool(REPOSITORY, 1, "both", 2, 2, "")]
    );
}

#[test]
fn a_single_statement_on_the_pool_is_not_flagged() {
    let source = r#"pub async fn rename(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
}
"#;
    assert_eq!(found(REPOSITORY, source), Vec::<String>::new());
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
        vec![on_pool(REPOSITORY, 1, "move_it", 2, 2, "")]
    );
}

#[test]
fn statements_on_a_transaction_or_a_borrowed_connection_are_not_flagged() {
    let source = r#"pub async fn move_it(pool: &SqlitePool) {
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
    assert_eq!(found(REPOSITORY, source), Vec::<String>::new());
}

#[test]
fn a_pre_check_on_the_pool_before_begin_write_is_flagged() {
    let source = r#"pub async fn rename(pool: &SqlitePool) {
    let row = sqlx::query("SELECT a").fetch_one(pool).await.unwrap();
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![on_pool(REPOSITORY, 1, "rename", 1, 2, "")]
    );
}

#[test]
fn a_read_on_the_pool_inside_the_write_transaction_is_flagged() {
    let own = r#"pub async fn rename(pool: &SqlitePool) {
    let mut tx = pool.begin_write().await.unwrap();
    let row = sqlx::query("SELECT a").fetch_one(pool).await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
"#;
    assert_eq!(
        found(REPOSITORY, own),
        vec![
            on_pool(REPOSITORY, 1, "rename", 1, 2, ""),
            awaits(REPOSITORY, 3, "rename", "fetch_one()"),
        ]
    );
    let helper = r#"impl Repo {
    pub async fn rename(&self) {
        let mut tx = self.pool.begin_write().await.unwrap();
        self.ensure_exists().await;
        sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
    async fn ensure_exists(&self) {
        sqlx::query("SELECT a").fetch_one(&self.pool).await.unwrap();
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, helper),
        vec![
            on_pool(REPOSITORY, 2, "rename", 1, 2, "ensure_exists"),
            awaits(REPOSITORY, 4, "rename", "ensure_exists()"),
        ]
    );
}

#[test]
fn a_read_on_the_pool_after_the_commit_is_flagged_twice() {
    let source = r#"impl Repo {
    pub async fn rename(&self) {
        let mut tx = self.pool.begin_write().await.unwrap();
        sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        self.load().await;
    }
    async fn load(&self) {
        sqlx::query("SELECT a").fetch_one(&self.pool).await.unwrap();
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            on_pool(REPOSITORY, 2, "rename", 1, 2, "load"),
            after_commit(REPOSITORY, 2, "rename"),
        ]
    );
}

#[test]
fn a_re_read_in_a_read_transaction_after_the_write_commit_is_flagged() {
    let source = r#"pub async fn rename(pool: &SqlitePool) {
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let mut read = pool.begin_read().await.unwrap();
    sqlx::query("SELECT a").fetch_one(&mut *read).await.unwrap();
    read.commit().await.unwrap();
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            reads_late(REPOSITORY, 1, "rename"),
            after_commit(REPOSITORY, 1, "rename"),
        ]
    );
}

#[test]
fn two_read_transactions_in_one_function_are_flagged() {
    let source = r#"pub async fn both(pool: &SqlitePool) {
    let mut first = pool.begin_read().await.unwrap();
    sqlx::query("SELECT a").fetch_one(&mut *first).await.unwrap();
    first.commit().await.unwrap();
    let mut second = pool.begin_read().await.unwrap();
    sqlx::query("SELECT b").fetch_one(&mut *second).await.unwrap();
    second.commit().await.unwrap();
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![begins(REPOSITORY, 1, "both", 0, 2)]
    );
}

#[test]
fn two_write_transactions_in_one_function_are_flagged() {
    let source = r#"pub async fn both(pool: &SqlitePool) {
    let mut first = pool.begin_write().await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *first).await.unwrap();
    first.commit().await.unwrap();
    let mut second = pool.begin_write().await.unwrap();
    sqlx::query("UPDATE b").execute(&mut *second).await.unwrap();
    second.commit().await.unwrap();
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![begins(REPOSITORY, 1, "both", 2, 0)]
    );
}

#[test]
fn a_read_transaction_that_ends_before_the_write_transaction_begins_is_not_flagged() {
    let source = r#"pub async fn rename(pool: &SqlitePool) {
    let mut read = pool.begin_read().await.unwrap();
    sqlx::query("SELECT a").fetch_one(&mut *read).await.unwrap();
    read.commit().await.unwrap();
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query("SELECT a").fetch_one(&mut *tx).await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
"#;
    assert_eq!(found(REPOSITORY, source), Vec::<String>::new());
}

#[test]
fn two_reads_in_one_read_transaction_are_not_flagged() {
    let source = r#"pub async fn both(pool: &SqlitePool) {
    let mut read = pool.begin_read().await.unwrap();
    sqlx::query("SELECT a").fetch_one(&mut *read).await.unwrap();
    sqlx::query("SELECT b").fetch_one(&mut *read).await.unwrap();
    read.commit().await.unwrap();
}
"#;
    assert_eq!(found(REPOSITORY, source), Vec::<String>::new());
}

#[test]
fn a_commit_that_returns_early_is_not_the_last_commit() {
    let source = r#"impl Repo {
    pub async fn rename(&self, id: &str) {
        let mut tx = self.pool.begin_write().await.unwrap();
        if seen(&mut *tx).await {
            tx.commit().await.unwrap();
            return;
        }
        sqlx::query("SELECT a").fetch_one(&mut *tx).await.unwrap();
        sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
}
"#;
    assert_eq!(found(REPOSITORY, source), Vec::<String>::new());
}

#[test]
fn a_read_on_the_pool_after_an_early_return_is_flagged() {
    let source = r#"impl Repo {
    pub async fn rename(&self, id: &str) {
        let mut tx = self.pool.begin_write().await.unwrap();
        if seen(&mut *tx).await {
            tx.commit().await.unwrap();
            return self.load(id).await;
        }
        sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
    async fn load(&self, id: &str) {
        sqlx::query("SELECT a").fetch_one(&self.pool).await.unwrap();
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![on_pool(REPOSITORY, 2, "rename", 1, 2, "load")]
    );
}

#[test]
fn a_statement_after_the_last_commit_is_flagged_even_with_an_early_return() {
    let source = r#"impl Repo {
    pub async fn rename(&self, id: &str) {
        let mut tx = self.pool.begin_write().await.unwrap();
        if seen(&mut *tx).await {
            tx.commit().await.unwrap();
            return;
        }
        sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let mut read = self.pool.begin_read().await.unwrap();
        sqlx::query("SELECT a").fetch_one(&mut *read).await.unwrap();
        read.commit().await.unwrap();
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            reads_late(REPOSITORY, 2, "rename"),
            after_commit(REPOSITORY, 2, "rename"),
        ]
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
    let listed = [(
        REPOSITORY,
        "compact",
        Exempt::Shape,
        "VACUUM runs outside a transaction",
    )];
    assert_eq!(
        file_violations_with(REPOSITORY, &ast, &[]).len(),
        1,
        "unlisted"
    );
    assert!(file_violations_with(REPOSITORY, &ast, &listed).is_empty());
    let other_file = [(
        "crates/x/src/repository.rs",
        "compact",
        Exempt::Shape,
        "elsewhere",
    )];
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
        vec![on_pool(REPOSITORY, 1, "load", 2, 2, "")]
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
        vec![on_pool(REPOSITORY, 2, "rename", 2, 2, "")]
    );
    let service = r#"async fn rename(pool: &SqlitePool) {
    foo.execute(pool).await.unwrap();
    foo.fetch_one(pool).await.unwrap();
}
"#;
    assert_eq!(found(SERVICE, service), Vec::<String>::new());
    assert_eq!(
        found(REPOSITORY, service),
        vec![on_pool(REPOSITORY, 1, "rename", 2, 2, "")]
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

#[test]
fn statements_made_through_same_file_helpers_are_counted() {
    let source = r#"impl Repo {
    pub async fn update(&self) {
        self.update_title().await;
        Self::update_prompt(self).await;
    }
    async fn update_title(&self) {
        sqlx::query("UPDATE a").execute(&self.pool).await.unwrap();
    }
    async fn update_prompt(&self) {
        sqlx::query("UPDATE b").execute(self.pool()).await.unwrap();
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![on_pool(
            REPOSITORY,
            2,
            "update",
            2,
            2,
            "update_title, update_prompt"
        )]
    );
}

#[test]
fn helper_statements_are_counted_through_recursion() {
    let source = r#"impl Repo {
    pub async fn update(&self) {
        self.save().await;
    }
    async fn save(&self) {
        self.write_a().await;
        self.write_b().await;
    }
    async fn write_a(&self) {
        sqlx::query("UPDATE a").execute(&self.pool).await.unwrap();
    }
    async fn write_b(&self) {
        sqlx::query("UPDATE b").execute(&self.pool).await.unwrap();
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            on_pool(REPOSITORY, 2, "update", 2, 2, "save, write_a, write_b"),
            on_pool(REPOSITORY, 5, "save", 2, 2, "write_a, write_b"),
        ]
    );
}

#[test]
fn helpers_given_the_transaction_are_not_flagged() {
    let source = r#"impl Repo {
    pub async fn update(&self) {
        let mut tx = self.pool.begin_write().await.unwrap();
        self.update_title(&mut *tx).await;
        self.update_prompt(&mut *tx).await;
        tx.commit().await.unwrap();
    }
    async fn update_title(&self, conn: &mut SqliteConnection) {
        sqlx::query("UPDATE a").execute(conn).await.unwrap();
    }
    async fn update_prompt(&self, pool: &mut SqliteConnection) {
        sqlx::query("UPDATE b").execute(pool).await.unwrap();
    }
}
"#;
    assert_eq!(found(REPOSITORY, source), Vec::<String>::new());
}

#[test]
fn a_free_helper_counts_when_its_executor_parameter_receives_the_pool() {
    let source = r#"pub async fn update(pool: &SqlitePool) {
    write_title(pool).await;
    write_prompt(pool).await;
}
async fn write_title(executor: &SqlitePool) {
    sqlx::query("UPDATE a").execute(executor).await.unwrap();
}
async fn write_prompt(executor: &SqlitePool) {
    sqlx::query("UPDATE b").execute(executor).await.unwrap();
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![on_pool(
            REPOSITORY,
            1,
            "update",
            2,
            2,
            "write_title, write_prompt"
        )]
    );
    let in_transaction = r#"pub async fn update(pool: &SqlitePool) {
    let mut tx = pool.begin_write().await.unwrap();
    write_title(&mut *tx).await;
    write_prompt(&mut *tx).await;
    tx.commit().await.unwrap();
}
async fn write_title(executor: &mut SqliteConnection) {
    sqlx::query("UPDATE a").execute(executor).await.unwrap();
}
async fn write_prompt(executor: &mut SqliteConnection) {
    sqlx::query("UPDATE b").execute(executor).await.unwrap();
}
"#;
    assert_eq!(found(REPOSITORY, in_transaction), Vec::<String>::new());
}

#[test]
fn helpers_that_call_each_other_terminate() {
    let source = r#"impl Repo {
    pub async fn first(&self) {
        self.second().await;
        sqlx::query("UPDATE a").execute(&self.pool).await.unwrap();
    }
    async fn second(&self) {
        self.first().await;
        sqlx::query("SELECT b").fetch_one(&self.pool).await.unwrap();
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            on_pool(REPOSITORY, 2, "first", 2, 2, "second"),
            on_pool(REPOSITORY, 6, "second", 2, 2, "first"),
        ]
    );
}

#[test]
fn helpers_that_each_begin_a_write_transaction_are_flagged() {
    let source = r#"impl Repo {
    pub async fn update(&self) {
        self.update_title().await;
        self.update_prompt().await;
    }
    async fn update_title(&self) {
        let mut tx = self.pool.begin_write().await.unwrap();
        sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
    async fn update_prompt(&self) {
        let mut tx = self.pool.begin_write().await.unwrap();
        sqlx::query("UPDATE b").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![begins(REPOSITORY, 2, "update", 2, 0)]
    );
}

const BEGIN_WRITE_HELPERS: &str = r#"impl DownloadRepository {
    async fn begin_write(&self) -> Result<WriteTx, DownloadError> {
        self.db_conn.pool().begin_write().await.map_err(DownloadError::from)
    }
    async fn commit(tx: WriteTx) -> Result<(), DownloadError> {
        tx.commit().await.map_err(DownloadError::from)
    }
"#;

#[test]
fn a_same_file_begin_write_helper_counts_as_one_begin_for_its_callers() {
    let source = format!(
        r#"{BEGIN_WRITE_HELPERS}    pub async fn delete(&self, id: &str) -> Result<(), DownloadError> {{
        let mut tx = self.begin_write().await?;
        sqlx::query("DELETE FROM downloads WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        Self::commit(tx).await
    }}
    pub async fn register(&self, rows: &[Row]) -> Result<(), DownloadError> {{
        let mut tx = self.begin_write().await?;
        for row in rows {{
            sqlx::query("INSERT INTO downloads VALUES (?)")
                .bind(&row.id)
                .execute(&mut *tx)
                .await?;
            for part in &row.parts {{
                sqlx::query("INSERT INTO parts VALUES (?)")
                    .bind(part)
                    .execute(&mut *tx)
                    .await?;
            }}
        }}
        Self::commit(tx).await
    }}
    pub async fn create(&self, row: Row) -> Result<(), DownloadError> {{
        self.register(&[row]).await
    }}
}}
"#
    );
    assert_eq!(found(REPOSITORY, &source), Vec::<String>::new());
}

#[test]
fn a_function_calling_the_begin_write_helper_twice_still_begins_two_units() {
    let source = format!(
        r#"{BEGIN_WRITE_HELPERS}    pub async fn swap(&self) -> Result<(), DownloadError> {{
        let mut first = self.begin_write().await?;
        sqlx::query("DELETE FROM a").execute(&mut *first).await?;
        Self::commit(first).await?;
        let mut second = self.begin_write().await?;
        sqlx::query("DELETE FROM b").execute(&mut *second).await?;
        Self::commit(second).await
    }}
}}
"#
    );
    assert_eq!(
        found(REPOSITORY, &source),
        vec![begins(REPOSITORY, 8, "swap", 2, 0)]
    );
}

#[test]
fn a_same_file_helper_named_begin_is_not_a_deferred_begin_but_a_pool_begin_is() {
    let source = r#"impl Repo {
    async fn begin(&self) -> Result<WriteTx, Error> {
        self.db_conn.pool().begin_write().await.map_err(Error::from)
    }
    async fn delete(&self, id: &str) -> Result<(), Error> {
        let mut tx = self.begin().await?;
        sqlx::query("DELETE FROM a WHERE id = ?").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn select(&self) {
        tokio::select! { tx = self.begin().await => {} }
    }
    async fn raw(&self) {
        let tx = self.pool.begin().await.unwrap();
    }
    async fn through_the_connection(&self) {
        let tx = self.db_conn.pool().begin().await.unwrap();
    }
    async fn on_a_parameter(pool: &SqlitePool) {
        let tx = pool.begin().await.unwrap();
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            format!("rule 1: {REPOSITORY}:15 {DEFERRED}"),
            format!("rule 1: {REPOSITORY}:18 {DEFERRED}"),
            format!("rule 1: {REPOSITORY}:21 {DEFERRED}"),
        ]
    );
}
