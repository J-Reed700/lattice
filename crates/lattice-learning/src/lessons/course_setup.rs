//! The one-at-a-time section of preparing a course's lessons. Lessons of one
//! course share its sources and curriculum snapshot; setting those up for a
//! lesson is held by one lesson at a time, per course, and a lesson's
//! writing, checking and publishing happen outside it.
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use tokio::sync::OwnedMutexGuard;

static COURSES: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A hold on a course's setup, released on drop. The map forgets a course
/// once nothing holds or waits for it.
pub(crate) struct CourseSetup {
    course: String,
    held: Option<OwnedMutexGuard<()>>,
}

/// Waits for the course's setup and holds it.
pub(crate) async fn enter(course: &str) -> CourseSetup {
    let lock = COURSES.lock().entry(course.to_owned()).or_default().clone();
    let mut setup = CourseSetup {
        course: course.to_owned(),
        held: None,
    };
    setup.held = Some(lock.lock_owned().await);
    setup
}

impl Drop for CourseSetup {
    fn drop(&mut self) {
        let mut courses = COURSES.lock();
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

#[cfg(test)]
pub(crate) fn tracks(course: &str) -> bool {
    COURSES.lock().contains_key(course)
}
