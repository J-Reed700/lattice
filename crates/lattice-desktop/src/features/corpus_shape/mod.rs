//! Document clustering over mean-pooled chunk embeddings with stability-fingerprinted labeling.

pub mod clustering;
pub mod commands;
pub mod entity;
pub mod fingerprint;
pub mod labeling;
pub mod plugin;
pub mod repository;
pub mod use_cases;

#[cfg(test)]
mod tests;
