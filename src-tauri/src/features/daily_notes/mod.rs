//! Journal pages, atomic captures, and revision-checked editing.

mod capture;
pub mod commands;
pub mod dto;
pub mod plugin;
pub mod repository;

#[cfg(test)]
mod tests;
