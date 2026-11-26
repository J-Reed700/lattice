//! Registry of editable content kinds and ordered derived-data reactors.

use std::collections::HashMap;
use std::sync::Arc;

use super::kind::{ContentKind, EditReactor};
use crate::shared::error::{AppError, Result};

/// Holds registered kinds by name and reactors in registration order.
#[derive(Default)]
pub struct EditRegistry {
    kinds: HashMap<String, Arc<dyn ContentKind>>,
    reactors: Vec<Arc<dyn EditReactor>>,
}

impl EditRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_kind(&mut self, kind: Arc<dyn ContentKind>) -> Result<()> {
        let name = kind.name();
        if self.kinds.contains_key(name) {
            return Err(AppError::InvalidState(format!(
                "Content kind `{name}` is already registered."
            )));
        }
        self.kinds.insert(name.to_owned(), kind);
        Ok(())
    }

    pub fn add_reactor(&mut self, reactor: Arc<dyn EditReactor>) -> Result<()> {
        let name = reactor.name();
        if self
            .reactors
            .iter()
            .any(|registered| registered.name() == name)
        {
            return Err(AppError::InvalidState(format!(
                "Edit reactor `{name}` is already registered."
            )));
        }
        self.reactors.push(reactor);
        Ok(())
    }

    pub fn kind(&self, name: &str) -> Option<&Arc<dyn ContentKind>> {
        self.kinds.get(name)
    }

    pub fn reactors_for(&self, kind: &str) -> Vec<&Arc<dyn EditReactor>> {
        self.reactors
            .iter()
            .filter(|reactor| reactor.kinds().contains(&kind))
            .collect()
    }

    pub fn reactor(&self, name: &str) -> Option<&Arc<dyn EditReactor>> {
        self.reactors.iter().find(|reactor| reactor.name() == name)
    }
}
