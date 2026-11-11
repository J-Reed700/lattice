//! What a learner may send as one turn of a practice session.

use lattice_core::shared::constants::MAX_CHAT_MESSAGE_CHARS;
use lattice_core::shared::error::{AppError, Result};

/// The bound a chat message has: one to `MAX_CHAT_MESSAGE_CHARS` characters, not all space.
pub fn validate_learner_turn(content: &str) -> Result<()> {
    if content.trim().is_empty() || content.chars().count() > MAX_CHAT_MESSAGE_CHARS {
        return Err(AppError::InvalidInput(format!(
            "Simulation turns must contain 1–{MAX_CHAT_MESSAGE_CHARS} characters."
        )));
    }
    Ok(())
}
