//! Stands in for `lattice_learning::references::course_index`, which declared these tests.

use lattice_learning::references::course_index::sync;
use lattice_learning::references::course_index::{
    CourseIndexStatusDto, CourseIndexes, IndexMode, COURSE_INDEX,
};

#[path = "../references/course_index/first_tests.rs"]
mod first_tests;
#[path = "../references/course_index/status_tests.rs"]
mod status_tests;
#[path = "../references/course_index/stopped_tests.rs"]
mod stopped_tests;
#[path = "../references/course_index/tests.rs"]
pub(super) mod tests;
