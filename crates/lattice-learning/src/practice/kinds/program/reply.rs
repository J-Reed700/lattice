//! What a program's counterpart adds to its reply: the lines it refers to and the clarification it answers from.

use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::session::kind::ReplyGuard;
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{Brief, Exercise, PartRef, Piece};
use lattice_core::shared::error::Result;
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct ProgramReply {
    pub tools: Arc<dyn Toolchains>,
}

#[async_trait]
impl ReplyGuard for ProgramReply {
    fn rules(&self, _brief: &Brief) -> String {
        "When the learner asks a question the statement leaves open, answer it from the clarifications you are given, word for word, and set clarification to that clarification's number; when none covers it, write \"Your call: state your assumption.\" and set clarification to your_call; otherwise set clarification to none."
            .to_owned()
    }

    fn format(&self) -> String {
        "lines are the line numbers of the learner's code that you refer to, or none; path is the file those lines are in, or null; clarification is as described above"
            .to_owned()
    }

    fn fields(&self, _brief: &Brief) -> Result<Vec<(String, Value)>> {
        Ok(Vec::new())
    }

    fn parts(
        &self,
        _brief: &Brief,
        _piece: &Piece,
        _fields: &Map<String, Value>,
    ) -> Result<Vec<PartRef>> {
        Ok(Vec::new())
    }

    fn check_words(
        &self,
        _exercise: &Exercise,
        _fields: &Map<String, Value>,
        _content: &str,
    ) -> Result<()> {
        Ok(())
    }

    async fn check_unsolved(
        &self,
        _exercise: &Exercise,
        _content: &str,
        _cancel: CancellationToken,
    ) -> Result<()> {
        Ok(())
    }
}
