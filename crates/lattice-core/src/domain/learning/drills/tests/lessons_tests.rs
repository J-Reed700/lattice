//! Which lesson of a course the next drill is built from: the ranks and their ties.

use super::*;

fn stand(id: &str) -> LessonStanding {
    LessonStanding {
        lesson_id: id.to_owned(),
        completed: false,
        current: false,
        due_at: None,
        pending: false,
        fits: true,
    }
}

fn due_at(id: &str, at: i64) -> LessonStanding {
    LessonStanding {
        due_at: Some(at),
        ..stand(id)
    }
}

fn pending(id: &str, due: Option<i64>) -> LessonStanding {
    LessonStanding {
        due_at: due,
        pending: true,
        ..stand(id)
    }
}

#[test]
fn a_lesson_that_does_not_fit_is_left_out() {
    let course = [
        LessonStanding {
            fits: false,
            ..stand("a")
        },
        stand("b"),
    ];
    assert_eq!(next_lessons(&course, 10), ["b"]);
}

#[test]
fn due_lessons_come_before_new_ones_rested_ones_and_pending_ones() {
    let course = [
        pending("p", Some(1)),
        due_at("r", 100),
        stand("n"),
        due_at("d", 5),
    ];
    assert_eq!(next_lessons(&course, 10), ["d", "n", "r", "p"]);
}

#[test]
fn due_lessons_are_taken_earliest_first() {
    let course = [due_at("a", 8), due_at("b", 3), due_at("c", 5)];
    assert_eq!(next_lessons(&course, 10), ["b", "c", "a"]);
}

#[test]
fn a_lesson_due_exactly_now_is_due() {
    let course = [stand("n"), due_at("d", 10)];
    assert_eq!(next_lessons(&course, 10), ["d", "n"]);
}

#[test]
fn a_lesson_due_a_moment_from_now_is_rested() {
    let course = [due_at("r", 11), stand("n")];
    assert_eq!(next_lessons(&course, 10), ["n", "r"]);
}

#[test]
fn never_rated_lessons_go_current_first_then_completed_then_the_rest() {
    let course = [
        stand("x"),
        LessonStanding {
            completed: true,
            ..stand("y")
        },
        LessonStanding {
            current: true,
            ..stand("z")
        },
        LessonStanding {
            completed: true,
            ..stand("w")
        },
        stand("v"),
    ];
    assert_eq!(next_lessons(&course, 10), ["z", "y", "w", "x", "v"]);
}

#[test]
fn a_lesson_both_current_and_completed_is_listed_once_and_first() {
    let course = [
        stand("x"),
        LessonStanding {
            completed: true,
            current: true,
            ..stand("y")
        },
    ];
    assert_eq!(next_lessons(&course, 10), ["y", "x"]);
}

#[test]
fn rested_lessons_are_taken_soonest_due_first() {
    let course = [due_at("a", 50), due_at("b", 20), due_at("c", 30)];
    assert_eq!(next_lessons(&course, 10), ["b", "c", "a"]);
}

#[test]
fn pending_lessons_go_last_in_course_order_whatever_their_due_date() {
    let course = [
        pending("p1", Some(100)),
        stand("f"),
        pending("p2", Some(1)),
        pending("p3", None),
    ];
    assert_eq!(next_lessons(&course, 10), ["f", "p1", "p2", "p3"]);
}

#[test]
fn lessons_of_one_rank_and_time_keep_the_courses_order() {
    let course = [due_at("r", 50), due_at("b", 5), due_at("a", 5)];
    assert_eq!(next_lessons(&course, 10), ["b", "a", "r"]);
}

#[test]
fn no_lesson_that_fits_gives_nothing() {
    let course = [LessonStanding {
        fits: false,
        ..stand("a")
    }];
    assert!(next_lessons(&course, 10).is_empty());
}

#[test]
fn an_empty_course_gives_nothing() {
    assert!(next_lessons(&[], 10).is_empty());
}

#[test]
fn a_lesson_s_standing_is_pending_new_due_or_rested() {
    for fits in [true, false] {
        let with = |lesson: LessonStanding| LessonStanding { fits, ..lesson };
        assert_eq!(
            with(pending("a", Some(1))).standing(10),
            DrillStanding::Pending
        );
        assert_eq!(with(stand("a")).standing(10), DrillStanding::New);
        assert_eq!(with(due_at("a", 10)).standing(10), DrillStanding::Due);
        assert_eq!(with(due_at("a", 5)).standing(10), DrillStanding::Due);
        assert_eq!(with(due_at("a", 11)).standing(10), DrillStanding::Rested);
    }
}
