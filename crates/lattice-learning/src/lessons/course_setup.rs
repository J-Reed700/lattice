//! The one-at-a-time section of preparing a course's lessons. Lessons of one
//! course share its sources and curriculum snapshot; setting those up for a
//! lesson is held by one lesson at a time, per course, and a lesson's
//! writing, checking and publishing happen outside it.
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::OwnedMutexGuard;

type Courses = Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

/// Each course's setup lock. Clones share the locks.
#[derive(Clone, Default)]
pub struct CourseSetups(Courses);

/// A hold on a course's setup, released on drop. The map forgets a course
/// once nothing holds or waits for it.
pub(crate) struct CourseSetup {
    courses: Courses,
    course: String,
    held: Option<OwnedMutexGuard<()>>,
}

impl CourseSetups {
    /// Waits for the course's setup and holds it.
    pub(crate) async fn enter(&self, course: &str) -> CourseSetup {
        let lock = self.0.lock().entry(course.to_owned()).or_default().clone();
        let mut setup = CourseSetup {
            courses: self.0.clone(),
            course: course.to_owned(),
            held: None,
        };
        setup.held = Some(lock.lock_owned().await);
        setup
    }

    #[cfg(test)]
    pub(crate) fn tracks(&self, course: &str) -> bool {
        self.0.lock().contains_key(course)
    }
}

impl Drop for CourseSetup {
    fn drop(&mut self) {
        let mut courses = self.courses.lock();
        drop(self.held.take());
        // A count of one is the map: no one else holds or waits for the lock.
        if courses
            .get(&self.course)
            .is_some_and(|lock| Arc::strong_count(lock) == 1)
        {
            courses.remove(&self.course);
        }
    }
}
