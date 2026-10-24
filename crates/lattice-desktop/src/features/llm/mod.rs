//! Local and remote LLM inference, model catalog, and scheduling.

pub(crate) mod activity;
pub mod commands;
pub mod di;
pub mod dto;
pub(crate) mod instances;
pub(crate) mod loading;
pub mod use_cases;

pub mod cloud;
pub mod engine;
pub mod llama_cpp;
pub(crate) mod scheduler;
