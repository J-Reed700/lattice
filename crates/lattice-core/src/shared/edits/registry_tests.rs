use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::kind::{Change, ContentKind, EditReactor, Loaded, Reaction};
use super::registry::EditRegistry;
use super::types::{Consequence, FieldProblem};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::{Reading, WriteTx};

struct FakeKind(&'static str);

#[async_trait]
impl ContentKind for FakeKind {
    fn name(&self) -> &'static str {
        self.0
    }

    fn affects(&self) -> &'static [&'static str] {
        &[]
    }

    async fn load(&self, _unit: &mut Reading<'_>, _key: &str) -> Result<Option<Loaded>> {
        Ok(None)
    }

    async fn check(
        &self,
        _unit: &mut Reading<'_>,
        _key: &str,
        _before: &Value,
        _after: &Value,
    ) -> Result<Vec<FieldProblem>> {
        Ok(Vec::new())
    }

    async fn store(&self, _tx: &mut WriteTx, _change: &Change<'_>) -> Result<()> {
        Ok(())
    }
}

struct FakeReactor {
    name: &'static str,
    kinds: &'static [&'static str],
}

#[async_trait]
impl EditReactor for FakeReactor {
    fn name(&self) -> &'static str {
        self.name
    }

    fn kinds(&self) -> &'static [&'static str] {
        self.kinds
    }

    async fn preview(
        &self,
        _unit: &mut Reading<'_>,
        _change: &Change<'_>,
    ) -> Result<Vec<Consequence>> {
        Ok(Vec::new())
    }

    async fn apply(&self, _tx: &mut WriteTx, _change: &Change<'_>) -> Result<Reaction> {
        Ok(Reaction::default())
    }
}

#[test]
fn duplicate_kind_and_reactor_names_are_refused() {
    let mut registry = EditRegistry::new();
    registry.add_kind(Arc::new(FakeKind("test.note"))).unwrap();
    let second_kind = registry.add_kind(Arc::new(FakeKind("test.note")));
    assert!(matches!(second_kind, Err(AppError::InvalidState(_))));

    registry
        .add_reactor(Arc::new(FakeReactor {
            name: "test.first",
            kinds: &["test.note"],
        }))
        .unwrap();
    let second_reactor = registry.add_reactor(Arc::new(FakeReactor {
        name: "test.first",
        kinds: &["test.note"],
    }));
    assert!(matches!(second_reactor, Err(AppError::InvalidState(_))));
}

#[test]
fn reactors_for_preserves_registration_order() {
    let mut registry = EditRegistry::new();
    for (name, kinds) in [
        ("test.alpha", &["test.note"][..]),
        ("test.other", &["test.other"][..]),
        ("test.beta", &["test.note", "test.other"][..]),
        ("test.gamma", &["test.note"][..]),
    ] {
        registry
            .add_reactor(Arc::new(FakeReactor { name, kinds }))
            .unwrap();
    }

    let names: Vec<&str> = registry
        .reactors_for("test.note")
        .into_iter()
        .map(|reactor| reactor.name())
        .collect();
    assert_eq!(names, ["test.alpha", "test.beta", "test.gamma"]);
    assert_eq!(
        registry.reactor("test.beta").map(|reactor| reactor.name()),
        Some("test.beta")
    );
}
