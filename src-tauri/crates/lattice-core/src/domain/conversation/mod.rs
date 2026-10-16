//! Conversation aggregates, durable memory rules and a chat's model choice.

mod aggregate;
pub mod memory;
mod model_choice;

pub use aggregate::*;
pub use model_choice::ChatModelChoice;
