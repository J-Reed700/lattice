//! Stage 1: the clean material and whatever its proofs run on later. What it is, and how it is written, is the
//! material's.

use super::outputs::CleanCode;
use super::DrillBuild;
use crate::drills::spec::DrillSpec;
use lattice_core::shared::error::Result;

impl DrillBuild<'_> {
    /// Stage 1. The clean files and, where the material has one, what its proofs run against.
    pub(crate) async fn clean_code(
        &self,
        spec: &DrillSpec,
    ) -> Result<CleanCode> {
        self.material.clean(spec, &self.context()).await
    }
}
