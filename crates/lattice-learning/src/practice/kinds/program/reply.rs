//! What a program's counterpart adds to its reply.

use crate::practice::session::kind::ReplyGuard;
use lattice_core::domain::learning::practice::work::Brief;

pub struct ProgramReply;

impl ReplyGuard for ProgramReply {
    fn rules(&self, _brief: &Brief) -> String {
        String::new()
    }

    fn format(&self) -> String {
        String::new()
    }
}
