//! What a program's counterpart adds to its reply: the lines it refers to and the clarification it answers from.

use crate::practice::session::kind::ReplyGuard;
use lattice_core::domain::learning::practice::work::Brief;

pub struct ProgramReply;

impl ReplyGuard for ProgramReply {
    fn rules(&self, _brief: &Brief) -> String {
        "When the learner asks a question the statement leaves open, answer it from the clarifications you are given, word for word, and set clarification to that clarification's number; when none covers it, write \"Your call: state your assumption.\" and set clarification to your_call; otherwise set clarification to none."
            .to_owned()
    }

    fn format(&self) -> String {
        "lines are the line numbers of the learner's code that you refer to, or none; path is the file those lines are in, or null; clarification is as described above"
            .to_owned()
    }
}
