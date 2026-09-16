//! # Question Answering (Q&A) Use Cases
//!
//! Use cases for Retrieval-Augmented Generation (RAG) based question answering.
//!
//! This module provides:
//! - **Ask Question**: RAG-based question answering with source citations

pub mod ask_question;

pub use ask_question::AskQuestionUseCase;
