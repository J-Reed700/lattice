//! The round (acceptance design §2.7, Template Method): one function owns the
//! order a lesson's checks go in, the stop of a round that moved nothing and
//! the ends; a review supplies the steps. The preparation job is one review
//! (`repair_loop`); a scripted one in the tests is another, so the loop's end
//! and its stop are tested without a model.
use super::claim_ledger::{Decision, Waiting};
use super::*;
use async_trait::async_trait;
use lattice_core::domain::learning::acceptance::AcceptancePolicy;
use lattice_core::domain::learning::claim_review::StopReason;

/// What a round of research came to.
pub(super) enum Researched {
    /// No statement waits for research and no hint names a claim.
    Nothing,
    /// It reached no search engine or page, so it recorded nothing and the
    /// statements stay where they are.
    Unreached,
    /// It searched and read what it saved: whether it saved a source, and the
    /// statements whose reading of it stopped.
    Searched { saved: bool, stopped: Vec<ClaimId> },
}

/// What settling the sections after the cuts left (claim classes design
/// §3.10).
pub(super) enum Settled {
    /// The draft, or the sections that go, changed: the lesson is read again.
    Again,
    /// Nothing is left to settle.
    Done,
}

/// What the Verifier audit lets a lesson whose claims are resolved do (§14.3).
pub(super) enum Audit {
    /// No audit gate.
    Off,
    /// The Verifier agrees with every claim.
    Agreed,
    /// The lesson waits for the user: this many claims, and the stop that kept
    /// the audit from finishing (boxed: a stop is far larger than the other
    /// variants).
    Hold {
        claims: usize,
        reason: Option<Box<StopReason>>,
    },
}

/// How a lesson's rounds end.
pub(super) enum Verdict {
    /// Every claim is resolved: the lesson publishes.
    Publish,
    /// The lesson waits for the user's review (§6.1): this many claims, with
    /// the stop that says why when there is one.
    Hold {
        claims: usize,
        reason: Option<StopReason>,
    },
    /// The lesson stops for this reason.
    Stop(StopReason),
    /// A round moved no statement and left the draft as it was: the
    /// statements that wait, and whether the round's research reached nothing.
    NoProgress {
        waiting: Vec<Waiting>,
        unreached: bool,
    },
}

/// The steps of a round, which `run` takes in its order. Each acts on the
/// review's own draft, ledger and sources, under the round's policy.
#[async_trait]
pub(super) trait Review: Send {
    /// What a lesson that publishes comes to.
    type Finished;

    /// A sync point (§9): the draft as it stands, and what the user did since
    /// the last one applied to it.
    async fn sync(&mut self, policy: &dyn AcceptancePolicy) -> Result<()>;
    /// Resumes a repair saved before a restart; whether it did, so that the
    /// round reads what it wrote.
    async fn resume(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool>;
    /// The decisions recorded so far: a round that records none has moved no
    /// statement.
    fn stepped(&self) -> usize;
    /// The draft as it stands.
    fn draft(&self) -> &str;
    /// The draft's statements and its examples' runs, before anything is read.
    async fn extract(&mut self) -> Result<()>;
    /// The draft with a repair that wrote a new statement taken back, before
    /// the statement is identified: one the fresh extraction holds that
    /// continues neither the repair's target nor a statement on its sentence
    /// (acceptance design §2.6).
    async fn take_back(&mut self, policy: &dyn AcceptancePolicy) -> Result<Option<String>>;
    /// Which claim of the ledger each extracted statement is.
    async fn identify(&mut self, policy: &dyn AcceptancePolicy) -> Result<()>;
    /// Why the policy cannot walk a statement, which stops the lesson.
    fn fault(&self, policy: &dyn AcceptancePolicy) -> Option<StopReason>;
    /// Reads every statement that waits for a reading on, to a remedy or an
    /// end, and reports what the latest readings found: the statements whose
    /// answers could not be read, asked twice.
    async fn read(&mut self, policy: &dyn AcceptancePolicy) -> Result<Vec<ClaimId>>;
    /// The draft with a flow repair that wrote a statement taken back (claim
    /// classes design §3.10).
    async fn unmend(&mut self) -> Result<Option<String>>;
    /// A draft a take-back restored becomes the draft, saved.
    async fn restore(&mut self, draft: String) -> Result<()>;
    /// One round of research for the statements at research nodes and the
    /// claims a hint names, and the reading of what it saved.
    async fn research(&mut self, policy: &dyn AcceptancePolicy) -> Result<Researched>;
    /// Moves the citations the statements at recite nodes wait for: whether
    /// that changed the draft, which is then saved.
    async fn recite(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool>;
    /// Drops the citations the statements at uncite nodes wait for: whether
    /// that changed the draft, which is then saved.
    async fn uncite(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool>;
    /// What the lesson does next (acceptance design §2.10), `stopped` being
    /// the statements this run stopped on.
    fn decide(&self, policy: &dyn AcceptancePolicy, stopped: &[ClaimId]) -> Decision;
    /// The round goes again for the statements that wait.
    fn again(&mut self, waiting: &[Waiting]);
    /// Repairs `targets`: the rewrites by section and the cuts, one repair.
    async fn repair(&mut self, policy: &dyn AcceptancePolicy, targets: &[ClaimId]) -> Result<()>;
    /// The last sync point: whether the lesson still publishes once what the
    /// user did meanwhile is read.
    async fn still_publishes(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool>;
    /// Settles the sections after the cuts. A lesson that cannot keep its
    /// shape stops, and that stop is the error.
    async fn settle(&mut self) -> Result<Settled>;
    /// The last guard (acceptance design §2.9): the stop for a sentence the
    /// lesson still holds that a claim the machine ended stood on, and how
    /// many such claims there are.
    fn ended_but_standing(&self) -> Option<(StopReason, usize)>;
    /// Takes the sections that go out of the lesson.
    async fn excise(&mut self) -> Result<()>;
    /// The Verifier audit, when the course opted in (§14.3).
    async fn audit(&mut self) -> Result<Audit>;
    /// Ends the lesson as `verdict` says: what it publishes, or the error that
    /// unwinds a hold or a stop.
    async fn finish(&mut self, verdict: Verdict) -> Result<Self::Finished>;
}

/// Takes `review`'s lesson round by round until it publishes, waits for the
/// user or stops (acceptance design §2.7, §2.10). A round syncs what the user
/// did, resumes a saved repair the first time, extracts, takes back a repair
/// that wrote a statement and identifies, and stops on a fault; then it reads,
/// takes back a flow repair that wrote a statement, researches and reads what
/// research saved, recites or uncites, and repairs the statements at rewrite
/// and cut nodes. A round that records no step and leaves the draft as it was
/// stops the lesson with what was pending. Once nothing is left for the
/// machine, the sections are settled and the lesson publishes or waits for the
/// user.
pub(super) async fn run<F>(
    review: &mut dyn Review<Finished = F>,
    policy: &dyn AcceptancePolicy,
) -> Result<F> {
    let mut resume = true;
    loop {
        review.sync(policy).await?;
        // A failed rewrite can resume from its saved defects. This only skips
        // repeating the diagnosis; it never skips reading the rewritten text.
        if std::mem::take(&mut resume) && review.resume(policy).await? {
            continue;
        }
        // What this iteration starts from, to know whether it moved anything.
        let (stepped, draft) = (review.stepped(), review.draft().to_owned());
        review.extract().await?;
        if let Some(restored) = review.take_back(policy).await? {
            review.restore(restored).await?;
            continue;
        }
        review.identify(policy).await?;
        if let Some(reason) = review.fault(policy) {
            return review.finish(Verdict::Stop(reason)).await;
        }
        let mut stopped = review.read(policy).await?;
        // A flow repair that wrote a statement is taken back (claim classes
        // design §3.10).
        if let Some(restored) = review.unmend().await? {
            review.restore(restored).await?;
            continue;
        }
        // Research for the statements at a research node and the claims a hint
        // names, once per claim: what it saves is read at once, and what it
        // does not find is recorded as tried (§4). A claim a hint names is the
        // user's: it takes no step, and the page that reads what the round
        // saved answers the hint. A round that reaches nothing records
        // nothing, and the claims stay where they are.
        let mut unreached = false;
        while stopped.is_empty() {
            match review.research(policy).await? {
                Researched::Nothing => break,
                Researched::Unreached => {
                    unreached = true;
                    break;
                }
                Researched::Searched {
                    saved,
                    stopped: read,
                } => {
                    stopped.extend(read);
                    if !saved {
                        break;
                    }
                }
            }
        }
        if stopped.is_empty() && (review.recite(policy).await? || review.uncite(policy).await?) {
            continue;
        }
        match review.decide(policy, &stopped) {
            Decision::Continue(waiting) => {
                // A pass that moved no statement and changed nothing would go
                // round again as it was.
                if review.stepped() == stepped && review.draft() == draft {
                    let verdict = Verdict::NoProgress { waiting, unreached };
                    return review.finish(verdict).await;
                }
                review.again(&waiting);
            }
            Decision::Stop(reason) => return review.finish(Verdict::Stop(*reason)).await,
            Decision::Publish => {
                // The last sync point: an edit made during this pass is
                // applied and verified before anything publishes.
                if !review.still_publishes(policy).await? {
                    continue;
                }
                // Sections after cuts (claim classes design §3.10): the flow
                // repairs and the sections that go are settled and checked
                // before the lesson is final, and the sections that go are then
                // taken out.
                match review.settle().await? {
                    Settled::Again => continue,
                    Settled::Done => {}
                }
                // The last guard (acceptance design §2.9): the lesson holds no
                // sentence that a claim the machine ended stood on.
                if let Some((reason, claims)) = review.ended_but_standing() {
                    let reason = Some(reason);
                    return review.finish(Verdict::Hold { claims, reason }).await;
                }
                review.excise().await?;
                // Then the Verifier audit, when the course opted in (§14.3):
                // review pass → audit → publish. Its checks, and anything the
                // user did while it ran, are read back before publishing.
                match review.audit().await? {
                    Audit::Off => {}
                    Audit::Agreed => {
                        if !review.still_publishes(policy).await? {
                            continue;
                        }
                    }
                    Audit::Hold { claims, reason } => {
                        let reason = reason.map(|stop| *stop);
                        return review.finish(Verdict::Hold { claims, reason }).await;
                    }
                }
                return review.finish(Verdict::Publish).await;
            }
            Decision::NeedsReview(claims) => {
                let reason = None;
                return review.finish(Verdict::Hold { claims, reason }).await;
            }
            Decision::Repair(targets) => review.repair(policy, &targets).await?,
        }
    }
}

#[cfg(test)]
#[path = "round_tests.rs"]
mod tests;
