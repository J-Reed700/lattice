//! The steps a transport's protocol derives from a request: a draft step, an
//! answer step, a correction turn, a larger send.

use super::GroundedRequest;
use crate::application::ports::llm_port::SamplingOverride;
use crate::application::services::grounded_generation::HistoryItem;

impl GroundedRequest {
    /// The prose step of a two-step call: `paragraph` added to the instructions, no schema.
    pub fn draft_step(&self, paragraph: &str) -> Self {
        let mut draft = self.clone();
        draft.instructions = format!("{}\n\n{paragraph}", self.instructions);
        draft.schema = None;
        draft
    }

    /// The step that answers from `draft`, continuing the conversation in which `prompt` asked for it.
    pub fn answer_step(&self, paragraph: &str, task: &str, prompt: &str, draft: &str) -> Self {
        let mut answer = self.clone();
        answer.instructions = format!("{}\n\n{paragraph}", self.instructions);
        answer.task = task.to_owned();
        answer.closing.clear();
        answer.history.extend([
            HistoryItem {
                role: "user".into(),
                content: prompt.to_owned(),
            },
            HistoryItem {
                role: "assistant".into(),
                content: draft.to_owned(),
            },
        ]);
        answer.call.sampling = Some(SamplingOverride::deterministic());
        answer
    }

    /// This request continued with the refused `answer` and `task` asking again.
    pub fn corrected(&self, answer: &str, task: &str) -> Self {
        let mut again = self.clone();
        again.task = task.to_owned();
        again.history.extend([
            HistoryItem {
                role: "user".into(),
                content: self.task.clone(),
            },
            HistoryItem {
                role: "assistant".into(),
                content: answer.to_owned(),
            },
        ]);
        again
    }

    /// This request's task sent again with `addition` after it, for a window too full for the refused answer.
    pub fn restated(&self, addition: &str) -> Self {
        let mut again = self.clone();
        again.task = format!("{}\n\n{addition}", self.task);
        again
    }

    /// This request with its answer given the room the window leaves, or kept to its reservation.
    pub fn filling_window(&self, fills: bool) -> Self {
        let mut request = self.clone();
        request.output_fills_window = fills;
        request
    }

    /// This request with its output room, the planner's default when none was set, multiplied by `factor`.
    pub fn larger(&self, factor: usize) -> Self {
        let mut larger = self.clone();
        larger.output_scale = self.output_scale.saturating_mul(factor);
        larger
    }
}
