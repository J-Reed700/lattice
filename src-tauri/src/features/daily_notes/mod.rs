//! Journal pages, atomic captures, and revision-checked editing.

mod capture;
mod capture_store;
pub mod commands;
pub mod dto;
pub mod plugin;
pub mod ports;

#[cfg(test)]
mod tests;
