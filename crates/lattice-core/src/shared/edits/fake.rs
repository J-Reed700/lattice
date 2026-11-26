#![cfg(test)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

use super::kind::{Change, ContentKind, EditReactor, Loaded, Reaction};
use super::registry::EditRegistry;
use super::service::ContentEdits;
use super::types::{
    Consequence, ContentRef, EditOp, EditOrigin, EditScope, Editability, FieldProblem, FieldSpec,
    TextFormat,
};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::{Database, Reading, WriteTx};
use crate::shared::testing::sqlite::migrated_in_memory;

pub(super) const FAKE_KIND: &str = "test.note";
pub(super) const FAKE_REACTOR: &str = "test.reactor";
pub(super) const FAKE_AFFECTED: &str = "test.notes";

#[derive(Debug, Clone)]
pub(super) struct StoredNote {
    pub title: String,
    pub body: String,
    pub tags_json: String,
    pub level: i64,
    pub count: i64,
}

#[derive(Debug, Clone)]
pub(super) struct RecordedChange {
    pub edit_id: String,
    pub origin: EditOrigin,
    pub context: Option<Value>,
    pub ops: Vec<EditOp>,
    pub before: Value,
    pub after: Value,
    pub reverted_receipt: Option<Value>,
}

pub(super) fn note_form() -> Vec<FieldSpec> {
    vec![
        FieldSpec::Text {
            path: "/title".into(),
            label: "Title".into(),
            format: TextFormat::Line,
            required: true,
        },
        FieldSpec::Text {
            path: "/body".into(),
            label: "Body".into(),
            format: TextFormat::Markdown,
            required: true,
        },
        FieldSpec::List {
            path: "/tags".into(),
            label: "Tags".into(),
            item_label: "Tag".into(),
            min_items: 1,
            max_items: Some(4),
            item: vec![
                FieldSpec::Text {
                    path: "/text".into(),
                    label: "Tag".into(),
                    format: TextFormat::Plain,
                    required: true,
                },
                FieldSpec::Flag {
                    path: "/main".into(),
                    label: "Main".into(),
                    exclusive: true,
                },
            ],
        },
        FieldSpec::Choice {
            path: "/level".into(),
            label: "Level".into(),
            options: vec!["Intro".into(), "Middle".into(), "Deep".into()],
        },
        FieldSpec::Number {
            path: "/count".into(),
            label: "Count".into(),
            min: 0,
            unit: "items".into(),
        },
    ]
}

pub(super) struct FakeNoteKind {
    not_now: Mutex<Option<String>>,
    extra_check: Mutex<Vec<FieldProblem>>,
    corrupt_store: AtomicBool,
}

impl FakeNoteKind {
    pub(super) fn new() -> Self {
        Self {
            not_now: Mutex::new(None),
            extra_check: Mutex::new(Vec::new()),
            corrupt_store: AtomicBool::new(false),
        }
    }

    pub(super) fn set_not_now(&self, reason: Option<String>) {
        *self.not_now.lock().unwrap() = reason;
    }

    pub(super) fn set_extra_check(&self, problems: Vec<FieldProblem>) {
        *self.extra_check.lock().unwrap() = problems;
    }

    pub(super) fn set_corrupt_store(&self, corrupt: bool) {
        self.corrupt_store.store(corrupt, Ordering::SeqCst);
    }
}

#[async_trait]
impl ContentKind for FakeNoteKind {
    fn name(&self) -> &'static str {
        FAKE_KIND
    }

    fn affects(&self) -> &'static [&'static str] {
        &[FAKE_AFFECTED]
    }

    async fn load(&self, unit: &mut Reading<'_>, key: &str) -> Result<Option<Loaded>> {
        let row =
            sqlx::query("SELECT title, body, tags_json, level, count FROM fake_notes WHERE id = ?")
                .bind(key)
                .fetch_optional(&mut **unit)
                .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let tags: Value = serde_json::from_str(&row.get::<String, _>("tags_json"))?;
        let editability = match self.not_now.lock().unwrap().clone() {
            Some(reason) => Editability::NotNow { reason },
            None => Editability::Editable,
        };
        Ok(Some(Loaded {
            label: format!("Note: {}", row.get::<String, _>("title")),
            scope: EditScope {
                feature: "test".into(),
                container: "notebook-1".into(),
            },
            editability,
            form: note_form(),
            document: json!({
                "title": row.get::<String, _>("title"),
                "body": row.get::<String, _>("body"),
                "tags": tags,
                "level": row.get::<i64, _>("level"),
                "count": row.get::<i64, _>("count"),
            }),
            notes: vec!["Scratch note.".into()],
        }))
    }

    async fn check(
        &self,
        _unit: &mut Reading<'_>,
        _key: &str,
        _before: &Value,
        _after: &Value,
    ) -> Result<Vec<FieldProblem>> {
        Ok(self.extra_check.lock().unwrap().clone())
    }

    async fn store(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<()> {
        let title = change
            .after
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let mut body = change
            .after
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if self.corrupt_store.load(Ordering::SeqCst) {
            body.push_str(" [tampered]");
        }
        let tags_json =
            serde_json::to_string(change.after.get("tags").unwrap_or(&Value::Array(vec![])))?;
        let level = change
            .after
            .get("level")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let count = change
            .after
            .get("count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        sqlx::query(
            "UPDATE fake_notes SET title = ?, body = ?, tags_json = ?, level = ?, count = ? WHERE id = ?",
        )
        .bind(title)
        .bind(body)
        .bind(tags_json)
        .bind(level)
        .bind(count)
        .bind(&change.content.key)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

pub(super) struct FakeReactor {
    recorded: Mutex<Vec<RecordedChange>>,
    after_commits: Mutex<Vec<Value>>,
    fail_apply: Mutex<Option<String>>,
    fail_after_commit: AtomicBool,
    preview_consequences: Mutex<Vec<Consequence>>,
    follow_up_payload: Mutex<Option<Value>>,
}

impl FakeReactor {
    pub(super) fn new() -> Self {
        Self {
            recorded: Mutex::new(Vec::new()),
            after_commits: Mutex::new(Vec::new()),
            fail_apply: Mutex::new(None),
            fail_after_commit: AtomicBool::new(false),
            preview_consequences: Mutex::new(Vec::new()),
            follow_up_payload: Mutex::new(None),
        }
    }

    pub(super) fn set_fail_apply(&self, message: Option<String>) {
        *self.fail_apply.lock().unwrap() = message;
    }

    pub(super) fn set_follow_up(&self, value: Option<Value>, fail: bool) {
        *self.follow_up_payload.lock().unwrap() = value;
        self.fail_after_commit.store(fail, Ordering::SeqCst);
    }

    pub(super) fn set_preview_consequences(&self, consequences: Vec<Consequence>) {
        *self.preview_consequences.lock().unwrap() = consequences;
    }

    pub(super) fn recorded(&self) -> Vec<RecordedChange> {
        self.recorded.lock().unwrap().clone()
    }

    pub(super) fn after_commits(&self) -> Vec<Value> {
        self.after_commits.lock().unwrap().clone()
    }
}

#[async_trait]
impl EditReactor for FakeReactor {
    fn name(&self) -> &'static str {
        FAKE_REACTOR
    }

    fn kinds(&self) -> &'static [&'static str] {
        &[FAKE_KIND]
    }

    async fn preview(
        &self,
        _unit: &mut Reading<'_>,
        _change: &Change<'_>,
    ) -> Result<Vec<Consequence>> {
        Ok(self.preview_consequences.lock().unwrap().clone())
    }

    async fn apply(&self, _tx: &mut WriteTx, change: &Change<'_>) -> Result<Reaction> {
        if let Some(msg) = self.fail_apply.lock().unwrap().clone() {
            return Err(AppError::InvalidState(msg));
        }
        self.recorded.lock().unwrap().push(RecordedChange {
            edit_id: change.edit_id.to_owned(),
            origin: change.origin.clone(),
            context: change.context.cloned(),
            ops: change.ops.to_vec(),
            before: change.before.clone(),
            after: change.after.clone(),
            reverted_receipt: change.reverted_receipt.cloned(),
        });
        let receipt =
            json!({ "editId": change.edit_id, "marker": format!("r-{}", change.edit_id) });
        Ok(Reaction {
            receipt: Some(receipt),
            affected: vec!["test.reactor.derived".into()],
            consequences: self.preview_consequences.lock().unwrap().clone(),
            follow_up: self.follow_up_payload.lock().unwrap().clone(),
        })
    }

    async fn after_commit(&self, follow_up: &Value) -> Result<()> {
        self.after_commits.lock().unwrap().push(follow_up.clone());
        if self.fail_after_commit.load(Ordering::SeqCst) {
            return Err(AppError::Other("follow-up error".into()));
        }
        Ok(())
    }
}

pub(super) struct Harness {
    pub pool: SqlitePool,
    pub database: Database,
    pub kind: Arc<FakeNoteKind>,
    pub reactor: Arc<FakeReactor>,
    pub edits: ContentEdits,
}

impl Harness {
    pub(super) async fn new() -> Result<Self> {
        let pool = migrated_in_memory().await?;
        sqlx::query(
            "CREATE TABLE fake_notes (id TEXT PRIMARY KEY, title TEXT NOT NULL, body TEXT NOT NULL, tags_json TEXT NOT NULL, level INTEGER NOT NULL, count INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await?;
        let database = Database::new(pool.clone());
        let kind = Arc::new(FakeNoteKind::new());
        let reactor = Arc::new(FakeReactor::new());
        let mut registry = EditRegistry::new();
        registry.add_kind(kind.clone())?;
        registry.add_reactor(reactor.clone())?;
        let edits = ContentEdits::new(database.clone(), Arc::new(registry), "1.0.0".into());
        Ok(Self {
            pool,
            database,
            kind,
            reactor,
            edits,
        })
    }

    pub(super) async fn seed_note(&self, id: &str, title: &str, body: &str) -> Result<()> {
        let tags = json!([
            { "text": "guide", "main": true },
            { "text": "reference", "main": false }
        ]);
        sqlx::query(
            "INSERT INTO fake_notes(id, title, body, tags_json, level, count) VALUES (?, ?, ?, ?, 0, 2)",
        )
        .bind(id)
        .bind(title)
        .bind(body)
        .bind(serde_json::to_string(&tags)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn note_row(&self, id: &str) -> Result<Option<StoredNote>> {
        let row =
            sqlx::query("SELECT title, body, tags_json, level, count FROM fake_notes WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|r| StoredNote {
            title: r.get("title"),
            body: r.get("body"),
            tags_json: r.get("tags_json"),
            level: r.get("level"),
            count: r.get("count"),
        }))
    }

    pub(super) async fn counts(&self) -> Result<(i64, i64)> {
        let edits: i64 = sqlx::query_scalar("SELECT count(*) FROM content_edits")
            .fetch_one(&self.pool)
            .await?;
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM content_edit_receipts")
            .fetch_one(&self.pool)
            .await?;
        Ok((edits, receipts))
    }
}

pub(super) fn note_ref(id: &str) -> ContentRef {
    ContentRef {
        kind: FAKE_KIND.into(),
        key: id.into(),
    }
}
