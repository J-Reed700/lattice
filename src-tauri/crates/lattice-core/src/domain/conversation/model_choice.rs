//! The model a chat asked to be answered by.

use serde::{Deserialize, Serialize};

use crate::domain::models::model_ref::ModelRef;

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
    fn a_new_chat_follows_settings() {
        assert_eq!(ChatModelChoice::default(), ChatModelChoice::Default);
    }
}
