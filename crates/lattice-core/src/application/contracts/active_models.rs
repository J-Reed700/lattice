//! What a chat on the Settings default and utility work run on, as the UI
//! reads it.

use crate::domain::models::model_ref::ModelRef;
use serde::{Deserialize, Serialize};

/// What a chat on the Settings default and utility work run on now, worked
/// out by the functions the model loader uses. Nothing is probed.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ActiveModels {
    /// The models a chat on the Settings default tries, in order. Empty when none is set.
    pub chat: Vec<ModelRef>,
    /// Why `chat` is empty, in the words the refusal uses.
    pub chat_unset: Option<String>,
    /// The models utility work tries, in order. Empty when none is set.
    pub utility: Vec<ModelRef>,
    /// The model the llama.cpp connection serves, when Settings names its server URL and model.
    pub llama_cpp: Option<ModelRef>,
    /// The chat model the Ollama connection serves, when the connection is saved and names a chat tag.
    pub ollama: Option<ModelRef>,
    /// The tag utility work asks the Ollama connection for: its utility tag, else its chat tag.
    pub ollama_utility_tag: Option<String>,
}
