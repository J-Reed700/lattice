//! Stub with main's behaviour: nothing is held.
#[derive(Clone, Default)]
pub struct CourseSetups;

pub(crate) struct CourseSetup;

impl CourseSetups {
    pub(crate) async fn enter(&self, _course: &str) -> CourseSetup {
        CourseSetup
    }

    #[cfg(test)]
    pub(crate) fn tracks(&self, _course: &str) -> bool {
        false
    }
}
