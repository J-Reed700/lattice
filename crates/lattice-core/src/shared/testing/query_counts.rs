//! SQLx statement budgets, routed by a measurement span through SQLite's worker.
//! One filtered test subscriber keeps callsite registration stable while
//! unrelated queries and parallel tests remain outside each counter.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::shared::persistence::transactions::CHANGED_ROWS;
use std::future::Future;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tracing::{field::Visit, subscriber::Interest, Instrument, Subscriber};
use tracing_subscriber::{
    filter::dynamic_filter_fn, layer::Context, registry::LookupSpan, Layer, Registry,
};

#[derive(Clone, Default)]
struct QueryCount {
    reads: Arc<AtomicUsize>,
    writes: Arc<AtomicUsize>,
}
struct Reads;

pub(super) fn layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    Reads.with_filter(
        dynamic_filter_fn(|metadata, cx| {
            (metadata.is_span() && metadata.name() == "count_reads") || current_count(cx).is_some()
        })
        .with_callsite_filter(|metadata| {
            if metadata.is_span() || metadata.target() == "sqlx::query" {
                Interest::sometimes()
            } else {
                Interest::never()
            }
        }),
    )
}

fn current_count<S: Subscriber + for<'lookup> LookupSpan<'lookup>>(
    ctx: &Context<'_, S>,
) -> Option<QueryCount> {
    let current = ctx.lookup_current()?;
    for span in current.scope() {
        if let Some(count) = span.extensions().get::<QueryCount>() {
            return Some(count.clone());
        }
    }
    None
}

#[derive(Default)]
struct Statement {
    sql: String,
    summary: String,
}
impl Visit for Statement {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "db.statement" => self.sql = value.trim().to_owned(),
            "summary" => self.summary = value.trim().to_owned(),
            _ => {}
        }
    }
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
}

impl<S: Subscriber + for<'lookup> LookupSpan<'lookup>> Layer<S> for Reads {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        if attrs.metadata().name() == "count_reads" {
            ctx.span(id)
                .unwrap()
                .extensions_mut()
                .insert(QueryCount::default());
        }
    }
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let Some(count) = current_count(&ctx) else {
            return;
        };
        let mut statement = Statement::default();
        event.record(&mut statement);
        let sql = if statement.sql.is_empty() {
            statement.summary
        } else {
            statement.sql
        };
        if sql == CHANGED_ROWS {
            return;
        }
        if sql.starts_with("SELECT") || sql.starts_with("WITH") {
            count.reads.fetch_add(1, Ordering::Relaxed);
        } else if ["INSERT", "UPDATE", "DELETE", "REPLACE"]
            .iter()
            .any(|prefix| sql.starts_with(prefix))
        {
            count.writes.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Counts SELECT/WITH statements, excluding a read unit's check of its own changes.
pub async fn count_reads<F: Future>(operation: F) -> (F::Output, usize) {
    let (result, count) = count_queries(operation).await;
    (result, count.reads.load(Ordering::Relaxed))
}

/// Counts INSERT/UPDATE/DELETE/REPLACE statements, excluding transaction control.
pub async fn count_writes<F: Future>(operation: F) -> (F::Output, usize) {
    let (result, count) = count_queries(operation).await;
    (result, count.writes.load(Ordering::Relaxed))
}

async fn count_queries<F: Future>(operation: F) -> (F::Output, QueryCount) {
    super::subscriber::install();
    let span = tracing::info_span!(parent: None, "count_reads");
    let count = span
        .with_subscriber(|(id, dispatch)| {
            dispatch
                .downcast_ref::<Registry>()
                .unwrap()
                .span(id)
                .unwrap()
                .extensions()
                .get::<QueryCount>()
                .unwrap()
                .clone()
        })
        .unwrap();
    let result = operation.instrument(span).await;
    (result, count)
}

#[tokio::test]
async fn counter_observes_worker_reads_and_excludes_writes() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let (_, count) = count_reads(async {
        sqlx::query("CREATE TABLE test (id INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        async {
            sqlx::query("SELECT 1").fetch_one(&pool).await.unwrap();
        }
        .instrument(tracing::info_span!("inner_reader"))
        .await;
        sqlx::query("SELECT 2").fetch_one(&pool).await.unwrap();
    })
    .await;
    assert_eq!(count, 2);

    let (_, writes) = count_writes(async {
        sqlx::query("INSERT INTO test VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("SELECT id FROM test")
            .fetch_all(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE test SET id = 2")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM test")
            .execute(&pool)
            .await
            .unwrap();
    })
    .await;
    assert_eq!(writes, 3);
}

#[tokio::test]
async fn a_read_units_check_of_its_own_changes_is_not_counted() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let database = crate::shared::persistence::Database::new(pool);
    let (_, count) = count_reads(async {
        let mut read = database.begin_read().await.unwrap();
        sqlx::query("SELECT 1").fetch_one(&mut *read).await.unwrap();
        read.commit().await.unwrap();
    })
    .await;
    assert_eq!(count, 1);
}
