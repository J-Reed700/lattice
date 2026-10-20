//! The course that goes first: the one open in its workspace, and any course
//! a lesson is about to read.
//!
//! Either claims its index as awaited in the passage index's turn order,
//! which every folder index shares: while it embeds, every other index's
//! build holds at its next batch and carries on after it. Its background
//! build also takes this kind's one slot ahead of other courses' builds,
//! which give up their turn and queue again behind it, as Explorer's builds
//! do for its open folder.

use super::super::COURSE_INDEX;
use super::CourseIndexes;
use crate::infrastructure::passage_index::{Claim, Priority};
use lattice_core::shared::Result;
use tracing::warn;

impl CourseIndexes {
    /// `program` is open in its workspace, in place of any course opened
    /// before: its index goes first until [`Self::close`].
    pub async fn open(&self, program: &str) -> Result<()> {
        let claim = self.turns.claim(&self.dir_of(program)?, Priority::Awaited);
        // The claim the previous course held is given back as it drops.
        *self.open.lock() = Some((program.to_string(), claim));
        self.put_first(program).await
    }

    /// `program`'s workspace closed. A course opened since stays first.
    pub fn close(&self, program: &str) {
        let mut open = self.open.lock();
        if open.as_ref().is_some_and(|(current, _)| current == program) {
            *open = None;
        }
    }

    /// A reader is about to bring `program` current and wait for it: its
    /// index goes first while the claim is held.
    pub(in crate::features::learning::references::course_index) fn awaited(
        &self,
        program: &str,
    ) -> Result<Claim> {
        Ok(self.turns.claim(&self.dir_of(program)?, Priority::Awaited))
    }

    /// Whether anyone waits on `program`'s index.
    pub(in crate::features::learning::references::course_index) fn is_awaited(
        &self,
        program: &str,
    ) -> bool {
        self.dir_of(program)
            .is_ok_and(|dir| self.turns.claimed(&dir) == Some(Priority::Awaited))
    }

    /// Puts `program`'s live build ahead of every other course's: those give
    /// up their turn, at their next batch when they are embedding, and queue
    /// again behind it. Nothing changes when it has no live build.
    pub(super) async fn put_first(&self, program: &str) -> Result<()> {
        let live = self.jobs.store().live(COURSE_INDEX).await?;
        let ours = |subject: Option<&str>| subject == Some(program);
        if !live.iter().any(|job| ours(job.subject_id.as_deref())) {
            return Ok(());
        }
        for other in live
            .iter()
            .filter(|job| !ours(job.subject_id.as_deref()) && !self.is_awaited_job(job))
        {
            self.jobs.requeue(&other.id);
        }
        Ok(())
    }

    /// Whether `job` builds a course someone waits on, which keeps its turn.
    fn is_awaited_job(&self, job: &lattice_core::shared::runtime::jobs::JobRecord) -> bool {
        job.subject_id
            .as_deref()
            .is_some_and(|program| self.is_awaited(program))
    }

    /// Puts `program`'s build first when someone waits on it; for a build
    /// just queued.
    pub(super) async fn first_if_awaited(&self, program: &str) {
        if !self.is_awaited(program) {
            return;
        }
        if let Err(error) = self.put_first(program).await {
            warn!(%error, program, "Could not put a course's index build first");
        }
    }
}
