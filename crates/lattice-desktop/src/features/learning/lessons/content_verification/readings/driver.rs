//! The reading driver (acceptance design §4): reads each statement along its
//! class's graph until its node is a remedy or an end, one batch per reading
//! a wave, and gives every reading it made with the policy's decision on it.
//! It records nothing; `ClaimLedger::record_reading` takes each step.
use super::*;
use lattice_core::domain::learning::acceptance::{
    decide, rewrite_ahead, AcceptancePolicy, Position, Step, Taken, Then, Walked,
};

/// One reading and the policy's decision on it.
#[derive(Debug)]
pub(in super::super) struct ReadStep {
    pub(in super::super) reading: Reading,
    pub(in super::super) read: Read,
    /// Where the policy sent the statement.
    pub(in super::super) then: Then,
    /// A rewrite is still reachable from where it went.
    pub(in super::super) rewrite_ahead: bool,
    /// The class the step moved the statement to, when it changed class.
    pub(in super::super) class: Option<Class>,
}

/// A statement's readings this time, in order, and where they leave it.
#[derive(Debug)]
pub(in super::super) struct ReadSteps {
    pub(in super::super) steps: Vec<ReadStep>,
    pub(in super::super) walked: Walked,
}

/// A statement on its way through its graph.
struct Course<'s> {
    statement: Statement<'s>,
    at: Position,
    steps: Vec<ReadStep>,
    /// Where an end or a stop left it; none while it is in its graph.
    ended: Option<Walked>,
}

impl Course<'_> {
    /// The reading it waits for; none at a remedy, an end or a stop.
    fn wants(&self, policy: &dyn AcceptancePolicy) -> Result<Option<Reading>> {
        if self.ended.is_some() {
            return Ok(None);
        }
        let node = policy
            .graph(self.at.class)
            .node(self.at.node)
            .ok_or_else(|| {
                AppError::InternalError(format!(
                    "The acceptance policy's {:?} graph has no node {}",
                    self.at.class, self.at.node.0
                ))
            })?;
        Ok(match node.step {
            Step::Read(reading) => Some(reading),
            _ => None,
        })
    }

    /// Takes `read`, the `reading` it was made by, to where the policy sends
    /// the statement.
    fn take(&mut self, policy: &dyn AcceptancePolicy, reading: Reading, read: Read) -> Result<()> {
        let outcome = read.outcome;
        let taken = Taken {
            step: Step::Read(reading),
            outcome,
        };
        let Some((then, next)) = decide(policy, self.at, taken) else {
            return Err(AppError::InternalError(format!(
                "The acceptance policy's {:?} graph has no edge from node {} for the outcome {outcome:?} of its {reading:?} reading",
                self.at.class, self.at.node.0
            )));
        };
        let class = next
            .map(|next| next.class)
            .filter(|class| *class != self.at.class);
        if let Some(class) = class {
            self.statement.classed = Classed {
                class,
                by: ClassifiedBy::Evidence,
            };
        }
        let ahead = next.is_some_and(|next| rewrite_ahead(policy, next));
        match (&then, next) {
            (_, Some(next)) => self.at = next,
            (Then::End(end), None) => self.ended = Some(Walked::Ended(*end)),
            (Then::Stop, None) => self.ended = Some(Walked::Stopped),
            (Then::Node { name, .. }, None) => {
                return Err(AppError::InternalError(format!(
                    "The acceptance policy sent a statement to node {name} without saying where that is"
                )));
            }
        }
        self.steps.push(ReadStep {
            reading,
            read,
            then,
            rewrite_ahead: ahead,
            class,
        });
        Ok(())
    }
}

/// Where a reading falls in a wave: the order a run asks them in, so that its
/// calls are the same every time.
fn place(reading: Reading) -> usize {
    match reading {
        Reading::Parts => 0,
        Reading::Ground => 1,
        Reading::Page => 2,
        Reading::Consistency => 3,
        Reading::Knowledge => 4,
        Reading::Veto => 5,
    }
}

/// Reads each of `statements`, at its position, along its graph until its
/// node is a remedy or an end: each wave gives every reading the statements
/// that wait for it in one call, so statements at the same reading are read
/// together. Gives one `ReadSteps` per statement, in order. A reader's outage
/// is the `Err`; nothing is recorded here, so nothing is half-recorded.
pub(in super::super) async fn read_until_remedy(
    policy: &dyn AcceptancePolicy,
    readers: &dyn ReaderSet,
    at: &ReadContext<'_>,
    statements: &[(Statement<'_>, Position)],
) -> Result<Vec<ReadSteps>> {
    let mut courses: Vec<Course<'_>> = statements
        .iter()
        .map(|&(statement, position)| Course {
            statement,
            at: position,
            steps: Vec::new(),
            ended: None,
        })
        .collect();
    loop {
        let mut waves: Vec<(Reading, Vec<&mut Course<'_>>)> = Vec::new();
        for course in courses.iter_mut() {
            let Some(reading) = course.wants(policy)? else {
                continue;
            };
            match waves.iter_mut().find(|(have, _)| *have == reading) {
                Some((_, members)) => members.push(course),
                None => waves.push((reading, vec![course])),
            }
        }
        if waves.is_empty() {
            break;
        }
        waves.sort_by_key(|(reading, _)| place(*reading));
        for (reading, members) in waves {
            let batch: Vec<Statement<'_>> = members.iter().map(|course| course.statement).collect();
            let reads = ask(readers, at, reading, &batch).await?;
            for (course, read) in members.into_iter().zip(reads) {
                course.take(policy, reading, read)?;
            }
        }
    }
    Ok(courses
        .into_iter()
        .map(|course| ReadSteps {
            walked: course.ended.unwrap_or(Walked::At(course.at)),
            steps: course.steps,
        })
        .collect())
}

/// What `reading`'s reader makes of `batch`, one `Read` for each statement; a
/// `Parts` or `Page` batch is weighed for where its support comes from.
async fn ask(
    readers: &dyn ReaderSet,
    at: &ReadContext<'_>,
    reading: Reading,
    batch: &[Statement<'_>],
) -> Result<Vec<Read>> {
    let reads = readers.get(reading).read(at, batch).await?;
    if reads.len() != batch.len() {
        return Err(AppError::InternalError(format!(
            "The {reading:?} reader gave {} readings for {} statements",
            reads.len(),
            batch.len()
        )));
    }
    match reading {
        Reading::Parts | Reading::Page => weighed(at, batch, reads).await,
        _ => Ok(reads),
    }
}

/// Reads each of `statements` once by the course's page, whatever its class: a
/// reading no graph walks, which answers a user's hint. An answer that could
/// not be read is asked once more, as the driver asks it. Gives one `Read` for
/// each statement, in order, and records nothing.
pub(in super::super) async fn read_page(
    readers: &dyn ReaderSet,
    at: &ReadContext<'_>,
    statements: &[Statement<'_>],
) -> Result<Vec<Read>> {
    let mut reads = ask(readers, at, Reading::Page, statements).await?;
    let unread: Vec<usize> = reads
        .iter()
        .enumerate()
        .filter(|(_, read)| read.outcome == Outcome::Unjudged)
        .map(|(index, _)| index)
        .collect();
    if unread.is_empty() {
        return Ok(reads);
    }
    let again: Vec<Statement<'_>> = unread
        .iter()
        .filter_map(|index| statements.get(*index).copied())
        .collect();
    for (index, read) in unread
        .into_iter()
        .zip(ask(readers, at, Reading::Page, &again).await?)
    {
        if let Some(slot) = reads.get_mut(index) {
            *slot = read;
        }
    }
    Ok(reads)
}

/// The ordinals of the `statements` a quotation found in its source settles,
/// or evidence the user brought: the gate leaves them out of its weighing.
pub(in super::super) fn settled(
    at: &ReadContext<'_>,
    statements: &[Statement<'_>],
) -> Result<HashSet<usize>> {
    let mut settled = HashSet::new();
    for (ordinal, statement) in statements.iter().enumerate() {
        let Statement { unit, claim, .. } = *statement;
        if quote_claims::settle(at.references, at.citations, unit, claim)?.is_some()
            || at.retained.settled_finding(unit, claim).is_some()
        {
            settled.insert(ordinal);
        }
    }
    Ok(settled)
}

/// The supported findings of a `Parts` or `Page` batch, weighed for where
/// their support comes from, as the pass weighs its own. The gate labels a
/// finding with its trust and never changes its verdict, so no outcome moves.
async fn weighed(
    at: &ReadContext<'_>,
    statements: &[Statement<'_>],
    reads: Vec<Read>,
) -> Result<Vec<Read>> {
    let left_out = settled(at, statements)?;
    let (outcomes, findings): (Vec<(Outcome, bool)>, Vec<Finding>) = reads
        .into_iter()
        .map(|read| ((read.outcome, read.reused), read.finding))
        .unzip();
    let mut results: Vec<CheckedClaim> = findings
        .into_iter()
        .enumerate()
        .map(|(ordinal, finding)| (ordinal, String::new(), finding, false))
        .collect();
    provenance_gate::weigh(at.llm, &mut results, &left_out).await?;
    Ok(outcomes
        .into_iter()
        .zip(results)
        .map(|((outcome, reused), (_, _, finding, _))| Read {
            outcome,
            finding,
            reused,
        })
        .collect())
}
