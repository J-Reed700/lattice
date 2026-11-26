#![allow(dead_code)]
//! P0 contract stub; P1 replaces this file.

use serde_json::Value;

use super::types::{FieldProblem, FieldSpec};

pub(crate) fn form_problems(_form: &[FieldSpec], _document: &Value) -> Vec<FieldProblem> {
    Vec::new()
}
