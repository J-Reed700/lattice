//! The round on a scripted review (acceptance design §2.7): statements that
//! walk the standard policy's graphs as their scripts say, and a draft that is
//! a string, so the round's end, its stop and its order are tested without a
//! model.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::*;
use async_trait::async_trait;
use lattice_core::domain::learning::acceptance::{
    decide, start, Classes, End, NodeId, Outcome, Position, StandardPolicy, Step, Taken, Then,
    Walked, CLASSES,
};
use std::collections::VecDeque;

/// A statement whose every step's outcome is given.
struct Scripted {
    claim: ClaimId,
    walked: Walked,
    /// The outcomes of the steps it has still to take, in order.
    script: VecDeque<Outcome>,
    /// The remedy at its node does nothing for it: research reaches nothing,
    /// a recite waits.
    stuck: bool,
}

impl Scripted {
    /// A statement at `node` of `class`'s graph, which takes the steps
    /// `outcomes` give.
    fn at(
        claim: &str,
        (class, node): (Class, &'static str),
        outcomes: impl IntoIterator<Item = Outcome>,
    ) -> Self {
        Self {
            claim: ClaimId(claim.to_owned()),
            walked: Walked::At(Position {
                class,
                visited: Classes::of(class),
                reasked: false,
                node: NodeId(node),
                rewrites_left: 2,
            }),
            script: outcomes.into_iter().collect(),
            stuck: false,
        }
    }

    /// A statement at the entry of `class`'s graph.
    fn entering(
        policy: &dyn AcceptancePolicy,
        claim: &str,
        class: Class,
        outcomes: impl IntoIterator<Item = Outcome>,
    ) -> Self {
        Self::at(claim, (class, policy.graph(class).entry().0), outcomes)
    }

    fn stuck(self) -> Self {
        Self {
            stuck: true,
            ..self
        }
    }

    /// The node it waits at, and that node's step.
    fn node(&self, policy: &dyn AcceptancePolicy) -> Option<(&'static str, Step)> {
        let Walked::At(at) = self.walked else {
            return None;
        };
        let node = policy.graph(at.class).node(at.node)?;
        Some((node.name, node.step))
    }

    /// Takes the step of the node it waits at, with the outcome its script
    /// gives next: the node's name.
    fn take(&mut self, policy: &dyn AcceptancePolicy) -> &'static str {
        let (Walked::At(at), Some((name, step))) = (self.walked, self.node(policy)) else {
            panic!("{} waits at no node", self.claim.0);
        };
        let outcome = self
            .script
            .pop_front()
            .unwrap_or_else(|| panic!("{} has no outcome left at {name}", self.claim.0));
        self.walked = match decide(policy, at, Taken { step, outcome }) {
            Some((_, Some(next))) => Walked::At(next),
            Some((Then::End(end), None)) => Walked::Ended(end),
            Some((Then::Stop, None)) => Walked::Stopped,
            other => panic!("{} at {name}: {outcome:?} gives {other:?}", self.claim.0),
        };
        name
    }
}

/// A call the round made: in which round, of which phase, and the steps the
/// statements took in it, each by its claim and the node it took it at.
#[derive(Debug, Clone, PartialEq)]
struct Call {
    round: usize,
    phase: &'static str,
    steps: Vec<(String, &'static str)>,
}

/// How a scripted lesson's rounds ended.
#[derive(Debug, PartialEq)]
enum Ended {
    Published,
    Held(usize),
    Stopped(StopCode),
    /// Each statement that waited, the node it waited at and that node's
    /// step, and whether research reached nothing.
    NoProgress {
        waiting: Vec<(String, &'static str, Step)>,
        unresearched: Option<Unresearched>,
    },
}

/// A review whose statements walk as their scripts say, whose draft is a
/// string, and which logs each call the round makes of it.
struct ScriptedReview {
    statements: Vec<Scripted>,
    draft: String,
    /// The steps its statements took.
    stepped: usize,
    /// The rounds begun.
    rounds: usize,
    calls: Vec<Call>,
    /// The statements whose answers cannot be read, reported by the next read.
    unreadable: Vec<ClaimId>,
    /// A claim the machine ended whose sentence the lesson still holds.
    standing: Option<(StopReason, usize)>,
    /// The statements published not verified, in the order they were marked.
    unverified: Vec<String>,
    /// Whether any statement can be marked not verified.
    marks: bool,
    /// Claims only the user could settle: the round's decision until they are
    /// marked.
    awaiting: Option<usize>,
}

impl ScriptedReview {
    fn new(statements: Vec<Scripted>) -> Self {
        Self {
            statements,
            draft: "A draft.".to_owned(),
            stepped: 0,
            rounds: 0,
            calls: Vec::new(),
            unreadable: Vec::new(),
            standing: None,
            unverified: Vec::new(),
            marks: true,
            awaiting: None,
        }
    }

    fn unmarkable(self) -> Self {
        Self {
            marks: false,
            ..self
        }
    }

    fn log(&mut self, phase: &'static str, steps: Vec<(String, &'static str)>) {
        self.calls.push(Call {
            round: self.rounds,
            phase,
            steps,
        });
    }

    /// Every statement that waits at a node whose step `wanted` holds takes
    /// it, unless it is stuck: the steps taken.
    fn take(
        &mut self,
        policy: &dyn AcceptancePolicy,
        wanted: impl Fn(Step) -> bool,
    ) -> Vec<(String, &'static str)> {
        let mut taken = Vec::new();
        for statement in &mut self.statements {
            let due = statement.node(policy).is_some_and(|(_, step)| wanted(step));
            if due && !statement.stuck {
                let node = statement.take(policy);
                taken.push((statement.claim.0.clone(), node));
            }
        }
        self.stepped += taken.len();
        taken
    }

    /// Reads every statement that waits for a reading on, to a remedy or an
    /// end.
    fn read_on(&mut self, policy: &dyn AcceptancePolicy) -> Vec<(String, &'static str)> {
        let mut taken = Vec::new();
        loop {
            let read = self.take(policy, |step| matches!(step, Step::Read(_)));
            if read.is_empty() {
                return taken;
            }
            taken.extend(read);
        }
    }

    /// A remedy's steps for the statements at the nodes `wanted` holds:
    /// whether any took one, which changes the draft.
    fn remedy(
        &mut self,
        phase: &'static str,
        policy: &dyn AcceptancePolicy,
        wanted: impl Fn(Step) -> bool,
    ) -> bool {
        let taken = self.take(policy, wanted);
        let changed = !taken.is_empty();
        if changed {
            self.draft.push_str(" Changed.");
        }
        self.log(phase, taken);
        changed
    }
}

#[async_trait]
impl Review for ScriptedReview {
    type Finished = Ended;

    async fn sync(&mut self, _policy: &dyn AcceptancePolicy) -> Result<()> {
        self.rounds += 1;
        self.log("sync", Vec::new());
        Ok(())
    }

    async fn resume(&mut self, _policy: &dyn AcceptancePolicy) -> Result<bool> {
        self.log("resume", Vec::new());
        Ok(false)
    }

    fn stepped(&self) -> usize {
        self.stepped
    }

    fn draft(&self) -> &str {
        &self.draft
    }

    async fn extract(&mut self) -> Result<()> {
        self.log("extract", Vec::new());
        Ok(())
    }

    async fn identify(&mut self, _policy: &dyn AcceptancePolicy) -> Result<()> {
        self.log("identify", Vec::new());
        Ok(())
    }

    fn fault(&self, _policy: &dyn AcceptancePolicy) -> Option<StopReason> {
        None
    }

    async fn read(&mut self, policy: &dyn AcceptancePolicy) -> Result<Vec<ClaimId>> {
        let read = self.read_on(policy);
        self.log("read", read);
        Ok(std::mem::take(&mut self.unreadable))
    }

    async fn unmend(&mut self) -> Result<Option<String>> {
        self.log("unmend", Vec::new());
        Ok(None)
    }

    async fn restore(&mut self, draft: String) -> Result<()> {
        self.draft = draft;
        self.log("restore", Vec::new());
        Ok(())
    }

    async fn research(&mut self, policy: &dyn AcceptancePolicy) -> Result<Researched> {
        let (mut due, mut stuck, mut saved) = (false, true, false);
        for statement in &self.statements {
            if statement
                .node(policy)
                .is_some_and(|(_, step)| step == Step::Research)
            {
                due = true;
                stuck &= statement.stuck;
                saved |= !statement.stuck && statement.script.front() == Some(&Outcome::Saved);
            }
        }
        if !due || stuck {
            self.log("research", Vec::new());
            return Ok(if due {
                Researched::Unresearched(Unresearched::Unreached)
            } else {
                Researched::Nothing
            });
        }
        let mut taken = self.take(policy, |step| step == Step::Research);
        taken.extend(self.read_on(policy));
        self.log("research", taken);
        Ok(Researched::Searched {
            saved,
            stopped: Vec::new(),
        })
    }

    async fn recite(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool> {
        Ok(self.remedy("recite", policy, |step| step == Step::Recite))
    }

    async fn uncite(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool> {
        Ok(self.remedy("uncite", policy, |step| step == Step::Uncite))
    }

    fn decide(&self, policy: &dyn AcceptancePolicy, stopped: &[ClaimId]) -> Decision {
        if !stopped.is_empty() {
            return Decision::Stop(Box::new(StopReason {
                code: StopCode::CheckerUnreadable,
                message: String::new(),
                fix: String::new(),
                at: 0,
                request: None,
            }));
        }
        if let Some(claims) = self.awaiting {
            return Decision::NeedsReview(claims);
        }
        let (mut repairs, mut waiting) = (Vec::new(), Vec::new());
        for statement in &self.statements {
            let Some((node, step)) = statement.node(policy) else {
                continue;
            };
            let claim = statement.claim.clone();
            match step {
                Step::Rewrite(_) | Step::Cut => repairs.push(claim),
                step => waiting.push(Waiting { claim, node, step }),
            }
        }
        if !repairs.is_empty() {
            Decision::Repair(repairs)
        } else if !waiting.is_empty() {
            Decision::Continue(waiting)
        } else {
            Decision::Publish
        }
    }

    fn again(&mut self, _waiting: &[Waiting]) {
        self.log("again", Vec::new());
    }

    async fn repair(&mut self, policy: &dyn AcceptancePolicy, targets: &[ClaimId]) -> Result<()> {
        let mut taken = Vec::new();
        for statement in &mut self.statements {
            if targets.contains(&statement.claim) {
                let node = statement.take(policy);
                taken.push((statement.claim.0.clone(), node));
            }
        }
        self.stepped += taken.len();
        if !taken.is_empty() {
            self.draft.push_str(" Repaired.");
        }
        self.log("repair", taken);
        Ok(())
    }

    async fn still_publishes(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool> {
        self.log("still_publishes", Vec::new());
        Ok(self
            .statements
            .iter()
            .all(|statement| statement.node(policy).is_none()))
    }

    async fn settle(&mut self) -> Result<Settled> {
        let waiting = self
            .statements
            .iter()
            .filter_map(|statement| match statement.walked {
                Walked::At(at) => Some((statement.claim.0.clone(), at.node.0)),
                _ => None,
            })
            .collect();
        self.log("settle", waiting);
        Ok(Settled::Done)
    }

    async fn publish_unsettled(&mut self, unsettled: Unsettled) -> Result<usize> {
        self.log("publish_unsettled", Vec::new());
        let claims = match unsettled {
            Unsettled::Waiting(claims) | Unsettled::Unreadable(claims) => claims,
            Unsettled::AwaitingUser => {
                let marked = usize::from(self.marks && self.awaiting.take().is_some());
                if marked > 0 {
                    self.unverified.push("awaiting".to_owned());
                }
                return Ok(marked);
            }
            Unsettled::Ended => {
                let marked = usize::from(self.marks && self.standing.take().is_some());
                if marked > 0 {
                    self.unverified.push("ended".to_owned());
                }
                return Ok(marked);
            }
        };
        if !self.marks {
            return Ok(0);
        }
        let mut marked = 0;
        for statement in &mut self.statements {
            if claims.contains(&statement.claim) {
                statement.walked = Walked::Stopped;
                self.unverified.push(statement.claim.0.clone());
                marked += 1;
            }
        }
        Ok(marked)
    }

    fn ended_but_standing(&self) -> Option<(StopReason, usize)> {
        self.standing.clone()
    }

    async fn excise(&mut self) -> Result<()> {
        self.log("excise", Vec::new());
        Ok(())
    }

    async fn audit(&mut self) -> Result<Audit> {
        self.log("audit", Vec::new());
        Ok(Audit::Off)
    }

    async fn finish(&mut self, verdict: Verdict) -> Result<Ended> {
        self.log("finish", Vec::new());
        Ok(match verdict {
            Verdict::Publish => Ended::Published,
            Verdict::Hold { claims, .. } => Ended::Held(claims),
            Verdict::Stop(reason) => Ended::Stopped(reason.code),
            Verdict::NoProgress {
                waiting,
                unresearched,
            } => Ended::NoProgress {
                waiting: waiting
                    .into_iter()
                    .map(|waits| (waits.claim.0, waits.node, waits.step))
                    .collect(),
                unresearched,
            },
        })
    }
}

/// A path of a class's graph: the steps a statement that enters it takes, and
/// the end they reach.
struct Path {
    class: Class,
    taken: Vec<Taken>,
    end: End,
}

/// Every path of every class's graph in `policy`, from the class's entry to an
/// end, through each change of class the graphs make.
fn paths(policy: &dyn AcceptancePolicy) -> Vec<Path> {
    let mut found = Vec::new();
    for class in CLASSES {
        let entry = start(policy, class);
        extend(policy, (class, entry), &mut Vec::new(), &mut found);
    }
    found
}

/// Each path that `taken`, the steps from `class`'s entry to `at`, goes on to
/// by the edges of `at`'s node.
fn extend(
    policy: &dyn AcceptancePolicy,
    (class, at): (Class, Position),
    taken: &mut Vec<Taken>,
    found: &mut Vec<Path>,
) {
    let node = policy.graph(at.class).node(at.node).unwrap();
    for &(outcome, _) in node.edges() {
        let step = Taken {
            step: node.step,
            outcome,
        };
        taken.push(step);
        match decide(policy, at, step) {
            Some((_, Some(next))) => extend(policy, (class, next), taken, found),
            Some((Then::End(end), None)) => found.push(Path {
                class,
                taken: taken.clone(),
                end,
            }),
            other => panic!("{}: {outcome:?} gives {other:?}", node.name),
        }
        taken.pop();
    }
}

/// The phases of the calls the round made after it marked statements not
/// verified: the Publish branch, and no new round.
fn after_the_mark(review: &ScriptedReview) -> Vec<&'static str> {
    let marked = review
        .calls
        .iter()
        .position(|call| call.phase == "publish_unsettled")
        .expect("the round marked statements");
    review.calls[marked + 1..]
        .iter()
        .map(|call| call.phase)
        .collect()
}

const THE_BRANCH: [&str; 5] = ["still_publishes", "settle", "excise", "audit", "finish"];

#[tokio::test]
async fn every_path_of_every_graph_ends_where_it_says_in_no_more_rounds_than_it_has_steps() {
    let policy = StandardPolicy::new();
    let paths = paths(&policy);
    for class in CLASSES {
        assert!(paths.iter().any(|path| path.class == class), "{class:?}");
    }
    for path in &paths {
        let outcomes = path.taken.iter().map(|taken| taken.outcome);
        let statement = Scripted::entering(&policy, "statement", path.class, outcomes);
        let mut review = ScriptedReview::new(vec![statement]);

        let ended = run(&mut review, &policy).await.unwrap();

        let statement = &review.statements[0];
        assert_eq!(
            (ended, statement.walked),
            (Ended::Published, Walked::Ended(path.end)),
            "{:?} {:?}",
            path.class,
            path.taken
        );
        assert!(statement.script.is_empty(), "{:?}", path.taken);
        assert!(
            review.rounds <= path.taken.len(),
            "{} rounds for {:?} {:?}",
            review.rounds,
            path.class,
            path.taken
        );
    }
}

#[tokio::test]
async fn a_round_that_moves_nothing_stops_naming_what_each_statement_waits_for() {
    let policy = StandardPolicy::new();
    // Its citation moves in the first round and it is read in the second; in
    // the third nothing moves, since research reaches nothing for the second
    // statement and the third's citation waits.
    let moved = Scripted::at(
        "moved",
        (Class::Sourced, "s.recite"),
        [Outcome::Done, Outcome::Stands],
    );
    let unreached = Scripted::at("unreached", (Class::Sourced, "s.research"), []).stuck();
    let deferred = Scripted::at("deferred", (Class::Sourced, "s.recite"), []).stuck();
    let mut review = ScriptedReview::new(vec![moved, unreached, deferred]);

    let ended = run(&mut review, &policy).await.unwrap();

    assert_eq!(
        ended,
        Ended::NoProgress {
            waiting: vec![
                ("unreached".to_owned(), "s.research", Step::Research),
                ("deferred".to_owned(), "s.recite", Step::Recite),
            ],
            unresearched: Some(Unresearched::Unreached),
        }
    );
    assert_eq!(review.rounds, 3);
    assert_eq!(review.statements[0].walked, Walked::Ended(End::Resolved));
}

#[tokio::test]
async fn a_round_that_reached_the_web_and_moved_nothing_publishes_what_waits_not_verified() {
    let policy = StandardPolicy::new();
    let moved = Scripted::at(
        "moved",
        (Class::Sourced, "s.recite"),
        [Outcome::Done, Outcome::Stands],
    );
    let deferred = Scripted::at("deferred", (Class::Sourced, "s.recite"), []).stuck();
    let mut review = ScriptedReview::new(vec![moved, deferred]);

    let ended = run(&mut review, &policy).await.unwrap();

    assert_eq!(ended, Ended::Published);
    assert_eq!(review.unverified, ["deferred"]);
    assert_eq!(review.statements[0].walked, Walked::Ended(End::Resolved));
    // It was settled and published once, after the statement was marked, and
    // no round began for the mark.
    let settled = review.calls.iter().filter(|call| call.phase == "settle");
    assert_eq!(settled.count(), 1, "{:?}", review.calls);
    assert_eq!(after_the_mark(&review), THE_BRANCH, "{:?}", review.calls);
    assert_eq!(review.rounds, 3);
}

#[tokio::test]
async fn claims_only_the_user_could_settle_are_published_not_verified_in_the_same_pass() {
    let policy = StandardPolicy::new();
    let mut review = ScriptedReview::new(vec![]);
    review.awaiting = Some(2);

    let ended = run(&mut review, &policy).await.unwrap();

    assert_eq!(ended, Ended::Published);
    assert_eq!(review.unverified, ["awaiting"]);
    assert_eq!(after_the_mark(&review), THE_BRANCH, "{:?}", review.calls);
    assert_eq!(review.rounds, 1);
}

#[tokio::test]
async fn an_unreadable_statement_marked_beside_one_with_machine_work_left_does_not_publish_until_that_is_done(
) {
    let policy = StandardPolicy::new();
    let silent = Scripted::at("silent", (Class::Sourced, "s.read"), []).stuck();
    let other = Scripted::at(
        "other",
        (Class::Sourced, "s.recite"),
        [Outcome::Done, Outcome::Stands],
    );
    let mut review = ScriptedReview::new(vec![silent, other]);
    review.unreadable = vec![ClaimId("silent".to_owned())];

    let ended = run(&mut review, &policy).await.unwrap();

    assert_eq!(ended, Ended::Published);
    assert_eq!(review.unverified, ["silent"]);
    assert_eq!(review.statements[1].walked, Walked::Ended(End::Resolved));
    // The branch's own check refused the first time, so the round went again
    // for the other statement, and the lesson was settled once, after it.
    let phases: Vec<&str> = review.calls.iter().map(|call| call.phase).collect();
    let first_check = phases.iter().position(|phase| *phase == "still_publishes");
    let settled = phases.iter().position(|phase| *phase == "settle");
    assert!(first_check < settled, "{phases:?}");
    assert_eq!(phases.iter().filter(|phase| **phase == "settle").count(), 1);
    assert!(
        phases
            .iter()
            .filter(|phase| **phase == "still_publishes")
            .count()
            >= 2,
        "{phases:?}"
    );
    assert!(review.rounds >= 2, "{phases:?}");
}

#[tokio::test]
async fn a_round_that_cannot_mark_what_waits_stops_as_it_did() {
    let policy = StandardPolicy::new();
    let deferred = Scripted::at("deferred", (Class::Sourced, "s.recite"), []).stuck();
    let mut review = ScriptedReview::new(vec![deferred]).unmarkable();

    let ended = run(&mut review, &policy).await.unwrap();

    assert_eq!(
        ended,
        Ended::NoProgress {
            waiting: vec![("deferred".to_owned(), "s.recite", Step::Recite)],
            unresearched: None,
        }
    );
    assert!(review.unverified.is_empty());
}

#[tokio::test]
async fn statements_whose_answers_cannot_be_read_are_published_not_verified() {
    let policy = StandardPolicy::new();
    let silent = Scripted::at("silent", (Class::Sourced, "s.read"), []).stuck();
    let mut review = ScriptedReview::new(vec![silent]);
    review.unreadable = vec![ClaimId("silent".to_owned())];

    let ended = run(&mut review, &policy).await.unwrap();

    assert_eq!(ended, Ended::Published);
    assert_eq!(review.unverified, ["silent"]);
}

#[tokio::test]
async fn a_claim_the_machine_ended_whose_sentence_stands_is_published_not_verified_or_held() {
    let policy = StandardPolicy::new();
    let standing = || {
        Some((
            StopReason {
                code: StopCode::EndedClaimStands,
                message: String::new(),
                fix: String::new(),
                at: 0,
                request: None,
            },
            1,
        ))
    };
    let mut review = ScriptedReview::new(vec![]);
    review.standing = standing();

    assert_eq!(run(&mut review, &policy).await.unwrap(), Ended::Published);
    assert_eq!(review.unverified, ["ended"]);
    // The branch goes on to excise and publishes in the same pass.
    assert_eq!(
        after_the_mark(&review),
        ["excise", "audit", "finish"],
        "{:?}",
        review.calls
    );
    assert_eq!(review.rounds, 1);

    let mut review = ScriptedReview::new(vec![]).unmarkable();
    review.standing = standing();

    assert_eq!(run(&mut review, &policy).await.unwrap(), Ended::Held(1));
    assert!(review.unverified.is_empty());
}

#[tokio::test]
async fn a_round_reads_then_researches_then_recites_then_repairs_and_settles_once_nothing_waits() {
    let policy = StandardPolicy::new();
    // Read, researched and read again to its rewrite in the first round, and
    // read to its end in the second.
    let rewritten = Scripted::entering(
        &policy,
        "rewritten",
        Class::Sourced,
        [
            Outcome::Unproven,
            Outcome::FoundNothing,
            Outcome::Refuted,
            Outcome::Rewritten,
            Outcome::Stands,
        ],
    );
    let cut = Scripted::at("cut", (Class::Reasoning, "r.cut"), [Outcome::Cut]);
    let mut review = ScriptedReview::new(vec![rewritten, cut]);

    let ended = run(&mut review, &policy).await.unwrap();

    assert_eq!(ended, Ended::Published);
    let step = |claim: &str, node: &'static str| (claim.to_owned(), node);
    let first: Vec<(&str, Vec<(String, &str)>)> = review
        .calls
        .iter()
        .filter(|call| call.round == 1)
        .map(|call| (call.phase, call.steps.clone()))
        .collect();
    assert_eq!(
        first,
        vec![
            ("sync", vec![]),
            ("resume", vec![]),
            ("extract", vec![]),
            ("identify", vec![]),
            ("read", vec![step("rewritten", "s.read")]),
            ("unmend", vec![]),
            (
                "research",
                vec![
                    step("rewritten", "s.research"),
                    step("rewritten", "s.reread")
                ]
            ),
            ("recite", vec![]),
            ("uncite", vec![]),
            (
                "repair",
                vec![step("rewritten", "s.rewrite"), step("cut", "r.cut")]
            ),
        ]
    );
    let settled: Vec<&Call> = review
        .calls
        .iter()
        .filter(|call| call.phase == "settle")
        .collect();
    assert_eq!(settled.len(), 1, "{:?}", review.calls);
    assert_eq!(settled[0].round, review.rounds);
    assert!(settled[0].steps.is_empty(), "{:?}", settled[0].steps);
}
