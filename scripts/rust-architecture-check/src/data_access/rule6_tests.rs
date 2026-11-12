use super::*;

const SERVICE: &str = "crates/lattice-learning/src/lessons/service.rs";
const REPOSITORY: &str = "crates/lattice-learning/src/lessons/repository.rs";
const LIVE: &str = "crates/lattice-desktop/src/infrastructure/persistence/database/migrate.rs";
const SYNC: &str = "crates/lattice-sync/src/persistence/jobs.rs";
const WRITES: &str = "a statement that writes runs outside a write unit; run it on begin_write() or a &mut WriteTx parameter";

fn found(path: &str, source: &str) -> Vec<String> {
    let ast = syn::parse_file(source).unwrap();
    file_violations(path, &ast)
        .iter()
        .filter(|violation| violation.rule == 6)
        .map(ToString::to_string)
        .collect()
}

fn writes(path: &str, line: usize) -> String {
    format!("rule 6: {path}:{line} {WRITES}")
}

fn none() -> Vec<String> {
    Vec::new()
}

/// The statement is on line 3.
fn on_read_unit(sql: &str) -> String {
    format!(
        "pub async fn run(db: &Database) {{\n    let mut tx = db.begin_read().await?;\n    \
         sqlx::query(r#\"{sql}\"#).execute(&mut *tx).await?;\n}}\n"
    )
}

/// The statement is on line 2.
fn on_connection(sql: &str) -> String {
    format!(
        "pub async fn run(conn: &mut SqliteConnection) {{\n    \
         sqlx::query(r#\"{sql}\"#).execute(&mut *conn).await?;\n}}\n"
    )
}

/// The statement is on line 2.
fn on_pool(sql: &str) -> String {
    format!(
        "pub async fn run(pool: &Pool<Sqlite>) {{\n    \
         sqlx::query(r#\"{sql}\"#).execute(pool).await?;\n}}\n"
    )
}

fn on_write_unit(sql: &str) -> String {
    format!(
        "pub async fn run(db: &Database) {{\n    let mut tx = db.begin_write().await?;\n    \
         sqlx::query(r#\"{sql}\"#).execute(&mut *tx).await?;\n}}\n"
    )
}

const WRITING: [&str; 17] = [
    "INSERT INTO t (a) VALUES (1)",
    "UPDATE t SET a = 1",
    "DELETE FROM t",
    "CREATE TABLE t (a)",
    "DROP TABLE t",
    "ALTER TABLE t ADD b",
    "VACUUM",
    "ANALYZE",
    "REINDEX",
    "REPLACE INTO t VALUES (1)",
    "PRAGMA optimize",
    "PRAGMA wal_checkpoint(TRUNCATE)",
    "PRAGMA incremental_vacuum",
    "PRAGMA user_version = 3",
    "WITH t AS (SELECT 1) DELETE FROM x",
    "delete from t",
    "SELECT 1; DELETE FROM t",
];

const READING: [&str; 21] = [
    "SELECT a FROM t",
    "SELECT replace(a, 'x', 'y') FROM t",
    "SELECT REPLACE (a, 'x', 'y') FROM t",
    "PRAGMA table_info(t)",
    "PRAGMA foreign_keys",
    "SELECT '--DELETE'",
    "SELECT 'it''s a DELETE' FROM t",
    "SELECT 1 -- DELETE FROM t",
    "SELECT /* DROP TABLE t */ 1",
    "SELECT \"update\" FROM t",
    "SELECT [delete] FROM t",
    "SELECT `drop` FROM t",
    "SELECT * FROM t WHERE id IN (SELECT delete FROM u)",
    "SELECT created_at, updated_at FROM t",
    "SELECT updated FROM t",
    "SELECT 'INSERT INTO t'",
    "PRAGMA table_info(a = b)",
    "PRAGMA user_version; SELECT a = b",
    "SELECT 1 WHERE a == b",
    "SELECT a FROM t WHERE b >= 1 AND c <= 2 AND d != 3",
    "SELECT json_extract(a, '$[0]') FROM t",
];

#[test]
fn every_writing_statement_is_flagged_on_a_read_unit() {
    for sql in WRITING {
        assert_eq!(
            found(SERVICE, &on_read_unit(sql)),
            vec![writes(SERVICE, 3)],
            "{sql}"
        );
    }
}

#[test]
fn every_writing_statement_is_flagged_on_a_connection_parameter_and_on_a_pool() {
    for sql in WRITING {
        assert_eq!(
            found(REPOSITORY, &on_connection(sql)),
            vec![writes(REPOSITORY, 2)],
            "{sql}"
        );
        assert_eq!(
            found(REPOSITORY, &on_pool(sql)),
            vec![writes(REPOSITORY, 2)],
            "{sql}"
        );
    }
}

#[test]
fn no_writing_statement_is_flagged_on_a_write_unit() {
    for sql in WRITING {
        assert_eq!(found(SERVICE, &on_write_unit(sql)), none(), "{sql}");
    }
}

#[test]
fn a_write_unit_parameter_owned_or_handed_down_is_a_write_unit() {
    let source = r#"pub async fn handed(tx: &mut WriteTx) {
    sqlx::query("DELETE FROM t").execute(&mut **tx).await?;
}
pub async fn owned(mut tx: WriteTx) {
    sqlx::query("DELETE FROM t").execute(&mut *tx).await?;
}
pub async fn other_name(unit: &mut WriteTx) {
    sqlx::query("DELETE FROM t").execute(&mut **unit).await?;
}
pub async fn borrowed(tx: &mut WriteTx) {
    let conn = &mut **tx;
    sqlx::query("DELETE FROM t").execute(&mut *conn).await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn a_read_unit_parameter_is_not_a_write_unit() {
    let source = r#"pub async fn handed(tx: &mut ReadTx) {
    sqlx::query("DELETE FROM t").execute(&mut **tx).await?;
}
pub async fn owned(mut tx: ReadTx) {
    sqlx::query("DELETE FROM t").execute(&mut *tx).await?;
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![writes(REPOSITORY, 2), writes(REPOSITORY, 5)]
    );
}

#[test]
fn a_function_is_judged_by_its_own_units() {
    let source = r#"pub async fn writer(db: &Database) {
    let mut tx = db.begin_write().await?;
    sqlx::query("DELETE FROM t").execute(&mut *tx).await?;
}
pub async fn reader(db: &Database) {
    let mut tx = db.begin_read().await?;
    sqlx::query("DELETE FROM t").execute(&mut *tx).await?;
    async fn inner(tx: &mut ReadTx) {
        sqlx::query("DELETE FROM t").execute(&mut **tx).await?;
    }
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![writes(REPOSITORY, 7), writes(REPOSITORY, 9)]
    );
}

#[test]
fn reading_statements_are_not_flagged_anywhere() {
    for sql in READING {
        assert_eq!(found(SERVICE, &on_read_unit(sql)), none(), "{sql}");
        assert_eq!(found(REPOSITORY, &on_connection(sql)), none(), "{sql}");
        assert_eq!(found(REPOSITORY, &on_pool(sql)), none(), "{sql}");
    }
}

#[test]
fn a_writing_word_after_a_comment_on_an_earlier_line_is_found() {
    let source = "pub async fn run(conn: &mut SqliteConnection) {
    sqlx::query(\"-- header\nDELETE FROM t\").execute(&mut *conn).await?;
}
";
    assert_eq!(found(REPOSITORY, source), vec![writes(REPOSITORY, 3)]);
}

#[test]
fn the_line_is_the_one_of_the_statement_call() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection) {
    sqlx::query("DELETE FROM t")
        .bind(1)
        .execute(&mut *conn)
        .await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), vec![writes(REPOSITORY, 4)]);
}

#[test]
fn every_statement_call_is_checked() {
    for call in [
        "execute",
        "execute_many",
        "fetch",
        "fetch_one",
        "fetch_all",
        "fetch_optional",
        "fetch_many",
    ] {
        let source = format!(
            "pub async fn run(conn: &mut SqliteConnection) {{\n    \
             sqlx::query(\"DELETE FROM t\").{call}(&mut *conn).await?;\n}}\n"
        );
        assert_eq!(
            found(REPOSITORY, &source),
            vec![writes(REPOSITORY, 2)],
            "{call}"
        );
    }
}

#[test]
fn every_sql_function_starts_a_statement() {
    for function in [
        "query",
        "query_as::<_, Row>",
        "query_scalar",
        "query_with",
        "query_as_with::<_, Row, _>",
        "query_scalar_with",
        "raw_sql",
    ] {
        let source = format!(
            "pub async fn run(conn: &mut SqliteConnection) {{\n    \
             sqlx::{function}(\"DELETE FROM t\").execute(&mut *conn).await?;\n}}\n"
        );
        assert_eq!(
            found(REPOSITORY, &source),
            vec![writes(REPOSITORY, 2)],
            "{function}"
        );
    }
}

#[test]
fn a_function_imported_from_sqlx_starts_a_statement_too() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection) {
    query("DELETE FROM t").execute(&mut *conn).await?;
    query_as::<_, Row>("DELETE FROM t RETURNING a").fetch_all(&mut *conn).await?;
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![writes(REPOSITORY, 2), writes(REPOSITORY, 3)]
    );
}

#[test]
fn a_call_that_is_not_a_statement_is_not_flagged() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection, client: Client) {
    client.execute("DELETE FROM t").await?;
    other("DELETE FROM t").fetch_all(&mut *conn).await?;
    let sql = "DELETE FROM t";
    helper(sql).execute(&mut *conn).await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn a_constant_sql_is_read() {
    let source = r#"const DELETE_ALL: &str = "DELETE FROM t";
const SELECT_ALL: &str = "SELECT 1";
static JOINED: &str = concat!("DELE", "TE FROM t");
pub async fn run(db: &Database) {
    let mut tx = db.begin_read().await?;
    sqlx::query(DELETE_ALL).execute(&mut *tx).await?;
    sqlx::query(SELECT_ALL).execute(&mut *tx).await?;
    sqlx::query(JOINED).execute(&mut *tx).await?;
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![writes(REPOSITORY, 6), writes(REPOSITORY, 8)]
    );
}

#[test]
fn an_associated_constant_sql_is_read() {
    let source = r#"impl Repo {
    const DELETE_ALL: &'static str = "DELETE FROM t";
    async fn run(&self, tx: &mut ReadTx) {
        sqlx::query(Self::DELETE_ALL).execute(&mut **tx).await?;
    }
}
"#;
    assert_eq!(found(REPOSITORY, source), vec![writes(REPOSITORY, 4)]);
}

#[test]
fn a_format_string_is_read_with_its_holes_replaced() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection, table: &str) {
    sqlx::query(&format!("DELETE FROM {}", table)).execute(&mut *conn).await?;
    sqlx::query(&format!("DELETE FROM {table}")).execute(&mut *conn).await?;
    sqlx::query(format!("SELECT * FROM {table} WHERE a = {{x}}").as_str()).execute(&mut *conn).await?;
    sqlx::query(&format!("SELECT * FROM {} WHERE a = ?", table)).execute(&mut *conn).await?;
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![writes(REPOSITORY, 2), writes(REPOSITORY, 3)]
    );
}

#[test]
fn a_local_built_from_literals_is_read() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection, name: &str) {
    let mut sql = String::from("SELECT 1");
    sql.push_str(" ; DELETE FROM t");
    sqlx::query(&sql).execute(&mut *conn).await?;
    let plain = "DELETE FROM t".to_string();
    sqlx::query(&plain).execute(&mut *conn).await?;
    let added = String::from("DELETE") + " FROM t";
    sqlx::query(&added).execute(&mut *conn).await?;
    let formatted = format!("DELETE FROM {name}");
    sqlx::query(&formatted).execute(&mut *conn).await?;
    let mut select = String::new();
    select.push_str("SELECT a");
    select += " FROM t";
    sqlx::query(&select).execute(&mut *conn).await?;
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            writes(REPOSITORY, 4),
            writes(REPOSITORY, 6),
            writes(REPOSITORY, 8),
            writes(REPOSITORY, 10),
        ]
    );
}

#[test]
fn a_literal_that_reaches_a_local_in_a_branch_counts() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection, clear: bool) {
    let sql = if clear { "DELETE FROM t" } else { "SELECT 1" };
    sqlx::query(sql).execute(&mut *conn).await?;
    let order = if clear { " DESC" } else { " ASC" };
    let mut ordered = String::from("SELECT a FROM t ORDER BY a");
    ordered.push_str(order);
    sqlx::query(&ordered).execute(&mut *conn).await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), vec![writes(REPOSITORY, 3)]);
}

#[test]
fn a_write_macro_into_a_local_is_read() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection) {
    let mut sql = String::new();
    write!(sql, "DELETE FROM t").unwrap();
    sqlx::query(&sql).execute(&mut *conn).await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), vec![writes(REPOSITORY, 4)]);
}

#[test]
fn a_query_builder_is_read_from_its_literals() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection, ids: &[i64]) {
    let mut builder = QueryBuilder::new("DELETE FROM t WHERE id IN (");
    builder.push_bind(1);
    builder.push(")");
    builder.build().execute(&mut *conn).await?;
    let mut select = QueryBuilder::<Sqlite>::new("SELECT id FROM t WHERE id IN (");
    select.push_bind(1);
    select.push(") ; DROP TABLE t");
    select.build_query_scalar::<i64>().fetch_all(&mut *conn).await?;
    let mut plain = QueryBuilder::new("SELECT id FROM t WHERE id IN (");
    plain.push_bind(1);
    plain.push(")");
    plain.build_query_scalar::<i64>().fetch_all(&mut *conn).await?;
    QueryBuilder::new("SELECT 1").push(" DELETE FROM t").build().execute(&mut *conn).await?;
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            writes(REPOSITORY, 5),
            writes(REPOSITORY, 9),
            writes(REPOSITORY, 14),
        ]
    );
}

#[test]
fn a_query_builder_on_a_write_unit_is_not_flagged() {
    let source = r#"pub async fn run(tx: &mut WriteTx) {
    let mut builder = QueryBuilder::new("INSERT INTO t (a) ");
    builder.push_values([1, 2], |mut row, value| {
        row.push_bind(value);
    });
    builder.build().execute(&mut **tx).await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn a_query_with_bindings_kept_in_a_local_is_read() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection) {
    let query = sqlx::query("DELETE FROM t");
    query.bind(1).execute(&mut *conn).await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), vec![writes(REPOSITORY, 3)]);
}

#[test]
fn the_sqlx_macros_are_read() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection, id: i64) {
    sqlx::query!("DELETE FROM t WHERE id = ?", id).execute(&mut *conn).await?;
    sqlx::query!("SELECT 1 AS one").fetch_one(&mut *conn).await?;
    sqlx::query_as!(Row, "DELETE FROM t RETURNING a").fetch_all(&mut *conn).await?;
    sqlx::query_as!(Row, "SELECT a FROM t").fetch_all(&mut *conn).await?;
    sqlx::query_scalar!("DELETE FROM t RETURNING a").fetch_all(&mut *conn).await?;
    sqlx::query_scalar!("SELECT a FROM t").fetch_all(&mut *conn).await?;
}
"#;
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            writes(REPOSITORY, 2),
            writes(REPOSITORY, 4),
            writes(REPOSITORY, 6),
        ]
    );
}

#[test]
fn a_statement_inside_a_macro_is_checked() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection) {
    let n = tokio::join!(sqlx::query("DELETE FROM t").execute(&mut *conn));
}
"#;
    assert_eq!(found(REPOSITORY, source), vec![writes(REPOSITORY, 2)]);
}

#[test]
fn sql_the_checker_cannot_read_is_not_flagged() {
    let source = r#"pub async fn parameter(conn: &mut SqliteConnection, sql: &str) {
    sqlx::query(sql).execute(&mut *conn).await?;
}
pub async fn call(conn: &mut SqliteConnection) {
    sqlx::query(&build()).execute(&mut *conn).await?;
}
pub async fn pushed(conn: &mut SqliteConnection, extra: &str) {
    let mut sql = String::from("SELECT 1");
    sql.push_str(extra);
    sqlx::query(&sql).execute(&mut *conn).await?;
}
pub async fn shadowed(conn: &mut SqliteConnection, sql: &str) {
    let sql = sql.to_string();
    sqlx::query(&sql).execute(&mut *conn).await?;
}
pub async fn on_a_pool(pool: &Pool<Sqlite>, sql: &str) {
    sqlx::query(sql).execute(pool).await?;
}
pub async fn on_a_write_unit(tx: &mut WriteTx, sql: &str) {
    sqlx::query(sql).execute(&mut **tx).await?;
}
pub async fn on_a_begun_unit(db: &Database, sql: &str) {
    let mut tx = db.begin_write().await?;
    sqlx::query(sql).execute(&mut *tx).await?;
}
pub async fn on_a_read_unit(db: &Database, sql: &str) {
    let mut tx = db.begin_read().await?;
    sqlx::query(sql).fetch_all(&mut *tx).await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn a_file_query_is_not_flagged() {
    let source = r#"pub async fn run(conn: &mut SqliteConnection) {
    sqlx::query_file!("queries/delete.sql").execute(&mut *conn).await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn the_live_database_directory_and_the_sync_server_are_not_scanned() {
    let source = on_connection("DELETE FROM t");
    assert_eq!(found(LIVE, &source), none());
    assert_eq!(found(SYNC, &source), none());
    assert_eq!(found(REPOSITORY, &source), vec![writes(REPOSITORY, 2)]);
}

#[test]
fn test_only_code_is_not_scanned() {
    let source = r#"#[cfg(test)]
mod tests {
    async fn run(conn: &mut SqliteConnection) {
        sqlx::query("DELETE FROM t").execute(&mut *conn).await?;
    }
}
#[cfg(test)]
async fn helper(conn: &mut SqliteConnection) {
    sqlx::query("DELETE FROM t").execute(&mut *conn).await?;
}
"#;
    assert_eq!(found(REPOSITORY, source), none());
    let whole = format!("#![cfg(test)]\n{}", on_connection("DELETE FROM t"));
    assert_eq!(found(REPOSITORY, &whole), none());
}
