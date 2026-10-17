//! The model a chat asked to be answered by.

use serde::{Deserialize, Serialize};

use crate::domain::models::model_ref::ModelRef;
use crate::shared::error::AppError;

/// A chat's model choice.
///
/// `Default` follows Settings live; it is not a snapshot. A chat that says
/// Default moves when Settings moves, and a replaced or deleted default never
/// strands it. What actually answered is recorded per message, not here.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatModelChoice {
    #[default]
    Default,
    /// This chat only; Settings and every other chat are untouched.
    Explicit { model: ModelRef },
}

impl ChatModelChoice {
    /// The stored form: `None` (SQL NULL) for `Default`, otherwise the chosen
    /// model as JSON. One form per choice, so `Default` is never also `null`.
    pub fn to_stored(&self) -> serde_json::Result<Option<String>> {
        match self {
            Self::Default => Ok(None),
            Self::Explicit { model } => serde_json::to_string(model).map(Some),
        }
    }

    /// Reads [`Self::to_stored`]'s form back. Anything that is not a model is
    /// an error, never `Default`: reading it as `Default` would move the chat
    /// to another model without anyone choosing that.
    pub fn from_stored(stored: Option<&str>) -> serde_json::Result<Self> {
        stored.map_or(Ok(Self::Default), |json| {
            serde_json::from_str(json).map(|model| Self::Explicit { model })
        })
    }

    /// Reads conversation `conversation_id`'s `model_choice` column. Every
    /// reader of the row reports an unreadable value the same way: an error
    /// naming the conversation, never a silent `Default`.
    pub fn from_column(conversation_id: &str, column: Option<&str>) -> Result<Self, AppError> {
        Self::from_stored(column).map_err(|e| {
            AppError::Database(format!(
                "conversation {conversation_id} has an unreadable model_choice: {e}"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn both_choices_round_trip_through_their_wire_form() {
        let explicit = ChatModelChoice::Explicit {
            model: ModelRef::Local {
                model_id: "ornith-1.5-9b".into(),
            },
        };
        for (choice, wire) in [
            (ChatModelChoice::Default, json!({"kind": "default"})),
            (
                explicit,
                json!({"kind": "explicit", "model": {"kind": "local", "model_id": "ornith-1.5-9b"}}),
            ),
        ] {
            assert_eq!(serde_json::to_value(&choice).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<ChatModelChoice>(wire).unwrap(),
                choice
            );
        }
    }

    #[test]
    fn an_explicit_choice_without_a_readable_model_is_an_error() {
        for unreadable in [
            json!({"kind": "explicit"}),
            json!({"kind": "explicit", "model": {"kind": "local"}}),
            json!({"kind": "pinned", "model": {"kind": "local", "model_id": "x"}}),
            json!(null),
        ] {
            assert!(
                serde_json::from_value::<ChatModelChoice>(unreadable.clone()).is_err(),
                "{unreadable} was read as a choice"
            );
        }
    }

    #[test]
    fn default_is_stored_as_null_and_an_explicit_choice_as_its_model() {
        assert_eq!(ChatModelChoice::Default.to_stored().unwrap(), None);
        assert_eq!(
            ChatModelChoice::from_stored(None).unwrap(),
            ChatModelChoice::Default
        );
        let explicit = ChatModelChoice::Explicit {
            model: ModelRef::Ollama {
                model: "qwen3:8b".into(),
            },
        };
        let stored = explicit.to_stored().unwrap();
        assert_eq!(
            stored.as_deref(),
            Some(r#"{"kind":"ollama","model":"qwen3:8b"}"#)
        );
        assert_eq!(
            ChatModelChoice::from_stored(stored.as_deref()).unwrap(),
            explicit
        );
    }

    #[test]
    fn a_stored_value_that_is_not_a_model_is_an_error_not_the_default() {
        for unreadable in [
            "",
            "null",
            "local:ornith",
            r#"{"kind":"local"}"#,
            r#"{"kind":"default"}"#,
        ] {
            assert!(
                ChatModelChoice::from_stored(Some(unreadable)).is_err(),
                "{unreadable:?} was read as a choice"
            );
        }
    }

    #[test]
    fn a_new_chat_follows_settings() {
        assert_eq!(ChatModelChoice::default(), ChatModelChoice::Default);
    }
}
