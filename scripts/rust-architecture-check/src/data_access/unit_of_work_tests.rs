use super::*;

const SERVICE: &str = "crates/lattice-learning/src/lessons/service.rs";
const REPOSITORY: &str = "crates/lattice-learning/src/lessons/repository.rs";
const OWNER: &str = "crates/lattice-desktop/src/features/indexing/library_owner.rs";

fn found(path: &str, source: &str) -> Vec<String> {
    let ast = syn::parse_file(source).unwrap();
    file_violations(path, &ast)
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn unit_type(path: &str, line: usize, what: &str) -> String {
    format!(
        "rule 4: {path}:{line} names {what} outside a data module; use WriteTx or ReadTx, \
         or move the code into a data module"
    )
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
fn a_service_naming_a_sqlx_unit_of_work_type_is_flagged() {
    let source = r#"use sqlx::{Sqlite, Transaction};
use sqlx::sqlite::SqliteConnection;
pub async fn go(tx: &mut sqlx::Transaction<'_, Sqlite>, c: &mut sqlx::sqlite::SqliteConnection) {}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![
            unit_type(SERVICE, 1, "sqlx::Transaction"),
            unit_type(SERVICE, 2, "sqlx::sqlite::SqliteConnection"),
            unit_type(SERVICE, 3, "sqlx::Transaction"),
            unit_type(SERVICE, 3, "sqlx::sqlite::SqliteConnection"),
        ]
    );
}

#[test]
fn a_use_tree_rename_does_not_hide_the_type() {
    let source = "use sqlx::Transaction as Tx;
use sqlx::{Acquire as _, SqliteExecutor};
";
    assert_eq!(
        found(SERVICE, source),
        vec![
            unit_type(SERVICE, 1, "sqlx::Transaction"),
            unit_type(SERVICE, 2, "sqlx::Acquire, sqlx::SqliteExecutor"),
        ]
    );
}

#[test]
fn qualified_paths_in_bounds_expressions_and_impls_are_flagged() {
    let source = r#"fn a<E: sqlx::Executor<'static>>(e: E) {}
fn b() { let c: Option<sqlx::pool::PoolConnection<sqlx::Sqlite>> = None; }
fn c() -> <sqlx::SqliteConnection as Debug>::Output { todo() }
impl Foo for sqlx::SqliteTransaction {}
fn e() { let _ = <sqlx::Transaction<'static, sqlx::Sqlite> as Default>::default(); }
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![
            unit_type(SERVICE, 1, "sqlx::Executor"),
            unit_type(SERVICE, 2, "sqlx::pool::PoolConnection"),
            unit_type(SERVICE, 3, "sqlx::SqliteConnection"),
            unit_type(SERVICE, 4, "sqlx::SqliteTransaction"),
            unit_type(SERVICE, 5, "sqlx::Transaction"),
        ]
    );
}

#[test]
fn the_same_types_are_fine_in_a_data_module_test_code_and_the_sync_server() {
    let source = r#"use sqlx::{Sqlite, Transaction};
pub async fn go(tx: &mut sqlx::Transaction<'_, Sqlite>, c: &mut sqlx::SqliteConnection) {}
"#;
    assert_eq!(found(REPOSITORY, source), Vec::<String>::new());
    assert_eq!(
        found("crates/lattice-sync/src/service.rs", source),
        Vec::<String>::new()
    );
    let tests = r#"#[cfg(test)]
mod tests {
    use sqlx::Transaction;
    fn f(c: &mut sqlx::SqliteConnection) {}
}
"#;
    assert_eq!(found(SERVICE, tests), Vec::<String>::new());
}

#[test]
fn the_pool_errors_rows_and_other_sqlx_names_are_not_refused() {
    let source = r#"use sqlx::{Error, FromRow, Pool, Row, Sqlite, SqlitePool, Type};
use sqlx::sqlite::SqliteRow;
fn f(p: &SqlitePool, q: &Pool<Sqlite>, e: sqlx::Error, r: SqliteRow) {}
"#;
    assert_eq!(found(SERVICE, source), Vec::<String>::new());
}

#[test]
fn a_service_unit_of_work_with_a_call_on_the_pool_is_flagged() {
    let source = r#"impl Service {
    pub async fn rename(&self) -> Result<()> {
        let n = self.repo.count(&self.pool).await?;
        let mut tx = self.pool.begin_write().await?;
        self.repo.save_in(&mut tx, n).await?;
        tx.commit().await?;
        Ok(())
    }
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![on_pool(SERVICE, 2, "rename", 1, 2, "")]
    );
}

#[test]
fn a_service_unit_of_work_that_goes_through_its_unit_is_not_flagged() {
    let source = r#"impl Service {
    pub async fn rename(&self, id: i64) -> Result<()> {
        let mut tx = self.pool.begin_write().await.map_err(db)?;
        foo_in(&mut tx).await?;
        bar_in(&mut *tx, id).await?;
        baz_in(&mut **tx).await?;
        self.repo.qux(tx.as_mut()).await?;
        tx.commit().await.map_err(db)?;
        self.events.publish(id).await;
        Ok(())
    }
    pub async fn show(&self) -> Result<()> {
        let mut read = self.pool.begin_read().await?;
        let x = self.repo.get_in(&mut read).await?;
        self.llm.complete(x).await?;
        Ok(())
    }
}
"#;
    assert_eq!(found(SERVICE, source), Vec::<String>::new());
}

#[test]
fn a_function_that_is_not_a_unit_of_work_is_not_checked_outside_data_modules() {
    let source = r#"impl Service {
    pub async fn show(&self) -> Result<()> {
        self.repo.a(&self.pool).await?;
        self.repo.b(&self.pool).await?;
        Ok(())
    }
}
"#;
    assert_eq!(found(SERVICE, source), Vec::<String>::new());
}

#[test]
fn two_write_units_in_a_service_function_are_flagged() {
    let source = r#"impl Service {
    pub async fn move_it(&self) -> Result<()> {
        let mut a = self.pool.begin_write().await?;
        self.repo.save_in(&mut a).await?;
        a.commit().await?;
        let mut b = self.pool.begin_write().await?;
        self.repo.save_in(&mut b).await?;
        b.commit().await?;
        Ok(())
    }
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![begins(SERVICE, 2, "move_it", 2, 0)]
    );
}

#[test]
fn a_read_unit_used_after_the_write_unit_begins_is_flagged() {
    let source = r#"impl Service {
    pub async fn rename(&self) -> Result<()> {
        let mut read = self.pool.begin_read().await?;
        let mut tx = self.pool.begin_write().await?;
        let before = self.repo.get_in(&mut read, 1).await?;
        self.repo.save_in(&mut tx, before).await?;
        tx.commit().await?;
        Ok(())
    }
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![
            reads_late(SERVICE, 2, "rename"),
            awaits(SERVICE, 5, "rename", "get_in()"),
        ]
    );
}

#[test]
fn a_data_call_after_the_last_commit_is_flagged_and_an_early_return_commit_is_not() {
    let after = r#"impl Service {
    pub async fn rename(&self) -> Result<()> {
        let mut tx = self.pool.begin_write().await?;
        self.repo.save_in(&mut tx, 1).await?;
        tx.commit().await?;
        self.repo.touch(&self.pool).await?;
        Ok(())
    }
}
"#;
    assert_eq!(
        found(SERVICE, after),
        vec![
            on_pool(SERVICE, 2, "rename", 1, 2, ""),
            after_commit(SERVICE, 2, "rename"),
        ]
    );
    let early = r#"impl Service {
    pub async fn rename(&self, id: i64) -> Result<()> {
        let mut tx = self.pool.begin_write().await?;
        if self.repo.seen_in(&mut tx, id).await? {
            tx.commit().await?;
            return Ok(());
        }
        self.repo.save_in(&mut tx, id).await?;
        tx.commit().await?;
        Ok(())
    }
}
"#;
    assert_eq!(found(SERVICE, early), Vec::<String>::new());
}

#[test]
fn a_same_file_helper_that_uses_the_pool_counts_in_a_service_unit() {
    let source = r#"impl Service {
    pub async fn rename(&self) -> Result<()> {
        let mut tx = self.pool.begin_write().await?;
        self.repo.save_in(&mut tx).await?;
        tx.commit().await?;
        self.refresh().await?;
        Ok(())
    }
    async fn refresh(&self) -> Result<()> {
        self.repo.load(&self.pool).await
    }
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![
            on_pool(SERVICE, 2, "rename", 1, 2, "refresh"),
            after_commit(SERVICE, 2, "rename"),
        ]
    );
}

#[test]
fn begin_write_is_recognised_as_a_path_call_and_on_any_receiver() {
    let source = r#"pub async fn first(pool: &SqlitePool) -> Result<()> {
    load(pool).await?;
    let mut tx = Transactions::begin_write(pool).await?;
    save_in(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn second(pool: &SqlitePool) -> Result<()> {
    load(pool).await?;
    let mut tx = pool.begin_write().await?;
    save_in(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![
            on_pool(SERVICE, 1, "first", 1, 2, ""),
            on_pool(SERVICE, 8, "second", 1, 2, ""),
        ]
    );
}

#[test]
fn a_model_call_under_an_open_write_unit_is_flagged() {
    let source = r#"impl Service {
    pub async fn summarize(&self, id: i64) -> Result<()> {
        let mut tx = self.pool.begin_write().await?;
        let text = self.repo.text_in(&mut tx, id).await?;
        let summary = self.llm.complete(&text).await?;
        self.repo.save_in(&mut tx, &summary).await?;
        tx.commit().await?;
        Ok(())
    }
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![awaits(SERVICE, 5, "summarize", "complete()")]
    );
}

#[test]
fn a_file_write_under_an_open_write_unit_is_flagged() {
    let source = r#"impl Service {
    pub async fn store(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let mut tx = self.pool.begin_write().await?;
        tokio::fs::write(path, bytes).await?;
        self.repo.save_in(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![awaits(SERVICE, 4, "store", "tokio::fs::write()")]
    );
}

#[test]
fn without_a_commit_the_unit_stays_open_to_the_end() {
    let source = r#"impl Service {
    pub async fn rename(&self, id: i64) -> Result<()> {
        let mut tx = self.pool.begin_write().await?;
        self.repo.save_in(&mut tx, id).await?;
        self.events.publish(id).await;
        Ok(())
    }
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![awaits(SERVICE, 5, "rename", "publish()")]
    );
}

#[test]
fn a_commit_in_an_early_return_block_closes_only_that_block() {
    let source = r#"impl Service {
    pub async fn rename(&self, id: i64) -> Result<()> {
        let mut tx = self.pool.begin_write().await?;
        if self.repo.seen_in(&mut tx, id).await? {
            tx.commit().await?;
            self.events.publish(id).await;
            return Ok(());
        }
        self.events.publish(id).await;
        self.repo.save_in(&mut tx, id).await?;
        tx.commit().await?;
        Ok(())
    }
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![awaits(SERVICE, 9, "rename", "publish()")]
    );
}

#[test]
fn a_data_module_may_execute_on_the_unit_but_not_await_anything_else() {
    let allowed = r#"pub async fn rename(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin_write().await?;
    sqlx::query("UPDATE a").execute(&mut *tx).await?;
    sqlx::query("UPDATE b").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
"#;
    assert_eq!(found(REPOSITORY, allowed), Vec::<String>::new());
    let refused = r#"pub async fn rename(pool: &SqlitePool) {
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
    tokio::time::sleep(delay).await;
    sqlx::query("UPDATE b").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
"#;
    assert_eq!(
        found(REPOSITORY, refused),
        vec![awaits(REPOSITORY, 4, "rename", "tokio::time::sleep()")]
    );
}

#[test]
fn a_function_that_takes_the_write_unit_is_under_it_throughout() {
    let source = r#"pub async fn record_in(tx: &mut WriteTx, id: i64) -> Result<()> {
    check_in(tx, id).await?;
    notify(id).await;
    Ok(())
}
pub async fn peek(tx: &mut ReadTx, id: i64) -> Result<()> {
    check_in(tx, id).await?;
    notify(id).await;
    Ok(())
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![awaits(SERVICE, 3, "record_in", "notify()")]
    );
}

#[test]
fn a_unit_opened_inside_a_closure_is_checked() {
    let source = r#"impl Service {
    pub async fn rename(&self, id: i64) -> Result<()> {
        query_with_timeout(|| async {
            let mut tx = self.pool.begin_write().await?;
            self.llm.complete(id).await?;
            self.repo.save_in(&mut tx, id).await?;
            tx.commit().await?;
            Ok(())
        })
        .await
    }
}
"#;
    assert_eq!(
        found(SERVICE, source),
        vec![awaits(SERVICE, 5, "rename", "complete()")]
    );
}

#[test]
fn the_library_owner_exception_is_honored() {
    let source = r#"impl Owner {
    pub async fn adopt(&self) -> Result<()> {
        let mut tx = self.pool.begin_write().await?;
        self.repo.set_owner_in(&mut tx, &self.id).await?;
        tokio::fs::File::create_new(&self.path).await?;
        tx.commit().await?;
        Ok(())
    }
}
"#;
    assert_eq!(found(OWNER, source), Vec::<String>::new());
    assert_eq!(
        found(SERVICE, source),
        vec![awaits(SERVICE, 5, "adopt", "tokio::fs::File::create_new()")]
    );
    let ast = syn::parse_file(source).unwrap();
    assert_eq!(file_violations_with(OWNER, &ast, &[]).len(), 1);
}
