//! The run recipe: one JSON file that says everything a run depends on, so
//! the same command reproduces the same configuration after other work lands.
//!
//! Settings start from `SettingsDto::default()` — what a fresh desktop install
//! ships — and the recipe's `settings` object is merged over them. Only the
//! wiring the harness owns (provider, server URL, model name) is set here; an
//! override for a field that does not exist is an error rather than a silent
//! no-op, so a renamed setting cannot quietly drop out of a recipe.

use crate::Res;
use lattice::features::conversation::chat::ToolPreferences;
use lattice_core::application::contracts::settings::SettingsDto;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Recipe {
    pub name: String,
    /// Dataset whose `documents` become the vault.
    pub corpus: String,
    /// A desktop-path question set (`items`) or a dataset (`queries`).
    pub questions: String,
    pub embedding_model_dir: String,
    pub embedding_model_id: String,
    pub server: Server,
    /// A model on the recipe's server to hold the utility role, as Settings ->
    /// Model roles assigns it to a server-hosted model. The corpus planner and
    /// the corrective retry run only when a utility model is set
    /// (`chat/retrieval/kb_retrieval.rs`); without one, a run measures a
    /// desktop whose roles are at their install defaults.
    #[serde(default)]
    pub utility_role: Option<UtilityRole>,
    #[serde(default)]
    pub index: Index,
    /// What the composer sends with a turn; the desktop default when absent.
    #[serde(default)]
    pub tool_preferences: Option<Value>,
    /// Merged over the shipped defaults.
    #[serde(default)]
    pub settings: Value,
    #[serde(default)]
    pub wait: Wait,
}

#[derive(Debug, Deserialize)]
pub struct Server {
    pub url: String,
    /// The model name sent in each request.
    pub model: String,
    /// The weights file, hashed at start so a run names what it measured.
    pub gguf: String,
    pub gguf_sha256: String,
    /// The command the server was started with, kept with the run.
    pub launch: String,
    pub expected_ctx: u64,
    pub expected_slots: u64,
    /// `"off"`: add `chat_template_kwargs.enable_thinking: false` to every
    /// request that does not choose for itself. `"as-sent"`: forward
    /// requests untouched.
    pub thinking: String,
}

#[derive(Debug, Deserialize)]
pub struct UtilityRole {
    /// The catalog id the role is recorded under.
    pub model_id: String,
    /// The name the server answers to, sent with each utility request.
    pub served_as: String,
}

#[derive(Debug, Deserialize)]
pub struct Index {
    pub chunk_tokens: usize,
}

impl Default for Index {
    /// The batch importer's chunking, which is what a desktop import runs.
    fn default() -> Self {
        Self { chunk_tokens: 800 }
    }
}

#[derive(Debug, Deserialize)]
pub struct Wait {
    /// How long to wait for the grounding check after an answer returns.
    pub verification_secs: u64,
    /// Silence on the wire that counts as the turn's background work done.
    pub quiet_ms: u64,
    pub quiet_cap_secs: u64,
}

impl Default for Wait {
    fn default() -> Self {
        Self {
            verification_secs: 300,
            quiet_ms: 3_000,
            quiet_cap_secs: 600,
        }
    }
}

impl Recipe {
    pub fn load(path: &Path) -> Res<Self> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    pub fn thinking_off(&self) -> Res<bool> {
        match self.server.thinking.as_str() {
            "off" => Ok(true),
            "as-sent" => Ok(false),
            other => {
                Err(format!("server.thinking must be \"off\" or \"as-sent\", not {other:?}").into())
            }
        }
    }

    /// The desktop composer's defaults (`useComposer.ts`): no switch forced
    /// on, so the intent step decides whether the vault is searched.
    pub fn tool_preferences(&self) -> Res<ToolPreferences> {
        let value = self.tool_preferences.clone().unwrap_or_else(|| {
            serde_json::json!({
                "knowledgeBase": false,
                "webSearch": false,
                "deepResearchMode": false,
                "followupMode": false,
                "turnMode": "auto",
                "enabledTools": []
            })
        });
        Ok(serde_json::from_value(value)?)
    }

    /// The shipped defaults, the recipe's overrides, and the harness wiring.
    pub fn settings(&self, llama_url: &str) -> Res<SettingsDto> {
        let mut merged = serde_json::to_value(SettingsDto::default())?;
        merge(&mut merged, &self.settings);
        // A server-hosted utility model is reached by the name in
        // `ollamaUtilityModel`, whichever remote provider is selected.
        let utility = self
            .utility_role
            .as_ref()
            .map_or(String::new(), |role| role.served_as.clone());
        let wiring = serde_json::json!({
            "llm": {
                "provider": "llamacpp",
                "llamaCpp": {"url": llama_url, "model": self.server.model},
                "ollamaUtilityModel": utility
            }
        });
        merge(&mut merged, &wiring);
        let settings: SettingsDto = serde_json::from_value(merged)?;
        let round_trip = serde_json::to_value(&settings)?;
        let mut missing = Vec::new();
        unknown_paths(&self.settings, &round_trip, String::new(), &mut missing);
        if !missing.is_empty() {
            return Err(
                format!("recipe settings name fields that do not exist: {missing:?}").into(),
            );
        }
        Ok(settings)
    }
}

/// Objects merge key by key; anything else replaces.
pub fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                match base.get_mut(key) {
                    Some(slot) => merge(slot, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

fn unknown_paths(patch: &Value, settled: &Value, prefix: String, out: &mut Vec<String>) {
    let Value::Object(patch) = patch else {
        return;
    };
    for (key, value) in patch {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match settled.get(key) {
            Some(next) => unknown_paths(value, next, path, out),
            None => out.push(path),
        }
    }
}

/// A recipe path: `~/` is the invoking user's home (read before the run
/// isolates its own), anything else is relative to the repository root.
pub fn resolve(raw: &str, repo_root: &Path, home: &Path) -> PathBuf {
    match raw.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => {
            let path = PathBuf::from(raw);
            if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            }
        }
    }
}

/// One question to put to the app: one or more turns of one conversation.
#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,
    pub tags: Vec<String>,
    pub turns: Vec<String>,
}

/// Reads a desktop-path question set, or a dataset's queries as one-turn
/// items so an existing labelled set runs through the same path.
pub fn load_items(path: &Path) -> Res<Vec<Item>> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let strings = |v: Option<&Value>| -> Vec<String> {
        v.and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let mut items = Vec::new();
    if let Some(list) = value.get("items").and_then(Value::as_array) {
        for item in list {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .ok_or("item without id")?;
            let turns: Vec<String> = item
                .get("turns")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|turn| turn.get("ask").and_then(Value::as_str))
                .map(str::to_owned)
                .collect();
            if turns.is_empty() {
                return Err(format!("item {id} has no turns").into());
            }
            items.push(Item {
                id: id.to_owned(),
                tags: strings(item.get("tags")),
                turns,
            });
        }
    } else if let Some(list) = value.get("queries").and_then(Value::as_array) {
        for query in list {
            let id = query
                .get("id")
                .and_then(Value::as_str)
                .ok_or("query without id")?;
            let text = query
                .get("text")
                .and_then(Value::as_str)
                .ok_or("query without text")?;
            items.push(Item {
                id: id.to_owned(),
                tags: strings(query.get("dimensions")),
                turns: vec![text.to_owned()],
            });
        }
    } else {
        return Err("questions file has neither `items` nor `queries`".into());
    }
    Ok(items)
}
