//! A program's session as the debrief's model reads and cites it.

use crate::practice::session::kind::{Account, Citation};
use crate::practice::session::record::SessionCheck;
use lattice_core::domain::learning::practice::work::{Exercise, Form, PartRef, Piece};
use lattice_core::shared::error::Result;
use serde_json::{Map, Value};

pub struct ProgramAccount;

impl Account for ProgramAccount {
    fn exercise_key(&self) -> String {
        String::new()
    }

    fn exercise(&self, _exercise: &Exercise) -> Result<Value> {
        Ok(Value::Null)
    }

    fn form_key(&self) -> String {
        String::new()
    }

    fn form(&self, _form: Form) -> Result<Value> {
        Ok(Value::Null)
    }

    fn checks_key(&self) -> String {
        String::new()
    }

    fn check(
        &self,
        _exercise: &Exercise,
        _check: &SessionCheck,
        _before: Option<&SessionCheck>,
    ) -> Result<Value> {
        Ok(Value::Null)
    }

    fn piece(&self, _exercise: &Exercise, _piece: &Piece) -> Result<Value> {
        Ok(Value::Null)
    }

    fn cites_parts(&self) -> Citation {
        Citation {
            tag: String::new(),
            sentence: String::new(),
        }
    }

    fn cites_check(&self) -> Citation {
        Citation {
            tag: String::new(),
            sentence: String::new(),
        }
    }

    fn fields(&self) -> Vec<(String, Value)> {
        Vec::new()
    }

    fn leave_empty(&self) -> String {
        String::new()
    }

    fn parts(
        &self,
        _exercise: &Exercise,
        _check: &SessionCheck,
        _fields: &Map<String, Value>,
    ) -> Result<Vec<PartRef>> {
        Ok(Vec::new())
    }
}
