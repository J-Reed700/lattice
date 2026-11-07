//! Stage 2: an independent review repairs the clean material until it has no issue of its own. The repaired material
//! is the clean baseline every later stage compares against.

use super::outputs::{CleanCode, CleanReview};
use super::DrillBuild;
use crate::features::learning::drills::spec::DrillSpec;
use lattice_core::shared::error::Result;

impl DrillBuild<'_> {
    /// Stage 2. The repairs the review found, and the material with them made.
    pub(in crate::features::learning) async fn clean_review(
        &self,
        spec: &DrillSpec,
        code: &CleanCode,
    ) -> Result<CleanReview> {
        self.material
            .clean_review(spec, code, &self.context())
            .await
    }
}
