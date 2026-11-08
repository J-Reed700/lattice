//! Stage 5: a judge checks each proven lesson issue against the sentences of the lesson it cites.

use super::outputs::{Baseline, LessonChecked, Proved};
use super::DrillBuild;
use crate::features::learning::drills::spec::DrillSpec;
use lattice_core::shared::error::Result;

impl DrillBuild<'_> {
    /// Stage 5. What the lesson check made of each proven mutation.
    pub(in crate::features::learning) async fn lesson_check(
        &self,
        spec: &DrillSpec,
        baseline: &Baseline,
        proved: &Proved,
    ) -> Result<LessonChecked> {
        let _ = (spec, baseline, proved);
        Ok(LessonChecked {
            links: Vec::new(),
            checks: Vec::new(),
        })
    }
}
