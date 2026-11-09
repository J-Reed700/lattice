//! Stub with main's behaviour: nothing is held.
pub(crate) struct CourseSetup;

pub(crate) async fn enter(_course: &str) -> CourseSetup {
    CourseSetup
}

#[cfg(test)]
pub(crate) fn tracks(_course: &str) -> bool {
    false
}
