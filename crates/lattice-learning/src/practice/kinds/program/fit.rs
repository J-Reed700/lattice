//! When a course has program code.

use crate::dto::LearningPracticeProfileDto;
use crate::practice::session::kind::Fit;

pub struct ProgramFit;

impl Fit for ProgramFit {
    fn asked_when(&self) -> String {
        "Program code. A course asks for it when learners practise programming itself: writing programs, solving problems in code, reading code to understand or review it, or finding and fixing defects; and when working code is the course's main way of learning its subject, as in algorithms, data structures, software design or systems programming. It does not when the course is about something people do without programs: a science, a language, a craft, speaking, writing prose, presenting, negotiating or studying; nor when code appears only as an occasional illustration.".to_owned()
    }

    fn wanted_by(&self, profile: &LearningPracticeProfileDto) -> bool {
        profile.language.is_some()
    }
}
