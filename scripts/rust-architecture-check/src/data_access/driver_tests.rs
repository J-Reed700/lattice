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
const DRIVER: &str = "crates/lattice-core/src/shared/persistence/driver.rs";
const TRANSACTIONS_FILE: &str = "crates/lattice-core/src/shared/persistence/transactions.rs";
const LEARNING_ROOT: &str = "crates/lattice-learning/src/lib.rs";
const LESSONS: &str = "crates/lattice-learning/src/lessons/mod.rs";
const INNER: &str = "crates/lattice-learning/src/lessons/repository/inner.rs";
const MIDDLE: &str = "crates/lattice-learning/src/lessons/repository/a.rs";
const LEAF: &str = "crates/lattice-learning/src/lessons/repository/a/b.rs";
const STORE_LIB: &str = "crates/lattice-store/src/lib.rs";
const STORE_INNER: &str = "crates/lattice-store/src/inner.rs";

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
fn a_trait_method_declaration_and_an_impl_that_reaches_outside_are_both_checked() {
    let declaration = "use sqlx::SqlitePool;
pub trait T {
    fn pool(&self) -> &SqlitePool;
}
";
    assert_eq!(
        found(REPOSITORY, declaration),
        vec![hands(REPOSITORY, 3, "SqlitePool", "fn", "pool")]
    );
    let public = "use sqlx::SqlitePool;
pub struct X {
    pool: SqlitePool,
}
impl T for X {
    fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
";
    assert_eq!(
        found(REPOSITORY, public),
        vec![hands(REPOSITORY, 6, "SqlitePool", "fn", "pool")]
    );
    let private = public.replace("pub struct", "struct");
    assert_eq!(found(REPOSITORY, &private), none());
    let elsewhere = "use sqlx::SqlitePool;
impl T for X {
    fn pool(&self) -> &SqlitePool {
        todo!()
    }
}
";
    assert_eq!(
        found(REPOSITORY, elsewhere),
        vec![hands(REPOSITORY, 3, "SqlitePool", "fn", "pool")]
    );
}

#[test]
fn an_associated_type_of_a_trait_impl_is_a_handout() {
    let public = "use sqlx::SqliteConnection;
pub struct Guard;
impl std::ops::Deref for Guard {
    type Target = SqliteConnection;
    fn deref(&self) -> &SqliteConnection {
        todo!()
    }
}
";
    assert_eq!(
        found(REPOSITORY, public),
        vec![
            hands(REPOSITORY, 4, "SqliteConnection", "type", "Target"),
            hands(REPOSITORY, 5, "SqliteConnection", "fn", "deref"),
        ]
    );
    let private = public.replace("pub struct", "struct");
    assert_eq!(found(REPOSITORY, &private), none());
}

#[test]
fn the_generic_arguments_and_self_type_of_a_trait_impl_are_checked() {
    let source = "use sqlx::SqlitePool;
pub struct Db(SqlitePool);
impl AsRef<SqlitePool> for Db {
    fn as_ref(&self) -> &SqlitePool {
        &self.0
    }
}
impl From<SqlitePool> for Db {
    fn from(pool: SqlitePool) -> Self {
        Self(pool)
    }
}
impl From<Db> for SqlitePool {
    fn from(db: Db) -> Self {
        db.0
    }
}
";
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            hands(REPOSITORY, 3, "SqlitePool", "impl", "AsRef for Db"),
            hands(REPOSITORY, 4, "SqlitePool", "fn", "as_ref"),
            hands(REPOSITORY, 13, "SqlitePool", "impl", "From for SqlitePool"),
        ]
    );
    let others = "use sqlx::SqlitePool;
pub struct Db(SqlitePool);
impl std::borrow::Borrow<SqlitePool> for Db {
    fn borrow(&self) -> &SqlitePool {
        &self.0
    }
}
impl Into<SqlitePool> for Db {
    fn into(self) -> SqlitePool {
        self.0
    }
}
";
    assert_eq!(
        found(REPOSITORY, others),
        vec![
            hands(REPOSITORY, 3, "SqlitePool", "impl", "Borrow for Db"),
            hands(REPOSITORY, 4, "SqlitePool", "fn", "borrow"),
            hands(REPOSITORY, 8, "SqlitePool", "impl", "Into for Db"),
            hands(REPOSITORY, 9, "SqlitePool", "fn", "into"),
        ]
    );
    let private = others.replace("pub struct", "struct");
    assert_eq!(found(REPOSITORY, &private), none());
}

#[test]
fn a_function_that_takes_a_closure_over_a_handle_hands_the_handle_to_it() {
    let source = "use sqlx::SqliteConnection;
pub fn with<F: FnOnce(&mut SqliteConnection)>(f: F) {}
pub fn wherever<F>(f: F)
where
    F: Fn(&SqliteConnection),
{
}
pub fn dynamic(f: impl FnMut(&mut SqliteConnection)) {}
pub fn boxed(f: Box<dyn Fn(&SqliteConnection)>) {}
pub fn pointer(f: fn(&SqliteConnection)) {}
pub trait Visitor {
    fn visit<F: FnOnce(&SqliteConnection)>(&self, f: F);
}
fn private<F: FnOnce(&mut SqliteConnection)>(f: F) {}
pub fn produces<F: Fn() -> SqliteConnection>(f: F) {}
pub fn takes(connection: &mut SqliteConnection) {}
";
    let handle = "SqliteConnection";
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            hands(REPOSITORY, 2, handle, "fn", "with"),
            hands(REPOSITORY, 3, handle, "fn", "wherever"),
            hands(REPOSITORY, 8, handle, "fn", "dynamic"),
            hands(REPOSITORY, 9, handle, "fn", "boxed"),
            hands(REPOSITORY, 10, handle, "fn", "pointer"),
            hands(REPOSITORY, 12, handle, "fn", "visit"),
        ]
    );
}

#[test]
fn a_handle_behind_a_same_file_alias_is_found() {
    let source = "use sqlx::SqlitePool;
type DbPool = SqlitePool;
type Nested = std::sync::Arc<DbPool>;
type Loop = Cycle;
type Cycle = Loop;
type Id = i64;
pub fn p() -> &'static DbPool {
    todo!()
}
pub fn q() -> Nested {
    todo!()
}
pub fn r() -> Loop {
    todo!()
}
pub fn s() -> Id {
    0
}
pub struct S {
    pub pool: DbPool,
}
";
    assert_eq!(
        found(REPOSITORY, source),
        vec![
            hands(REPOSITORY, 7, "DbPool", "fn", "p"),
            hands(REPOSITORY, 10, "Nested", "fn", "q"),
            hands(REPOSITORY, 20, "DbPool", "field", "S.pool"),
        ]
    );
}

#[test]
fn a_handle_name_that_arrives_without_a_sqlx_import_is_still_a_handle() {
    for (what, source, line) in [
        (
            "glob",
            "use super::*;\npub fn p() -> SqlitePool {\n    todo!()\n}\n",
            2,
        ),
        (
            "named from super",
            "use super::SqlitePool;\npub fn p() -> SqlitePool {\n    todo!()\n}\n",
            2,
        ),
        (
            "no import",
            "pub fn p() -> SqlitePool {\n    todo!()\n}\n",
            1,
        ),
    ] {
        assert_eq!(
            found(REPOSITORY, source),
            vec![hands(REPOSITORY, line, "SqlitePool", "fn", "p")],
            "{what}"
        );
    }
    let defined =
        "use super::*;\nstruct SqlitePool;\npub fn p() -> SqlitePool {\n    SqlitePool\n}\n";
    assert_eq!(found(REPOSITORY, defined), none());
    let renamed = "use crate::shared::persistence::Database as Pool;
pub fn o() -> Pool {
    todo!()
}
";
    assert_eq!(found(REPOSITORY, renamed), none());
    let domain = "use crate::banking::Transaction;
pub fn t() -> Transaction {
    todo!()
}
";
    assert_eq!(found(REPOSITORY, domain), none());
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

use super::{
    error::PersistenceError,
    transactions::{ReadTx, Transactions, WriteTx},
};

#[derive(Clone, Debug)]
pub struct Database(SqlitePool);

impl Database {
    pub async fn is_reachable(&self) -> bool {
        self.0.acquire().await.is_ok()
    }

    pub async fn close(&self) {
        self.0.close().await;
    }

    pub fn open_connections(&self) -> u32 {
        self.0.size()
    }
}

impl From<SqlitePool> for Database {
    fn from(pool: SqlitePool) -> Self {
        Self(pool)
    }
}

#[async_trait]
impl Transactions for Database {
    async fn begin_write(&self) -> Result<WriteTx, PersistenceError> {
        self.0.begin_write().await.map_err(PersistenceError::from)
    }

    async fn begin_read(&self) -> Result<ReadTx, PersistenceError> {
        self.0.begin_read().await.map_err(PersistenceError::from)
    }
}
"#;

const DRIVER_SOURCE: &str = "use sqlx::SqlitePool;
use super::{Database, PersistenceError, Transactions};

pub trait DriverPool: Transactions + Send + Sync + std::fmt::Debug {
    fn sqlx_pool(&self) -> &SqlitePool;
}

impl DriverPool for Database {
    fn sqlx_pool(&self) -> &SqlitePool {
        &self.0
    }
}

impl DriverPool for SqlitePool {
    fn sqlx_pool(&self) -> &SqlitePool {
        self
    }
}

impl From<PersistenceError> for sqlx::Error {
    fn from(error: PersistenceError) -> Self {
        sqlx::Error::Protocol(error.to_string())
    }
}
";

const DRIVER_EXTRAS: &str = "pub fn sqlx_pool(database: &Database) -> &SqlitePool {
    todo!()
}
pub struct Other;
impl Other {
    pub fn sqlx_pool(&self) -> &SqlitePool {
        todo!()
    }
}
impl DriverPool for Other {
    fn sqlx_pool(&self) -> &SqlitePool {
        todo!()
    }
}
pub trait Another {
    fn sqlx_pool(&self) -> &SqlitePool;
}
";

const GUARD_SOURCE: &str = "use sqlx::SqliteConnection;
use std::ops::{Deref, DerefMut};
pub struct WriteTx(Inner);
pub struct ReadTx(Inner);
impl Deref for WriteTx {
    type Target = SqliteConnection;
    fn deref(&self) -> &SqliteConnection {
        todo!()
    }
}
impl DerefMut for WriteTx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        todo!()
    }
}
impl Deref for ReadTx {
    type Target = SqliteConnection;
    fn deref(&self) -> &SqliteConnection {
        todo!()
    }
}
impl DerefMut for ReadTx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        todo!()
    }
}
";

const GUARD_EXTRAS: &str = "pub struct Other(Inner);
impl Deref for Other {
    type Target = SqliteConnection;
    fn deref(&self) -> &SqliteConnection {
        todo!()
    }
}
impl WriteTx {
    pub fn raw(&mut self) -> &mut SqliteConnection {
        todo!()
    }
}
";

#[test]
fn the_core_database_file_and_the_driver_door_pass_every_rule() {
    assert_eq!(found(DATABASE, DATABASE_SOURCE), none());
    assert_eq!(found(DRIVER, DRIVER_SOURCE), none());
}

#[test]
fn the_driver_trait_and_its_impls_hand_out_the_pool_anywhere_but_in_driver_rs() {
    let elsewhere = "crates/lattice-learning/src/persistence/driver.rs";
    assert_eq!(
        found(elsewhere, DRIVER_SOURCE),
        vec![
            hands(elsewhere, 5, "SqlitePool", "fn", "sqlx_pool"),
            hands(elsewhere, 9, "SqlitePool", "fn", "sqlx_pool"),
            hands(elsewhere, 15, "SqlitePool", "fn", "sqlx_pool"),
        ]
    );
}

#[test]
fn a_driver_exemption_covers_only_its_own_trait_or_impl() {
    let source = format!("{DRIVER_SOURCE}{DRIVER_EXTRAS}");
    assert_eq!(
        found(DRIVER, &source),
        vec![
            hands(DRIVER, 25, "SqlitePool", "fn", "sqlx_pool"),
            hands(DRIVER, 30, "SqlitePool", "fn", "sqlx_pool"),
            hands(DRIVER, 35, "SqlitePool", "fn", "sqlx_pool"),
            hands(DRIVER, 40, "SqlitePool", "fn", "sqlx_pool"),
        ]
    );
}

#[test]
fn the_unit_of_work_guard_may_deref_to_the_connection_and_no_other_type_may() {
    assert_eq!(found(TRANSACTIONS_FILE, GUARD_SOURCE), none());
    let elsewhere = "crates/lattice-learning/src/persistence/transactions.rs";
    let handle = "SqliteConnection";
    assert_eq!(
        found(elsewhere, GUARD_SOURCE),
        vec![
            hands(elsewhere, 6, handle, "type", "Target"),
            hands(elsewhere, 7, handle, "fn", "deref"),
            hands(elsewhere, 12, handle, "fn", "deref_mut"),
            hands(elsewhere, 17, handle, "type", "Target"),
            hands(elsewhere, 18, handle, "fn", "deref"),
            hands(elsewhere, 23, handle, "fn", "deref_mut"),
        ]
    );
    let source = format!("{GUARD_SOURCE}{GUARD_EXTRAS}");
    assert_eq!(
        found(TRANSACTIONS_FILE, &source),
        vec![
            hands(TRANSACTIONS_FILE, 29, handle, "type", "Target"),
            hands(TRANSACTIONS_FILE, 30, handle, "fn", "deref"),
            hands(TRANSACTIONS_FILE, 35, handle, "fn", "raw"),
        ]
    );
}

#[test]
fn only_a_data_module_may_call_sqlx_pool() {
    let source = r#"fn a(db: &Database) {
    let pool = db.sqlx_pool();
    let other = DriverPool::sqlx_pool(&db);
    let text = format!("{:?}", db.sqlx_pool());
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![
            names(SERVICE, 3, "DriverPool::sqlx_pool"),
            unwraps(SERVICE, 2),
            unwraps(SERVICE, 3),
            unwraps(SERVICE, 4)
        ]
    );
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn the_driver_trait_in_a_use_tree_is_flagged_whatever_its_form() {
    let source = "use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::persistence::driver::DriverPool as Pool;
use lattice_core::shared::persistence::driver::{self, DriverPool as Door};
use lattice_core::shared::persistence::driver::*;
use lattice_core::shared::persistence::{driver, Database};
use lattice_core::shared::persistence::Database as Db;
";
    let driver = "lattice_core::shared::persistence::driver";
    assert_eq!(
        found(SERVICE, source),
        vec![
            names(SERVICE, 1, &format!("{driver}::DriverPool")),
            names(SERVICE, 2, &format!("{driver}::DriverPool")),
            names(SERVICE, 3, &format!("{driver}, {driver}::DriverPool")),
            names(SERVICE, 4, &format!("{driver}::*")),
            names(SERVICE, 5, driver),
        ]
    );
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn the_driver_trait_in_a_bound_a_generic_argument_or_an_impl_is_flagged() {
    let source = "fn a(db: &impl DriverPool) {}
fn b<T: DriverPool>(db: &T) {}
fn c<T>(db: &T)
where
    T: DriverPool,
{
}
struct S {
    db: std::sync::Arc<dyn DriverPool>,
}
impl DriverPool for S {}
";
    assert_eq!(
        found(SERVICE, source),
        vec![
            names(SERVICE, 1, "DriverPool"),
            names(SERVICE, 2, "DriverPool"),
            names(SERVICE, 5, "DriverPool"),
            names(SERVICE, 9, "DriverPool"),
            names(SERVICE, 11, "DriverPool"),
        ]
    );
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn a_path_ending_in_sqlx_pool_is_flagged_as_a_call_a_reference_and_a_qualified_path() {
    let source = r#"fn a(db: &Database, dbs: &[Database]) {
    let f = Database::sqlx_pool;
    let all: Vec<_> = dbs.iter().map(DriverPool::sqlx_pool).collect();
    let one = DriverPool::sqlx_pool(db);
    let two = <Database as DriverPool>::sqlx_pool(db);
    let three = format!("{:?}", DriverPool::sqlx_pool(db));
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![
            names(SERVICE, 2, "Database::sqlx_pool"),
            names(SERVICE, 3, "DriverPool::sqlx_pool"),
            names(SERVICE, 4, "DriverPool::sqlx_pool"),
            names(SERVICE, 5, "DriverPool::sqlx_pool"),
            names(SERVICE, 6, "DriverPool::sqlx_pool"),
            unwraps(SERVICE, 4),
            unwraps(SERVICE, 5),
            unwraps(SERVICE, 6),
        ]
    );
    assert_eq!(found(REPOSITORY, source), none());
}

#[test]
fn test_code_may_name_the_driver_trait() {
    let source = "#[cfg(test)]
mod tests {
    use lattice_core::shared::persistence::driver::DriverPool;
    fn f(db: &impl DriverPool) {
        db.sqlx_pool();
    }
}
";
    assert_eq!(found(SERVICE, source), none());
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

#[test]
fn the_sync_exemption_covers_only_database_sqlx_pool() {
    let source = "use sqlx::PgPool;
pub struct Database(PgPool);
impl Database {
    pub(crate) fn sqlx_pool(&self) -> &PgPool {
        &self.0
    }
    pub fn pool(&self) -> &PgPool {
        &self.0
    }
}
pub struct Other(PgPool);
impl Other {
    pub fn sqlx_pool(&self) -> &PgPool {
        &self.0
    }
}
pub fn sqlx_pool(database: &Database) -> &PgPool {
    &database.0
}
";
    assert_eq!(
        found(SYNC_DB, source),
        vec![
            hands(SYNC_DB, 7, "PgPool", "fn", "pool"),
            hands(SYNC_DB, 13, "PgPool", "fn", "sqlx_pool"),
            hands(SYNC_DB, 17, "PgPool", "fn", "sqlx_pool"),
        ]
    );
}

#[test]
fn a_re_export_of_a_private_modules_handout_is_flagged_where_it_leaves_the_data_module() {
    let inner = "use sqlx::SqlitePool;
pub fn pool_of() -> &'static SqlitePool {
    todo!()
}
";
    let private = [
        (REPOSITORY, "mod inner;\nuse inner::pool_of;\n"),
        (INNER, inner),
    ];
    assert_eq!(found_among(&private, REPOSITORY), none());
    assert_eq!(found_among(&private, INNER), none());
    let public = [
        (REPOSITORY, "mod inner;\npub use inner::pool_of;\n"),
        (INNER, inner),
    ];
    assert_eq!(
        found_among(&public, REPOSITORY),
        vec![hands(REPOSITORY, 2, "SqlitePool", "re-export", "pool_of")]
    );
    assert_eq!(found_among(&public, INNER), none());
}

#[test]
fn every_form_of_a_re_export_reaches_the_handout() {
    let inner = "use sqlx::SqlitePool;
pub fn pool_of() -> &'static SqlitePool {
    todo!()
}
";
    for (form, source, shown) in [
        (
            "crate-wide",
            "mod inner;\npub(crate) use inner::pool_of;\n",
            "pool_of",
        ),
        (
            "self path",
            "mod inner;\npub use self::inner::pool_of;\n",
            "pool_of",
        ),
        (
            "crate path",
            "mod inner;\npub use crate::lessons::repository::inner::pool_of;\n",
            "pool_of",
        ),
        (
            "renamed",
            "mod inner;\npub use inner::pool_of as get_pool;\n",
            "get_pool",
        ),
        (
            "grouped",
            "mod inner;\npub use inner::{pool_of, other};\n",
            "pool_of",
        ),
        ("glob", "mod inner;\npub use inner::*;\n", "pool_of"),
    ] {
        let files = [
            (LEARNING_ROOT, "pub mod lessons;\n"),
            (LESSONS, "pub mod repository;\n"),
            (REPOSITORY, source),
            (INNER, inner),
        ];
        assert_eq!(
            found_among(&files, REPOSITORY),
            vec![hands(REPOSITORY, 2, "SqlitePool", "re-export", shown)],
            "{form}"
        );
    }
}

#[test]
fn a_re_export_chain_is_followed_to_the_handout() {
    let leaf = "use sqlx::SqlitePool;
pub fn pool_of() -> &'static SqlitePool {
    todo!()
}
";
    let files = [
        (REPOSITORY, "mod a;\npub use a::*;\n"),
        (MIDDLE, "mod b;\npub use b::pool_of;\n"),
        (LEAF, leaf),
    ];
    assert_eq!(
        found_among(&files, REPOSITORY),
        vec![hands(REPOSITORY, 2, "SqlitePool", "re-export", "pool_of")]
    );
    assert_eq!(found_among(&files, MIDDLE), none());
    assert_eq!(found_among(&files, LEAF), none());
}

#[test]
fn a_re_exported_type_hands_out_what_its_public_fields_and_methods_return() {
    let inner = "use sqlx::SqlitePool;
pub struct Db {
    pub pool: SqlitePool,
    secret: SqlitePool,
}
pub struct Hidden(SqlitePool);
pub struct Wrapper;
impl Wrapper {
    pub fn pool(&self) -> &SqlitePool {
        todo!()
    }
}
pub fn count() -> i64 {
    0
}
";
    for (name, handed) in [
        ("Db", true),
        ("Hidden", false),
        ("Wrapper", true),
        ("count", false),
    ] {
        let source = format!("mod inner;\npub use inner::{name};\n");
        let files = [(REPOSITORY, source.as_str()), (INNER, inner)];
        let expected = if handed {
            vec![hands(REPOSITORY, 2, "SqlitePool", "re-export", name)]
        } else {
            none()
        };
        assert_eq!(found_among(&files, REPOSITORY), expected, "{name}");
    }
}

#[test]
fn the_store_crates_public_api_is_checked_and_its_crate_private_items_are_not() {
    let source = "use sqlx::SqlitePool;
pub fn open() -> SqlitePool {
    todo!()
}
pub(crate) fn crate_wide() -> SqlitePool {
    todo!()
}
fn private() -> SqlitePool {
    todo!()
}
pub struct S {
    pub pool: SqlitePool,
    pub(crate) inner: SqlitePool,
}
mod hidden {
    use sqlx::SqlitePool;
    pub fn f() -> SqlitePool {
        todo!()
    }
}
pub mod open_mod {
    use sqlx::SqlitePool;
    pub fn g() -> SqlitePool {
        todo!()
    }
}
";
    assert_eq!(
        found(STORE_LIB, source),
        vec![
            hands(STORE_LIB, 2, "SqlitePool", "fn", "open"),
            hands(STORE_LIB, 12, "SqlitePool", "field", "S.pool"),
            hands(STORE_LIB, 23, "SqlitePool", "fn", "g"),
        ]
    );
}

#[test]
fn the_store_crates_public_re_export_of_a_hidden_handout_is_flagged() {
    let inner = "use sqlx::SqlitePool;
pub fn pool_of() -> &'static SqlitePool {
    todo!()
}
";
    for (form, lib, flagged) in [
        ("pub", "mod inner;\npub use inner::pool_of;\n", true),
        (
            "crate",
            "mod inner;\npub(crate) use inner::pool_of;\n",
            false,
        ),
        ("private", "mod inner;\nuse inner::pool_of;\n", false),
    ] {
        let files = [(STORE_LIB, lib), (STORE_INNER, inner)];
        let expected = if flagged {
            vec![hands(STORE_LIB, 2, "SqlitePool", "re-export", "pool_of")]
        } else {
            none()
        };
        assert_eq!(found_among(&files, STORE_LIB), expected, "{form}");
        assert_eq!(found_among(&files, STORE_INNER), none(), "{form}");
    }
}

#[test]
fn an_exemption_is_keyed_by_the_file_and_not_by_the_module_path_alone() {
    const SYNC_PERSISTENCE: &str = "crates/lattice-sync/src/persistence.rs";
    const OTHER_PERSISTENCE: &str = "crates/lattice-other/src/persistence.rs";
    const OTHER_DB: &str = "crates/lattice-other/src/persistence/db.rs";
    let db = "use sqlx::SqlitePool;
pub struct Database {
    pool: SqlitePool,
}
impl Database {
    pub fn sqlx_pool(&self) -> &SqlitePool {
        &self.pool
    }
}
";
    let persistence = "pub mod db;
pub use db::Database;
";
    let sync = [(SYNC_PERSISTENCE, persistence), (SYNC_DB, db)];
    assert_eq!(found_among(&sync, SYNC_DB), none());
    assert_eq!(found_among(&sync, SYNC_PERSISTENCE), none());
    let other = [(OTHER_PERSISTENCE, persistence), (OTHER_DB, db)];
    assert_eq!(
        found_among(&other, OTHER_DB),
        vec![hands(OTHER_DB, 6, "SqlitePool", "fn", "sqlx_pool")]
    );
    assert_eq!(
        found_among(&other, OTHER_PERSISTENCE),
        vec![hands(
            OTHER_PERSISTENCE,
            2,
            "SqlitePool",
            "re-export",
            "Database"
        )]
    );
}
