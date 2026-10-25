//! SQLx read budgets, routed by a measurement span through SQLite's worker.
//! One filtered test subscriber keeps callsite registration stable while
//! unrelated queries and parallel tests remain outside each counter.

use std::future::Future;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, OnceLock,
};
use tracing::{field::Visit, Instrument, Subscriber};
use tracing_subscriber::{layer::Context, prelude::*, registry::LookupSpan, Layer, Registry};

#[derive(Clone, Default)]
struct ReadCount(Arc<AtomicUsize>);
struct Reads;

fn current_count<S: Subscriber + for<'lookup> LookupSpan<'lookup>>(
    ctx: &Context<'_, S>,
) -> Option<ReadCount> {
    let current = ctx.lookup_current()?;
    for span in current.scope() {
        if let Some(count) = span.extensions().get::<ReadCount>() {
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
    fn register_callsite(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        if metadata.is_span() || metadata.target() == "sqlx::query" {
            tracing::subscriber::Interest::sometimes()
        } else {
            tracing::subscriber::Interest::never()
        }
    }
    fn enabled(&self, metadata: &tracing::Metadata<'_>, ctx: Context<'_, S>) -> bool {
        (metadata.is_span() && metadata.name() == "count_reads") || current_count(&ctx).is_some()
    }
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
                .insert(ReadCount::default());
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
        if sql.starts_with("SELECT") || sql.starts_with("WITH") {
            count.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub async fn count_reads<F: Future>(operation: F) -> (F::Output, usize) {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(Reads))
            .expect("query budgets install the library test subscriber once");
    });
    let span = tracing::info_span!(parent: None, "count_reads");
    let count = span
        .with_subscriber(|(id, dispatch)| {
            dispatch
                .downcast_ref::<Registry>()
                .unwrap()
                .span(id)
                .unwrap()
                .extensions()
                .get::<ReadCount>()
                .unwrap()
                .clone()
        })
        .unwrap();
    let result = operation.instrument(span).await;
    (result, count.0.load(Ordering::Relaxed))
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
}
