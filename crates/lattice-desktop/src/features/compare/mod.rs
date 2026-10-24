//! Multi-document comparison table generation with citations.

pub mod commands;
pub mod di;
pub mod dto;
pub mod parser;
pub mod plugin;
pub mod prompt;
pub mod retrieval;
pub mod scoped_search;
pub mod use_case;

#[cfg(test)]
mod tests;
