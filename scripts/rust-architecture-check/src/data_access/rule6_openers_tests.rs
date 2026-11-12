use super::*;

const SERVICE: &str = "crates/lattice-learning/src/lessons/service.rs";
const REPOSITORY: &str = "crates/lattice-learning/src/lessons/repository.rs";
const CORE: &str = "crates/lattice-core/src/shared/persistence/database.rs";
const CORE_OTHER: &str = "crates/lattice-core/src/shared/jobs.rs";
const LIVE: &str = "crates/lattice-desktop/src/infrastructure/persistence/database/open.rs";
const INDEX: &str = "crates/lattice-index/src/passage_index/store.rs";
const INDEX_OTHER: &str = "crates/lattice-index/src/passage_index/search.rs";
const SYNC: &str = "crates/lattice-sync/src/persistence/db.rs";
const OPENS: &str = "opens a database outside the openers; take the Database you are given instead";

fn found(path: &str, source: &str) -> Vec<String> {
    let ast = syn::parse_file(source).unwrap();
    file_violations(path, &ast)
        .iter()
        .filter(|violation| violation.rule == 6)
        .map(ToString::to_string)
        .collect()
}

fn opens(path: &str, line: usize) -> String {
    format!("rule 6: {path}:{line} {OPENS}")
}

fn none() -> Vec<String> {
    Vec::new()
}

const NAMES: [&str; 4] = [
    "SqlitePool",
    "SqlitePoolOptions",
    "SqliteConnectOptions",
    "PoolOptions",
];

const CALLS: [&str; 6] = [
    "Database::new(pool)",
    "SqliteConnection::connect(url)",
    "SqliteConnection::connect_with(&options)",
    "Connection::connect(url)",
    "Connection::connect_with(&options)",
    "<SqliteConnection as Connection>::connect_with(&options)",
];

/// Every form in one file: a use, a field, a parameter, a call, a path call; one per line.
const EVERYTHING: &str = "use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
struct Holder {
    pool: SqlitePool,
}
fn open(url: &str, options: &SqliteConnectOptions) {
    let db = Database::new(pool);
    let pool = SqlitePool::connect(url);
    let conn = SqliteConnection::connect(url);
}
";

#[test]
fn every_pool_name_is_flagged_as_a_use_a_type_and_a_path() {
    for name in NAMES {
        let used = format!("use sqlx::sqlite::{name};\n");
        assert_eq!(found(SERVICE, &used), vec![opens(SERVICE, 1)], "{name}");
        let field = format!("struct S {{\n    pool: Arc<{name}>,\n}}\n");
        assert_eq!(found(SERVICE, &field), vec![opens(SERVICE, 2)], "{name}");
        let parameter = format!("fn f(\n    pool: &{name},\n) {{}}\n");
        assert_eq!(
            found(SERVICE, &parameter),
            vec![opens(SERVICE, 2)],
            "{name}"
        );
        let call = format!("fn f() {{\n    let a = sqlx::sqlite::{name}::new();\n}}\n");
        assert_eq!(found(SERVICE, &call), vec![opens(SERVICE, 2)], "{name}");
        let generic = format!("fn f() {{\n    let a = {name}::<Sqlite>::new();\n}}\n");
        assert_eq!(found(SERVICE, &generic), vec![opens(SERVICE, 2)], "{name}");
    }
}

#[test]
fn a_renamed_and_a_grouped_use_are_flagged_per_name() {
    let source = "use sqlx::SqlitePool as Pool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteJournalMode};
";
    assert_eq!(
        found(SERVICE, source),
        vec![opens(SERVICE, 1), opens(SERVICE, 2), opens(SERVICE, 2)]
    );
}

#[test]
fn every_opening_call_is_flagged_once() {
    for call in CALLS {
        let source = format!("fn f() {{\n    let a = {call};\n}}\n");
        assert_eq!(found(SERVICE, &source), vec![opens(SERVICE, 2)], "{call}");
    }
}

#[test]
fn a_call_on_a_pool_named_type_is_flagged_once_not_per_name() {
    let source = "fn f() {
    let a = SqlitePool::connect(url);
    let b = SqlitePool::connect_with(options);
    let c = SqlitePool::connect_lazy(url);
}
";
    assert_eq!(
        found(SERVICE, source),
        vec![opens(SERVICE, 2), opens(SERVICE, 3), opens(SERVICE, 4)]
    );
}

#[test]
fn a_database_and_a_connection_by_themselves_are_not_openers() {
    let source = "use lattice_core::shared::persistence::Database;
use sqlx::SqliteConnection as Raw;
fn f(db: &Database, other: Database) {
    let again = db.clone();
    let made = Self::new(db.clone());
    let other = Holder::new(db);
    let ok = SqliteJournalMode::Wal;
}
fn g(conn: &mut SqliteConnection) {}
";
    assert_eq!(found(SERVICE, source), vec![]);
}

#[test]
fn a_name_in_a_macro_body_is_flagged() {
    let source = "fn f() {
    tracing::info!(\"{:?}\", SqlitePool::connect(url));
    let v = vec![Database::new(pool)];
}
";
    assert_eq!(
        found(SERVICE, source),
        vec![opens(SERVICE, 2), opens(SERVICE, 3)]
    );
}

#[test]
fn data_modules_are_not_exempt() {
    assert_eq!(
        found(
            REPOSITORY,
            "fn f() {\n    let db = Database::new(pool);\n}\n"
        ),
        vec![opens(REPOSITORY, 2)]
    );
}

#[test]
fn the_openers_may_name_and_call_everything() {
    for path in [CORE, LIVE, INDEX] {
        assert_eq!(found(path, EVERYTHING), none(), "{path}");
    }
    assert_eq!(
        found(
            "crates/lattice-core/src/shared/persistence/nested/more.rs",
            EVERYTHING
        ),
        none()
    );
}

#[test]
fn only_the_listed_prefixes_and_files_are_openers() {
    for path in [CORE_OTHER, INDEX_OTHER, SERVICE] {
        assert_eq!(found(path, EVERYTHING).len(), 8, "{path}");
    }
}

#[test]
fn the_sync_server_is_not_scanned() {
    assert_eq!(found(SYNC, EVERYTHING), none());
}

#[test]
fn test_only_code_may_open_a_database() {
    let source = "#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;
    fn f() {
        let db = Database::new(pool);
        let pool = SqlitePool::connect(url);
    }
}
#[cfg(test)]
async fn helper() -> SqlitePool {
    SqlitePool::connect(url)
}
";
    assert_eq!(found(SERVICE, source), none());
    let whole = format!("#![cfg(test)]\n{EVERYTHING}");
    assert_eq!(found(SERVICE, &whole), none());
}
