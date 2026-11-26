#![allow(dead_code)]
//! P0 contract stub; P1 replaces this file.

use serde_json::Value;

use super::types::{EditOp, FieldSpec};
use crate::shared::error::{AppError, Result};

pub(crate) fn apply_ops(
    _document: &Value,
    _form: &[FieldSpec],
    _ops: &[EditOp],
) -> Result<(Value, Vec<EditOp>)> {
    Err(AppError::InvalidState(
        "Content edits are not built yet.".into(),
    ))
}
