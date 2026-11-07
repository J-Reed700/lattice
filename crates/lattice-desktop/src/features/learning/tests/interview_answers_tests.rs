//! Stands in for `lattice_learning::practice::interview_answers::tests`, which declared this eval.

use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_learning::dto::LearningSourceVersionDto;
use lattice_learning::practice::interview_answers::interview_evidence::EvidenceBank;
use lattice_learning::practice::interview_answers::*;
use lattice_learning::practice::practical_generation::invalid;
use serde_json::{json, Value};

#[path = "../practice/interview_answer_live_tests.rs"]
mod live_tests;
