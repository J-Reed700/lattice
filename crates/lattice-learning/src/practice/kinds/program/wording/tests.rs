#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::dto::CodingRunScope;
use crate::practice::coding::proof::fakes::spec;
use crate::practice::kinds::program::shape::brief_of;
use lattice_core::domain::learning::practice::work::{Form, Rung, Stage};

#[test]
fn the_card_is_code_and_a_check_is_a_run() {
    assert_eq!(ProgramWording.card(), "Code");
    assert_eq!(ProgramWording.check_noun(), "run");
}

#[test]
fn a_form_is_worded_as_the_task_it_was() {
    for (form, task) in [(Form::Build, Form::Build), (Form::Repair, Form::Repair)] {
        assert_eq!(ProgramWording.form_label(form), format!("{task:?}"));
    }
}

#[test]
fn a_form_a_program_lacks_keeps_its_own_name() {
    assert_eq!(ProgramWording.form_label(Form::Complete), "Complete");
    assert_eq!(ProgramWording.form_label(Form::Arrange), "Arrange");
}

#[test]
fn a_stage_is_worded_as_the_stage_it_was() {
    let brief = brief_of(&spec()).unwrap();
    let stages = [
        (Stage::Understand, Stage::Understand),
        (Stage::Examples, Stage::Examples),
        (Stage::Plan, Stage::Plan),
        (Stage::Make, Stage::Make),
        (Stage::Check, Stage::Check),
        (Stage::Review, Stage::Review),
        (Stage::Reproduce, Stage::Reproduce),
        (Stage::Locate, Stage::Locate),
        (Stage::Fix, Stage::Fix),
        (Stage::Verify, Stage::Verify),
    ];
    for (stage, old) in stages {
        for form in [Form::Build, Form::Repair] {
            assert_eq!(
                ProgramWording.stage_label(&brief, form, stage),
                format!("{old:?}")
            );
        }
    }
}

#[test]
fn a_scope_is_worded_as_the_scope_it_was() {
    for (scope, old) in [
        (CheckScope::Shown, CodingRunScope::Shown),
        (CheckScope::All, CodingRunScope::All),
        (CheckScope::Own, CodingRunScope::Input),
    ] {
        assert_eq!(ProgramWording.scope_label(scope), format!("{old:?}"));
    }
}

#[test]
fn a_rung_is_worded_as_the_rung_it_was() {
    for (rung, old) in [
        (Rung::Nudge, Rung::Nudge),
        (Rung::Idea, Rung::Idea),
        (Rung::Plan, Rung::Plan),
        (Rung::Point, Rung::Point),
    ] {
        assert_eq!(ProgramWording.rung_label(rung), format!("{old:?}"));
    }
}
