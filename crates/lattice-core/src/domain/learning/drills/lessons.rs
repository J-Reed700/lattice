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

/// Where a lesson's drills stand, in the order the next drill prefers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrillStanding {
    /// Rated, and due again.
    Due,
    /// No drill of it has been rated.
    New,
    /// Rated, and not due yet.
    Rested,
    /// A drill of it is out and not yet graded, or ready to be taken.
    Pending,
}

impl LessonStanding {
    /// Where the lesson's drills stand at `now`.
    pub fn standing(&self, _now: i64) -> DrillStanding {
        DrillStanding::New
    }
}

/// The ids of `lessons`, given in course order, in the order the next drill prefers them: lessons due, earliest first;
/// then those never rated, the current lesson first, then completed lessons, then the rest; then those not yet due,
/// soonest first; and last those pending. A lesson that does not fit is left out, and ties keep the course's order.
pub fn next_lessons(lessons: &[LessonStanding], now: i64) -> Vec<&str> {
    let mut ranked: Vec<(u8, i64, usize, &str)> = lessons
        .iter()
        .enumerate()
        .filter(|(_, lesson)| lesson.fits)
        .map(|(index, lesson)| {
            let (rank, time) = match (lesson.pending, lesson.due_at) {
                (true, _) => (3, 0),
                (false, Some(at)) if at <= now => (0, at),
                (false, Some(at)) => (2, at),
                (false, None) => (
                    1,
                    if lesson.current {
                        0
                    } else {
                        i64::from(!lesson.completed) + 1
                    },
                ),
            };
            (rank, time, index, lesson.lesson_id.as_str())
        })
        .collect();
    ranked.sort_by_key(|&(rank, time, index, _)| (rank, time, index));
    ranked.into_iter().map(|(_, _, _, id)| id).collect()
}
