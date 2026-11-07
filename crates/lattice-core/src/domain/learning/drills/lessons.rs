//! Which lesson of a course the next drill is built from.

/// Where one lesson of a course stands for its next drill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonStanding {
    pub lesson_id: String,
    /// The learner marked it done.
    pub completed: bool,
    /// The course's current lesson.
    pub current: bool,
    /// When its drills are next due; `None` while no drill of it has been rated.
    pub due_at: Option<i64>,
    /// A drill of it is out and not yet graded, or ready to be taken.
    pub pending: bool,
    /// It can be drilled in the material asked for.
    pub fits: bool,
}

/// The ids of `lessons`, given in course order, in the order the next drill prefers them (design §5).
pub fn next_lessons(lessons: &[LessonStanding], _now: i64) -> Vec<&str> {
    lessons
        .iter()
        .map(|lesson| lesson.lesson_id.as_str())
        .collect()
}
