//! What the Program kind tells the fit check, and when a profile asks for it outright.

use crate::dto::LearningPracticeProfileDto;
use crate::practice::coding::proof::fakes::{Behaviour, FakeRunner};
use crate::practice::kinds::program::kind;
use crate::practice::kinds::program::toolchain::FakeToolchains;
use crate::practice::session::kind::Kind;
use std::sync::Arc;

fn program_kind() -> Kind {
    kind(Arc::new(FakeToolchains::new(FakeRunner::new(|_, _| {
        Behaviour::Fails
    }))))
}

fn profile(language: Option<&str>) -> LearningPracticeProfileDto {
    LearningPracticeProfileDto {
        program_id: "course".to_owned(),
        details: "Preparing for a backend interview".to_owned(),
        language: language.map(str::to_owned),
        interview_at: Some(1),
        company: Some("a company".to_owned()),
        role: Some("Backend engineer".to_owned()),
        revision: 1,
    }
}

#[test]
fn the_fit_check_is_told_what_program_code_is_and_when_a_course_asks_for_it() {
    assert_eq!(
        program_kind().fit.asked_when(),
        "Program code. A course asks for it when learners practise programming itself: writing programs, solving problems in code, reading code to understand or review it, or finding and fixing defects; and when working code is the course's main way of learning its subject, as in algorithms, data structures, software design or systems programming. It does not when the course is about something people do without programs: a science, a language, a craft, speaking, writing prose, presenting, negotiating or studying; nor when code appears only as an occasional illustration."
    );
}

#[test]
fn a_profile_asks_for_program_code_outright_when_it_has_a_language() {
    let fit = program_kind().fit;
    assert!(fit.wanted_by(&profile(Some("python"))));
    assert!(!fit.wanted_by(&profile(None)));
}
