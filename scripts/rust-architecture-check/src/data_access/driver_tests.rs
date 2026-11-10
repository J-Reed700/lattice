use super::*;

const SERVICE: &str = "crates/lattice-learning/src/lessons/service.rs";
const REPOSITORY: &str = "crates/lattice-learning/src/lessons/repository.rs";
const FEATURE_REPOSITORY: &str = "crates/lattice-learning/src/features/x/repository.rs";
const LEARNING_ROOT_REPOSITORY: &str = "crates/lattice-learning/src/repository.rs";
const DOCUMENT: &str =
    "crates/lattice-desktop/src/infrastructure/persistence/repositories/document/implementation.rs";
const INFRASTRUCTURE: &str = "crates/lattice-desktop/src/infrastructure/mod.rs";
const PERSISTENCE: &str = "crates/lattice-desktop/src/infrastructure/persistence/mod.rs";
const CONNECTION: &str = "crates/lattice-desktop/src/infrastructure/persistence/connection.rs";
const DATABASE: &str = "crates/lattice-core/src/shared/persistence/database.rs";
const SYNC_DB: &str = "crates/lattice-sync/src/persistence/db.rs";

fn found(path: &str, source: &str) -> Vec<String> {
    let ast = syn::parse_file(source).unwrap();
    file_violations(path, &ast)
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn found_among(files: &[(&str, &str)], target: &str) -> Vec<String> {
    let parsed: Vec<(Vec<String>, syn::File)> = files
        .iter()
        .map(|(path, source)| (handouts::module_of(path), syn::parse_file(source).unwrap()))
        .collect();
    let modules =
        handouts::module_scopes(parsed.iter().map(|(module, ast)| (module.as_slice(), ast)));
    let index = files.iter().position(|(path, _)| *path == target).unwrap();
    let (module, ast) = &parsed[index];
    file_violations_in(target, module, ast, &modules)
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn names(path: &str, line: usize, what: &str) -> String {
    format!(
        "rule 4: {path}:{line} names {what} outside a data module; hold a Database, \
         take WriteTx or ReadTx, or move the code into a data module"
    )
}

fn hands(path: &str, line: usize, handle: &str, kind: &str, name: &str) -> String {
    format!(
        "rule 5: {path}:{line} hands out {handle} from {kind} {name}; return a Database \
         or keep it private to data modules"
    )
}

fn unwraps(path: &str, line: usize) -> String {
    format!(
        "rule 5: {path}:{line} calls sqlx_pool() outside a data module; pass the Database \
         to a data-module function"
    )
}

fn none() -> Vec<String> {
    Vec::new()
}

#[test]
fn a_use_of_a_sqlx_type_outside_a_data_module_is_flagged() {
    assert_eq!(
        found(SERVICE, "use sqlx::SqlitePool;\n"),
        vec![names(SERVICE, 1, "sqlx::SqlitePool")]
    );
}

#[test]
fn every_use_form_that_names_sqlx_is_flagged() {
    let source = "use sqlx;
use sqlx as s;
use sqlx::{self, Row};
use sqlx::sqlite::*;
use std::sync::Arc;
";
    assert_eq!(
        found(SERVICE, source),
        vec![
            names(SERVICE, 1, "sqlx"),
            names(SERVICE, 2, "sqlx"),
            names(SERVICE, 3, "sqlx, sqlx::Row"),
            names(SERVICE, 4, "sqlx::sqlite::*"),
        ]
    );
}

#[test]
fn a_sqlx_error_in_a_signature_is_flagged() {
    assert_eq!(
        found(SERVICE, "fn f(e: sqlx::Error) {}\n"),
        vec![names(SERVICE, 1, "sqlx::Error")]
    );
}

#[test]
fn a_derive_naming_sqlx_is_flagged() {
    let source = "#[derive(Debug, sqlx::FromRow)]
struct R {
    a: i64,
}
";
    assert_eq!(
        found(SERVICE, source),
        vec![names(SERVICE, 1, "sqlx::FromRow")]
    );
}

#[test]
fn a_sqlx_attribute_is_flagged() {
    let source = r#"struct R {
    #[sqlx(rename = "x")]
    a: i64,
}
"#;
    assert_eq!(found(SERVICE, source), vec![names(SERVICE, 2, "sqlx")]);
}

#[test]
fn extern_crate_and_a_leading_colon_use_are_flagged() {
    let source = "extern crate sqlx;
use ::sqlx::Row;
";
    assert_eq!(
        found(SERVICE, source),
        vec![names(SERVICE, 1, "sqlx"), names(SERVICE, 2, "sqlx::Row")]
    );
}

#[test]
fn a_sqlx_path_inside_a_macro_body_is_flagged() {
    let source = "fn f() {
    let v = vec![sqlx::Error::RowNotFound];
}
";
    assert_eq!(
        found(SERVICE, source),
        vec![names(SERVICE, 2, "sqlx::Error::RowNotFound")]
    );
}

#[test]
fn a_sqlx_macro_is_flagged_as_sql_and_as_sqlx() {
    let source = "fn f() {
    sqlx::query!(\"select 1\");
}
";
    assert_eq!(
        found(SERVICE, source),
        vec![
            format!("rule 2: {SERVICE}:2 builds SQL outside a data module: sqlx::query"),
            names(SERVICE, 2, "sqlx::query"),
        ]
    );
}

#[test]
fn a_data_module_may_name_sqlx_everywhere() {
    let source = r#"extern crate sqlx;
use ::sqlx::Row;
use sqlx::SqlitePool;
#[derive(Debug, sqlx::FromRow)]
struct R {
    #[sqlx(rename = "x")]
    a: i64,
}
fn f(e: sqlx::Error) {
    let v = vec![sqlx::Error::RowNotFound];
}
"#;
    assert_eq!(found(REPOSITORY, source), none());
    assert_eq!(
        found(SERVICE, source),
        vec![
            names(SERVICE, 1, "sqlx"),
            names(SERVICE, 2, "sqlx::Row"),
            names(SERVICE, 3, "sqlx::SqlitePool"),
            names(SERVICE, 4, "sqlx::FromRow"),
            names(SERVICE, 6, "sqlx"),
            names(SERVICE, 9, "sqlx::Error"),
            names(SERVICE, 10, "sqlx::Error::RowNotFound"),
        ]
    );
}

#[test]
fn the_sync_server_may_not_name_sqlx_outside_its_data_modules() {
    let source = "pub struct State {
    pub pool: sqlx::PgPool,
}
";
    let path = "crates/lattice-sync/src/http/state.rs";
    assert_eq!(found(path, source), vec![names(path, 2, "sqlx::PgPool")]);
}

#[test]
fn test_only_items_and_docs_and_strings_do_not_count() {
    let source = r#"#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;
}
/// uses sqlx
fn f() {
    let s = "sqlx::Error";
}
"#;
    assert_eq!(found(SERVICE, source), none());
}

#[test]
fn a_public_method_returning_the_pool_is_a_handout_and_a_private_one_is_not() {
    let source = "use sqlx::SqlitePool;
pub struct R {
    pool: SqlitePool,
}
impl R {
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
";
    assert_eq!(
        found(FEATURE_REPOSITORY, source),
        vec![hands(FEATURE_REPOSITORY, 6, "SqlitePool", "fn", "pool")]
    );
    let private = source.replace("pub fn pool", "fn pool");
    assert_eq!(found(FEATURE_REPOSITORY, &private), none());
}

#[test]
fn a_pub_super_field_is_visible_from_the_crate_root() {
    let source = "use sqlx::SqlitePool;
pub struct R {
    pub(super) pool: SqlitePool,
}
";
    assert_eq!(
        found(LEARNING_ROOT_REPOSITORY, source),
        vec![hands(
            LEARNING_ROOT_REPOSITORY,
            3,
            "SqlitePool",
            "field",
            "R.pool"
        )]
    );
}

#[test]
fn a_pub_super_field_is_hidden_when_super_is_a_data_module() {
    let source = "use sqlx::SqlitePool;
pub struct R {
    pub(super) pool: SqlitePool,
}
";
    assert_eq!(found(DOCUMENT, source), none());
}

#[test]
fn pub_in_resolves_from_the_crate_and_stops_at_the_deepest_scope() {
    let source = "use sqlx::SqlitePool;
pub struct R {
    pub(in crate::lessons) a: SqlitePool,
    pub(in crate::lessons::repository) b: SqlitePool,
    pub(crate) c: SqlitePool,
    pub(self) d: SqlitePool,
}
";
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            hands(REPOSITORY, 3, "SqlitePool", "field", "R.a"),
            hands(REPOSITORY, 5, "SqlitePool", "field", "R.c"),
        ]
    );
}

#[test]
fn a_private_module_hides_a_public_item_from_outside_data_modules() {
    let connection = "use sqlx::SqlitePool;
pub fn pool() -> SqlitePool {
    todo!()
}
";
    let private = [
        (INFRASTRUCTURE, "pub mod persistence;"),
        (PERSISTENCE, "mod connection;"),
        (CONNECTION, connection),
    ];
    assert_eq!(found_among(&private, CONNECTION), none());
    let public = [
        (INFRASTRUCTURE, "pub mod persistence;"),
        (PERSISTENCE, "pub mod connection;"),
        (CONNECTION, connection),
    ];
    assert_eq!(
        found_among(&public, CONNECTION),
        vec![hands(CONNECTION, 2, "SqlitePool", "fn", "pool")]
    );
    let outer = [
        (INFRASTRUCTURE, "mod persistence;"),
        (PERSISTENCE, "pub mod connection;"),
        (CONNECTION, connection),
    ];
    assert_eq!(
        found_among(&outer, CONNECTION),
        vec![hands(CONNECTION, 2, "SqlitePool", "fn", "pool")]
    );
}

#[test]
fn inline_modules_narrow_a_public_item_the_same_way() {
    let source = "use sqlx::SqlitePool;
mod inner {
    use sqlx::SqlitePool;
    pub fn hidden() -> SqlitePool { todo!() }
}
pub mod open {
    use sqlx::SqlitePool;
    pub fn shown() -> SqlitePool { todo!() }
}
";
    assert_eq!(
        found(REPOSITORY, source),
        vec![hands(REPOSITORY, 8, "SqlitePool", "fn", "shown")]
    );
}

#[test]
fn a_public_re_export_of_sqlx_is_a_handout_and_a_private_use_is_not() {
    assert_eq!(
        found(REPOSITORY, "pub use sqlx::Error as DbError;\n"),
        vec![hands(REPOSITORY, 1, "sqlx::Error", "re-export", "DbError")]
    );
    assert_eq!(found(REPOSITORY, "use sqlx::Error;\n"), none());
}

#[test]
fn aliases_fields_variants_and_statics_of_a_handle_are_handouts() {
    let source = "use sqlx::SqlitePool;
pub type DbPool = sqlx::SqlitePool;
pub struct Db(pub SqlitePool);
pub enum Conn {
    Pool(SqlitePool),
}
pub static P: std::sync::OnceLock<SqlitePool> = std::sync::OnceLock::new();
";
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            hands(REPOSITORY, 2, "sqlx::SqlitePool", "alias", "DbPool"),
            hands(REPOSITORY, 3, "SqlitePool", "field", "Db.0"),
            hands(REPOSITORY, 5, "SqlitePool", "field", "Conn::Pool.0"),
            hands(REPOSITORY, 7, "SqlitePool", "static", "P"),
        ]
    );
}

#[test]
fn a_trait_method_declaration_is_checked_and_a_trait_impl_is_not() {
    let declaration = "use sqlx::SqlitePool;
pub trait T {
    fn pool(&self) -> &SqlitePool;
}
";
    assert_eq!(
        found(REPOSITORY, declaration),
        vec![hands(REPOSITORY, 3, "SqlitePool", "fn", "pool")]
    );
    let implementation = "use sqlx::SqlitePool;
struct X {
    pool: SqlitePool,
}
impl T for X {
    fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
";
    assert_eq!(found(REPOSITORY, implementation), none());
}

#[test]
fn a_handle_is_resolved_through_the_files_sqlx_imports() {
    let local = "struct Pool;
pub fn p() -> Pool {
    Pool
}
";
    assert_eq!(found(REPOSITORY, local), none());
    let generic = "use sqlx::{Pool, Sqlite};
pub fn p() -> Pool<Sqlite> {
    todo!()
}
";
    assert_eq!(
        found(REPOSITORY, generic),
        vec![hands(REPOSITORY, 2, "Pool", "fn", "p")]
    );
    let module = "use sqlx::sqlite;
pub fn o() -> sqlite::SqliteConnectOptions {
    todo!()
}
";
    assert_eq!(
        found(REPOSITORY, module),
        vec![hands(
            REPOSITORY,
            2,
            "sqlite::SqliteConnectOptions",
            "fn",
            "o"
        )]
    );
    let alias = "use sqlx::SqlitePool as Db;
pub fn o() -> Db {
    todo!()
}
";
    assert_eq!(
        found(REPOSITORY, alias),
        vec![hands(REPOSITORY, 2, "Db", "fn", "o")]
    );
    let renamed_module = "use sqlx::sqlite as s;
pub fn o() -> s::SqliteConnectOptions {
    todo!()
}
";
    assert_eq!(
        found(REPOSITORY, renamed_module),
        vec![hands(REPOSITORY, 2, "s::SqliteConnectOptions", "fn", "o")]
    );
    let nested = "use sqlx::{sqlite::SqliteConnectOptions as Opts};
pub fn o() -> Opts {
    todo!()
}
";
    assert_eq!(
        found(REPOSITORY, nested),
        vec![hands(REPOSITORY, 2, "Opts", "fn", "o")]
    );
    let foreign = "use sqlx::prelude::*;
use crate::shared::persistence::Database as Pool;
pub fn o() -> Pool {
    todo!()
}
";
    assert_eq!(found(REPOSITORY, foreign), none());
    let not_a_handle = "use sqlx::Error as Pool;
pub fn o() -> Pool {
    todo!()
}
";
    assert_eq!(found(REPOSITORY, not_a_handle), none());
}

#[test]
fn a_handle_inside_a_result_option_or_impl_bound_is_found() {
    let source = "use sqlx::{SqlitePool, Executor};
pub fn a() -> Result<Option<(i64, std::sync::Arc<SqlitePool>)>, String> {
    todo!()
}
pub fn b() -> impl Executor<'static> {
    todo!()
}
";
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            hands(REPOSITORY, 2, "SqlitePool", "fn", "a"),
            hands(REPOSITORY, 5, "Executor", "fn", "b"),
        ]
    );
}

#[test]
fn a_sqlx_error_is_not_a_handle() {
    let source = "impl R {
    pub async fn row(&self) -> Result<i64, sqlx::Error> {
        Ok(1)
    }
}
";
    assert_eq!(found(REPOSITORY, source), none());
}

const DATABASE_SOURCE: &str = r#"//! The database as code outside a data module holds it.
use async_trait::async_trait;
use sqlx::SqlitePool;

use super::transactions::{ReadTx, Transactions, WriteTx};

/// The application's database: it begins units of work and closes; only a
/// data module reaches the driver's pool inside it.
#[derive(Clone, Debug)]
pub struct Database(SqlitePool);

impl Database {
    /// Whether a connection can be had.
    pub async fn is_reachable(&self) -> bool {
        self.0.acquire().await.is_ok()
    }

    pub async fn close(&self) {
        self.0.close().await;
    }
}

impl From<SqlitePool> for Database {
    fn from(pool: SqlitePool) -> Self {
        Self(pool)
    }
}

/// What a data module's function takes: the database, or the driver's pool itself.
pub trait DatabaseHandle: Transactions + Send + Sync + std::fmt::Debug {
    /// The driver's pool. Called only in data modules.
    fn sqlx_pool(&self) -> &SqlitePool;

    /// The database, for code outside data modules to hold.
    fn database(&self) -> Database;
}

impl DatabaseHandle for Database {
    fn sqlx_pool(&self) -> &SqlitePool {
        &self.0
    }

    fn database(&self) -> Database {
        self.clone()
    }
}

impl DatabaseHandle for SqlitePool {
    fn sqlx_pool(&self) -> &SqlitePool {
        self
    }

    fn database(&self) -> Database {
        Database(self.clone())
    }
}

#[async_trait]
impl Transactions for Database {
    async fn begin_write(&self) -> Result<WriteTx, sqlx::Error> {
        self.0.begin_write().await
    }

    async fn begin_read(&self) -> Result<ReadTx, sqlx::Error> {
        self.0.begin_read().await
    }
}
"#;

#[test]
fn the_core_database_file_passes_every_rule_and_the_same_trait_elsewhere_does_not() {
    assert_eq!(found(DATABASE, DATABASE_SOURCE), none());
    let elsewhere = "crates/lattice-learning/src/persistence/database.rs";
    assert_eq!(
        found(elsewhere, DATABASE_SOURCE),
        vec![hands(elsewhere, 32, "SqlitePool", "fn", "sqlx_pool")]
    );
}

#[test]
fn only_a_data_module_may_call_sqlx_pool() {
    let source = r#"fn a(db: &Database) {
    let pool = db.sqlx_pool();
    let other = DatabaseHandle::sqlx_pool(&db);
    let text = format!("{:?}", db.sqlx_pool());
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![
            unwraps(SERVICE, 2),
            unwraps(SERVICE, 3),
            unwraps(SERVICE, 4)
        ]
    );
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn the_sync_servers_accessor_is_exempt_and_its_connect_is_not_a_handout() {
    let source = "use sqlx::PgPool;
pub struct Database(PgPool);
impl Database {
    pub(crate) fn sqlx_pool(&self) -> &PgPool {
        &self.0
    }
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        todo!()
    }
}
";
    assert_eq!(found(SYNC_DB, source), none());
    let other = "crates/lattice-sync/src/persistence/pool.rs";
    assert_eq!(
        found(other, source),
        vec![hands(other, 4, "PgPool", "fn", "sqlx_pool")]
    );
}
