//! What a program's counterpart adds to its reply: the lines it refers to and the clarification it answers from.

use crate::practice::coding::problem::SOLUTION_PATH;
use crate::practice::coding::solution_check::{code_blocks, passes_every_case};
use crate::practice::kinds::program::parts::part_of;
use crate::practice::kinds::program::shape::{files_of, problem_of, spec_of};
use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::session::counterpart::fields_as;
use crate::practice::session::kind::ReplyGuard;
use async_trait::async_trait;
use lattice_core::domain::learning::practice::problem_patterns::named_in;
use lattice_core::domain::learning::practice::work::{Brief, Exercise, PartRef, Piece};
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// What a reply says when it answers with the problem's own "your call" line.
const YOUR_CALL: &str = "Your call: state your assumption";

pub struct ProgramReply {
    pub tools: Arc<dyn Toolchains>,
}

/// The reply fields a program adds.
#[derive(Deserialize)]
struct Fields {
    #[serde(default)]
    lines: Vec<u32>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    clarification: Option<String>,
}

fn refuse(reason: impl Into<String>) -> AppError {
    AppError::InvalidInput(reason.into())
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

    fn fields(&self, brief: &Brief) -> Result<Vec<(String, Value)>> {
        let mut options: Vec<String> = (0..spec_of(brief)?.clarifications.len())
            .map(|index| index.to_string())
            .collect();
        options.push("your_call".to_owned());
        options.push("none".to_owned());
        Ok(vec![
            (
                "lines".to_owned(),
                json!({"type": "array", "items": {"type": "integer"}}),
            ),
            ("path".to_owned(), json!({"type": ["string", "null"]})),
            (
                "clarification".to_owned(),
                json!({"type": "string", "enum": options}),
            ),
        ])
    }

    fn parts(
        &self,
        _brief: &Brief,
        piece: &Piece,
        fields: &Map<String, Value>,
    ) -> Result<Vec<PartRef>> {
        let fields: Fields = fields_as(fields)?;
        if fields.lines.is_empty() {
            return Ok(Vec::new());
        }
        let path = fields.path.unwrap_or_else(|| SOLUTION_PATH.to_owned());
        let count = files_of(piece)?
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.content.lines().count())
            .ok_or_else(|| refuse(format!("{path} is not one of the learner's files.")))?;
        for line in &fields.lines {
            let exists = *line >= 1 && usize::try_from(*line).is_ok_and(|line| line <= count);
            if !exists {
                return Err(refuse(format!(
                    "Line {line} is not in {path}, which has {count} lines. Refer only to lines that exist."
                )));
            }
        }
        Ok(fields
            .lines
            .iter()
            .map(|line| part_of(&path, *line))
            .collect())
    }

    fn check_words(
        &self,
        exercise: &Exercise,
        fields: &Map<String, Value>,
        content: &str,
    ) -> Result<()> {
        let spec = spec_of(&exercise.brief)?;
        if let Some(pattern) = named_in(content, &spec.patterns) {
            return Err(refuse(format!(
                "The reply names the technique \"{}\". Do not name it; ask about the idea instead.",
                pattern.name
            )));
        }
        let fields: Fields = fields_as(fields)?;
        match fields.clarification.as_deref() {
            None | Some("none") => {}
            Some("your_call") => {
                if !content.contains(YOUR_CALL) {
                    return Err(refuse(format!(
                        "With clarification your_call the reply must contain \"{YOUR_CALL}\"."
                    )));
                }
            }
            Some(index) => {
                let clarification = index
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| spec.clarifications.get(index));
                match clarification {
                    None => {
                        return Err(refuse(format!(
                            "{index} is not a clarification of this problem. Use a clarification's number, your_call or none."
                        )))
                    }
                    Some(clarification) if !content.contains(&clarification.answer) => {
                        return Err(refuse(format!(
                            "The reply must contain the answer to clarification {index} word for word: {}",
                            clarification.answer
                        )))
                    }
                    Some(_) => {}
                }
            }
        }
        Ok(())
    }

    async fn check_unsolved(
        &self,
        exercise: &Exercise,
        content: &str,
        cancel: CancellationToken,
    ) -> Result<()> {
        let blocks = code_blocks(content);
        if blocks.is_empty() {
            return Ok(());
        }
        let problem = problem_of(exercise)?;
        let runner = self.tools.runner(&problem.spec.language)?;
        let harness = self.tools.harness(&problem.spec.language)?;
        for block in blocks {
            let solves = passes_every_case(
                runner.as_ref(),
                harness.as_ref(),
                &problem,
                &block,
                cancel.clone(),
            )
            .await?;
            if solves {
                return Err(refuse(
                    "A code block in the reply passes every case, so it solves the problem. Do not write a working solution; describe the step in words, or show a fragment that leaves the problem unsolved.",
                ));
            }
        }
        Ok(())
    }
}
