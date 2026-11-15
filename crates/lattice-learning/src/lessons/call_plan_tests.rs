#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;
use lattice_core::domain::learning::acceptance::{
    next_rewrite, start, stepped, walk_from, Classes, End, Graph, Next, Node, Outcome,
    StandardPolicy, Step, Taken, CLASSES,
};
use std::collections::{HashSet, VecDeque};

fn alone(class: Class) -> Planned {
    Planned {
        class,
        at: PlannedAt::Entry,
    }
}

/// The plan as production makes it: empty, then each of `statements` taken in
/// where it stands, beside the earlier ones on its sentence. `sentences` holds
/// the sentence each stands on, none for one past its end.
fn made(
    policy: &dyn AcceptancePolicy,
    statements: &[Planned],
    sentences: &[Option<u32>],
    sections: u32,
) -> CallPlan {
    let sentence = |index: usize| sentences.get(index).copied().flatten();
    let joining: Vec<Joining> = statements
        .iter()
        .enumerate()
        .map(|(index, planned)| Joining {
            planned: *planned,
            beside: (0..index)
                .filter(|&earlier| {
                    sentence(index).is_some() && sentence(earlier) == sentence(index)
                })
                .collect(),
        })
        .collect();
    let mut plan = CallPlan::empty(sections);
    plan.take_in(policy, &joining);
    plan
}

fn ended(statement: Planned) -> Planned {
    Planned {
        at: PlannedAt::Ended(None),
        ..statement
    }
}

/// `statement`, at `position`.
fn moved_to(statement: Planned, position: Position) -> Planned {
    Planned {
        at: PlannedAt::At(position),
        ..statement
    }
}

/// The node `name` of `class`'s graph, with `rewrites_left` rewrites left and
/// nothing else spent.
fn at_node(policy: &dyn AcceptancePolicy, class: Class, name: &str, rewrites_left: u8) -> Position {
    Position {
        class,
        visited: Classes::of(class),
        reasked: false,
        node: policy.graph(class).id_of(name).unwrap(),
        rewrites_left,
    }
}

fn mosts(plan: &CallPlan) -> Vec<u32> {
    plan.slots.iter().map(|slot| slot.most).collect()
}

fn reserves(plan: &CallPlan) -> Vec<u32> {
    plan.slots.iter().map(|slot| slot.reserve).collect()
}

#[test]
fn the_ceiling_of_a_plan_made_is_the_sorting_calls_each_longest_path_the_reserves_and_the_flow_repairs(
) {
    let policy = StandardPolicy::new();
    // A Sourced and a Reasoning statement share a sentence, two Common ones
    // share another, and a Sourced and a Common one stand alone.
    let statements = [
        alone(Class::Sourced),
        alone(Class::Reasoning),
        alone(Class::Sourced),
        alone(Class::Common),
        alone(Class::Common),
        alone(Class::Common),
    ];
    let plan = made(
        &policy,
        &statements,
        &[Some(0), Some(0), None, Some(1), Some(1), None],
        2,
    );

    assert_eq!(mosts(&plan), [19, 20, 19, 19, 19, 19]);
    assert_eq!(reserves(&plan), [17, 17, 0, 15, 15, 0]);
    // The 3 sorting calls, made; the longest path from each entry with the
    // re-ask and a teaching rewrite, 19 for each of the six but the Reasoning
    // one, whose path through Sourced into Common is one call longer (20, 115
    // in all); the reserve of each of the four that share a sentence, two
    // readings from the post-rewrite node with the re-ask still possible and
    // the narrowing call, 2 × 8 + 1 = 17 for the Sourced and the Reasoning one
    // and 2 × 7 + 1 = 15 for a Common one (64 in all); a flow repair for
    // each of the 2 sections.
    assert_eq!(
        plan.ceiling(3),
        3 + (19 + 20 + 19 + 19 + 19 + 19) + (17 + 17 + 15 + 15) + 2
    );
}

#[test]
fn a_statement_the_ledger_has_part_way_costs_what_is_left_of_its_path() {
    let policy = StandardPolicy::new();
    let at = |class: Class, node: &str, reasked: bool, rewrites_left: u8| {
        PlannedAt::At(Position {
            class,
            visited: Classes::of(class),
            reasked,
            node: policy.graph(class).id_of(node).unwrap(),
            rewrites_left,
        })
    };
    let statements = [
        Planned {
            at: at(Class::Sourced, "s.recite", false, 2),
            ..alone(Class::Sourced)
        },
        Planned {
            at: at(Class::Common, "c.ground", false, 2),
            ..alone(Class::Common)
        },
        // A rewrite's successor, which has read once already and has no
        // rewrite left.
        Planned {
            at: at(Class::Reasoning, "r.final", true, 0),
            ..alone(Class::Reasoning)
        },
        ended(alone(Class::Common)),
    ];
    let plan = made(&policy, &statements, &[], 0);

    // The citation to move and the readings after it; a ground reading and
    // what follows it; the post-rewrite readings alone; a finished statement
    // costs nothing. A statement with its rewrites left holds the readings of
    // the successor a rewrite would give it too.
    assert_eq!(mosts(&plan), [17, 13, 5, 0]);
    assert_eq!(plan.ceiling(0), 17 + 13 + 5);
}

/// One way through the graphs: where it starts and the steps it takes, one
/// edge of a graph each.
struct Journey {
    start: Position,
    taken: Vec<Taken>,
}

fn entry(policy: &dyn AcceptancePolicy, class: Class) -> Position {
    start(policy, class)
}

/// A shortest way from each class's entry to every edge of every graph
/// (through the changes of class), found by following each node's edges, and
/// the way to spend the re-ask at every reading: the edge or the re-ask is the
/// last step of its journey.
fn journeys(policy: &dyn AcceptancePolicy) -> Vec<Journey> {
    let mut found = Vec::new();
    let mut covered: HashSet<(Class, &'static str, Outcome)> = HashSet::new();
    for class in CLASSES {
        let start = entry(policy, class);
        let mut seen: HashSet<Position> = HashSet::from([start]);
        let mut queue: VecDeque<(Position, Vec<Taken>)> = VecDeque::from([(start, Vec::new())]);
        while let Some((at, way)) = queue.pop_front() {
            let node = policy.graph(at.class).node(at.node).unwrap();
            let mut moves: Vec<Outcome> =
                node.edges().iter().map(|&(outcome, _)| outcome).collect();
            if matches!(node.step, Step::Read(_)) && !at.reasked {
                moves.push(Outcome::Unjudged);
            }
            for outcome in moves {
                let mut taken = way.clone();
                taken.push(Taken {
                    step: node.step,
                    outcome,
                });
                if outcome == Outcome::Unjudged || covered.insert((at.class, node.name, outcome)) {
                    found.push(Journey {
                        start,
                        taken: taken.clone(),
                    });
                }
                if let Walked::At(next) = walk_from(policy, start, &taken) {
                    if seen.insert(next) {
                        queue.push_back((next, taken));
                    }
                }
            }
        }
    }
    found
}

/// Where `journey` has a statement after its first `steps` steps.
fn place(policy: &dyn AcceptancePolicy, journey: &Journey, steps: usize) -> PlannedAt {
    if steps == 0 {
        return PlannedAt::Entry;
    }
    let taken = &journey.taken[..steps.min(journey.taken.len())];
    match walk_from(policy, journey.start, taken) {
        Walked::At(at) => PlannedAt::At(at),
        Walked::Ended(End::Removed { .. }) => PlannedAt::Ended(None),
        Walked::Ended(_) => {
            // The teaching review may still rewrite the sentence of a
            // statement that ended with a rewrite left.
            let (_, before) = taken.split_last().unwrap();
            let Walked::At(left) = walk_from(policy, journey.start, before) else {
                panic!("a journey ends from a place in the graph");
            };
            PlannedAt::Ended(next_rewrite(policy, stepped(policy, left)))
        }
        other => panic!("{other:?} after {taken:?}"),
    }
}

#[test]
fn the_journeys_take_every_edge_of_every_graph_at_least_once() {
    let policy = StandardPolicy::new();
    let mut covered: HashSet<(Class, &'static str, Outcome)> = HashSet::new();
    for journey in journeys(&policy) {
        let (last, before) = journey.taken.split_last().unwrap();
        if last.outcome == Outcome::Unjudged {
            continue;
        }
        let Walked::At(at) = walk_from(&policy, journey.start, before) else {
            panic!("a journey leaves the graph before its last step");
        };
        let node = policy.graph(at.class).node(at.node).unwrap();
        covered.insert((at.class, node.name, last.outcome));
    }
    for class in CLASSES {
        for node in policy.graph(class).nodes() {
            for &(outcome, _) in node.edges() {
                assert!(
                    covered.contains(&(class, node.name, outcome)),
                    "{} on {outcome:?}",
                    node.name
                );
            }
        }
    }
}

/// The calls a step taken from `at` makes: those of the node it leaves, or of
/// the node it reads again.
fn calls_of(policy: &dyn AcceptancePolicy, journey: &Journey, at: PlannedAt) -> u32 {
    let (class, node) = match at {
        PlannedAt::Entry => (
            journey.start.class,
            policy.graph(journey.start.class).entry(),
        ),
        PlannedAt::At(at) | PlannedAt::Narrowing(at) => (at.class, at.node),
        PlannedAt::Ended(_) => return 0,
    };
    policy.graph(class).node(node).map_or(0, |node| node.calls)
}

#[test]
fn moving_statements_one_graph_edge_at_a_time_never_raises_the_ceiling() {
    let policy = StandardPolicy::new();
    let journeys = journeys(&policy);
    let longest = journeys
        .iter()
        .map(|journey| journey.taken.len())
        .max()
        .unwrap();
    let after = |steps: usize| -> Vec<Planned> {
        journeys
            .iter()
            .map(|journey| Planned {
                class: journey.start.class,
                at: place(&policy, journey, steps),
            })
            .collect()
    };
    // The 5 sorting calls are made. Each step a statement takes makes the
    // calls of the node it leaves, which come out of what the plan holds for
    // it.
    let mut calls = 5;
    let mut plan = made(&policy, &after(0), &[], 4);
    let mut ceiling = plan.ceiling(calls);
    assert!(ceiling > 0);
    for steps in 1..=longest {
        let (now, held) = (after(steps), mosts(&plan));
        for journey in &journeys {
            if journey.taken.len() >= steps {
                calls += calls_of(&policy, journey, place(&policy, journey, steps - 1));
            }
        }
        plan.moved(&policy, &now);
        // The number a statement is held at is the graph's own for where it
        // now is, which is never more than it was: the plan never had to hold
        // a number back.
        for (index, statement) in now.iter().enumerate() {
            let graph_says = most(&policy, statement.class, statement.at);
            assert!(graph_says <= held[index], "step {steps}, statement {index}");
            assert_eq!(
                mosts(&plan)[index],
                graph_says,
                "step {steps}, statement {index}"
            );
        }
        let next = plan.ceiling(calls);
        assert!(next <= ceiling, "step {steps}: {next} after {ceiling}");
        ceiling = next;
    }
    // Every journey is over or at its last step; the ceiling is the calls
    // made and what is left.
    assert_eq!(
        plan.ceiling(calls),
        calls + 4 + mosts(&plan).iter().sum::<u32>(),
        "no reserve is held where no statement shares a sentence"
    );
}

#[test]
fn a_statement_that_reads_again_for_an_answer_it_could_not_read_costs_no_more() {
    let policy = StandardPolicy::new();
    let at = entry(&policy, Class::Sourced);
    let mut plan = made(&policy, &[alone(Class::Sourced)], &[], 0);
    let before = plan.ceiling(0);

    let reasked = Position {
        reasked: true,
        ..at
    };
    plan.moved(
        &policy,
        &[Planned {
            at: PlannedAt::At(reasked),
            ..alone(Class::Sourced)
        }],
    );

    // The reading that could not be read was the node's one call.
    assert_eq!(before, 19);
    assert_eq!(plan.ceiling(1), 17);
}

#[test]
fn a_groups_reserves_are_released_when_the_whole_sentence_has_ended_and_never_come_back() {
    let policy = StandardPolicy::new();
    let statements = [
        alone(Class::Sourced),
        alone(Class::Reasoning),
        alone(Class::Common),
        alone(Class::Common),
    ];
    let mut plan = made(&policy, &statements, &[Some(0), Some(0), Some(0), None], 0);
    assert_eq!(reserves(&plan), [17, 17, 15, 0]);
    assert_eq!(plan.ceiling(0), (19 + 20 + 19 + 19) + (17 + 17 + 15));

    // Two of the three end. Every reserve stays: the one that has not ended
    // can still rewrite the sentence, and itself waits to be read again.
    plan.moved(
        &policy,
        &[
            ended(statements[0]),
            ended(statements[1]),
            statements[2],
            statements[3],
        ],
    );
    assert_eq!(reserves(&plan), [17, 17, 15, 0]);
    assert_eq!(plan.ceiling(0), (19 + 19) + (17 + 17 + 15));

    // The last of them ends: the whole sentence has, and nothing is reserved.
    plan.moved(
        &policy,
        &[
            ended(statements[0]),
            ended(statements[1]),
            ended(statements[2]),
            statements[3],
        ],
    );
    assert_eq!(reserves(&plan), [0, 0, 0, 0]);
    assert_eq!(plan.ceiling(0), 19);

    // Shown at their entries again, they cost what that costs, a rise the
    // ceiling shows, and no reserve comes back to pay for it.
    plan.moved(&policy, &statements);
    assert_eq!(reserves(&plan), [0, 0, 0, 0]);
    assert_eq!(mosts(&plan), [19, 20, 19, 19]);
}

#[test]
fn a_statement_that_has_not_ended_keeps_its_reserve_though_every_neighbour_had_when_the_plan_was_made(
) {
    let policy = StandardPolicy::new();
    let statements = [ended(alone(Class::Sourced)), alone(Class::Reasoning)];
    let plan = made(&policy, &statements, &[Some(0), Some(0)], 0);

    // The finished Sourced statement is read again if the Reasoning one is
    // rewritten, and the Reasoning one has not ended itself.
    assert_eq!(reserves(&plan), [17, 17]);
    assert_eq!(mosts(&plan), [0, 20]);

    let both_ended = [ended(alone(Class::Sourced)), ended(alone(Class::Reasoning))];
    assert_eq!(
        reserves(&made(&policy, &both_ended, &[Some(0), Some(0)], 0)),
        [0, 0]
    );
}

#[test]
fn a_statement_the_plan_does_not_hold_is_ignored() {
    let policy = StandardPolicy::new();
    let mut plan = made(&policy, &[alone(Class::Sourced)], &[], 0);

    plan.moved(
        &policy,
        &[ended(alone(Class::Sourced)), ended(alone(Class::Common))],
    );

    assert_eq!(plan.ceiling(0), 0);
    assert_eq!(plan.slots.len(), 1);
    // Fewer statements than the plan holds leaves the rest where they were.
    let mut plan = made(
        &policy,
        &[alone(Class::Sourced), alone(Class::Common)],
        &[],
        0,
    );
    plan.moved(&policy, &[ended(alone(Class::Sourced))]);
    assert_eq!(mosts(&plan), [0, 19]);
}

#[test]
fn a_place_that_costs_more_than_a_statement_held_is_a_rise_that_is_shown_not_clamped() {
    let policy = StandardPolicy::new();
    let mut plan = made(&policy, &[alone(Class::Common)], &[], 0);
    let cut = at_node(&policy, Class::Common, "c.cut", 0);

    plan.moved(&policy, &[moved_to(alone(Class::Common), cut)]);
    assert_eq!(plan.ceiling(0), 0);

    // It has no reserve, so shown at its entry again it costs what the entry
    // costs, and the ceiling says so.
    plan.moved(&policy, &[alone(Class::Common)]);
    assert_eq!(plan.ceiling(0), 19);
}

#[test]
fn a_passed_statements_successor_reads_again_from_its_reserve_and_the_ceiling_does_not_rise() {
    let policy = StandardPolicy::new();
    let reasoning = alone(Class::Reasoning);
    let sourced = alone(Class::Sourced);
    let mut plan = made(&policy, &[reasoning, sourced], &[Some(0), Some(0)], 0);

    // The Reasoning statement passes while its neighbour is still to be read.
    plan.moved(&policy, &[ended(reasoning), sourced]);
    assert_eq!(mosts(&plan), [0, 19]);
    assert_eq!(reserves(&plan), [17, 17]);
    let before = plan.ceiling(0);

    // The neighbour is rewritten, so the Reasoning statement's successor
    // reads from the post-rewrite node, its re-ask unspent: 8 calls, drawn
    // from the 17 held for it. What the statement is held for stays 17.
    let reading = moved_to(reasoning, at_node(&policy, Class::Reasoning, "r.final", 0));
    let rewritten = moved_to(sourced, at_node(&policy, Class::Sourced, "s.final", 0));
    plan.moved(&policy, &[reading, rewritten]);
    assert_eq!(mosts(&plan), [8, 8]);
    assert_eq!(reserves(&plan), [9, 17]);
    assert_eq!(plan.ceiling(0), (8 + 9) + (8 + 17));
    assert!(plan.ceiling(0) <= before);

    // The successor reads on: what it may still cost falls, and the rest of
    // the reserve stays for the neighbour's narrowing.
    let reveto = moved_to(reasoning, at_node(&policy, Class::Reasoning, "r.reveto", 0));
    plan.moved(&policy, &[reveto, rewritten]);
    assert_eq!(mosts(&plan), [7, 8]);
    assert_eq!(reserves(&plan), [9, 17]);

    // Both end, and with them the sentence.
    plan.moved(&policy, &[ended(reasoning), ended(sourced)]);
    assert_eq!(plan.ceiling(0), 0);
}

#[test]
fn a_reserve_is_held_while_the_statements_own_successor_reads_though_its_neighbour_has_ended() {
    let policy = StandardPolicy::new();
    let reasoning = alone(Class::Reasoning);
    let sourced = alone(Class::Sourced);
    let mut plan = made(&policy, &[reasoning, sourced], &[Some(0), Some(0)], 0);
    plan.moved(&policy, &[ended(reasoning), sourced]);

    // The neighbour is rewritten and passes; the Reasoning statement's
    // successor is still to read.
    let reading = moved_to(reasoning, at_node(&policy, Class::Reasoning, "r.final", 0));
    plan.moved(&policy, &[reading, ended(sourced)]);

    // The neighbour's end releases nothing: the statement has not ended, so
    // its reserve (less what the successor drew) and the neighbour's stay.
    assert_eq!(mosts(&plan), [8, 0]);
    assert_eq!(reserves(&plan), [9, 17]);

    // The successor ends, the sentence is over, and nothing is held.
    plan.moved(&policy, &[ended(reasoning), ended(sourced)]);
    assert_eq!(reserves(&plan), [0, 0]);
    assert_eq!(plan.ceiling(0), 0);
}

#[test]
fn two_re_readings_in_a_row_each_with_the_re_ask_unspent_are_paid_from_the_reserve_without_a_rise()
{
    let policy = StandardPolicy::new();
    let reasoning = alone(Class::Reasoning);
    let sourced = alone(Class::Sourced);
    let mut plan = made(&policy, &[reasoning, sourced], &[Some(0), Some(0)], 0);
    let reading = moved_to(reasoning, at_node(&policy, Class::Reasoning, "r.final", 0));
    let rewritten = moved_to(sourced, at_node(&policy, Class::Sourced, "s.final", 0));
    let mut ceilings = Vec::new();
    let mut step = |plan: &mut CallPlan, statements: [Planned; 2]| {
        plan.moved(&policy, &statements);
        ceilings.push(plan.ceiling(0));
        (plan.slots[0].most, plan.slots[0].reserve)
    };

    // The Reasoning statement passes; its neighbour is rewritten, and the
    // statement's successor reads from the post-rewrite node, its re-ask
    // unspent: 8 of the 17 held for it.
    step(&mut plan, [ended(reasoning), sourced]);
    assert_eq!(step(&mut plan, [reading, rewritten]), (8, 9));
    // The successor passes and the neighbour is narrowed: the next successor
    // starts with the re-ask unspent again, and the 9 left pay for its 8.
    assert_eq!(step(&mut plan, [ended(reasoning), rewritten]), (0, 9));
    assert_eq!(step(&mut plan, [reading, rewritten]), (8, 1));
    // Both end, and with the sentence the reserve.
    assert_eq!(step(&mut plan, [ended(reasoning), ended(sourced)]), (0, 0));

    // Neither re-reading raises the ceiling: the statement is held for 17
    // while its first successor reads (8 + 9), for 9 once that has passed
    // (0 + 9), and for 9 again while the second reads (8 + 1).
    assert_eq!(ceilings, [53, 42, 34, 34, 0]);
    assert!(ceilings.windows(2).all(|pair| pair[1] <= pair[0]));
}

/// A policy whose every class reads once and ends, and reads once more after
/// a rewrite: for `sourced_after` calls in Sourced, for one call elsewhere. A
/// read here is a plain step, with no re-ask to add to its cost.
struct Reads {
    graphs: Vec<Graph>,
}

fn reads(sourced_after: u32) -> Reads {
    let work = |name: &'static str, calls: u32| {
        Node::new(name, Step::Cut, calls).otherwise(Next::End(End::Removed {
            section_goes: false,
        }))
    };
    let graphs = CLASSES
        .iter()
        .map(|&class| {
            let after = if class == Class::Sourced {
                sourced_after
            } else {
                1
            };
            Graph::new(class, "read", "again")
                .with(work("read", 1))
                .with(work("again", after))
        })
        .collect();
    Reads { graphs }
}

impl AcceptancePolicy for Reads {
    fn version(&self) -> &str {
        "reads"
    }

    fn graph(&self, class: Class) -> &Graph {
        let known = CLASSES.iter().position(|&each| each == class).unwrap();
        &self.graphs[known]
    }
}

#[test]
fn a_re_reading_beyond_the_reserve_is_a_rise_that_is_shown_not_clamped() {
    // A Reasoning statement is held a reserve of 2 × 1 + 1 = 3. The evidence
    // moved it to Sourced, whose post-rewrite reading costs 10.
    let policy = reads(10);
    let first = alone(Class::Reasoning);
    let second = alone(Class::Reasoning);
    let mut plan = made(&policy, &[first, second], &[Some(0), Some(0)], 0);
    assert_eq!(reserves(&plan), [3, 3]);
    plan.moved(&policy, &[ended(first), second]);
    let before = plan.ceiling(0);
    assert_eq!(before, 1 + (3 + 3));

    let sourced = Position {
        visited: Classes::of(Class::Sourced).with(Class::Reasoning),
        ..at_node(&policy, Class::Sourced, "again", 0)
    };
    plan.moved(&policy, &[moved_to(first, sourced), second]);

    // The reserve pays 3 of the 10. The other 7 are a rise, and the ceiling
    // goes up by them.
    assert_eq!(plan.slots[0].class, Class::Sourced);
    assert_eq!(mosts(&plan), [10, 1]);
    assert_eq!(reserves(&plan), [0, 3]);
    assert_eq!(plan.ceiling(0), before + 7);
}

#[test]
fn a_statement_the_evidence_moved_to_another_class_is_costed_there_and_keeps_the_reserve_it_was_sized_with(
) {
    let policy = StandardPolicy::new();
    let first = alone(Class::Common);
    let second = alone(Class::Common);
    let mut plan = made(&policy, &[first, second], &[Some(0), Some(0)], 0);
    assert_eq!(reserves(&plan), [15, 15]);

    // Sent to its source: Sourced's entry, the class changed.
    let sourced = Position {
        visited: Classes::of(Class::Sourced).with(Class::Common),
        ..at_node(&policy, Class::Sourced, "s.read", 2)
    };
    plan.moved(&policy, &[moved_to(first, sourced), second]);

    assert_eq!(plan.slots[0].class, Class::Sourced);
    assert_eq!(mosts(&plan), [16, 19]);
    assert_eq!(reserves(&plan), [15, 15]);
}

#[test]
fn a_statement_the_ledger_holds_in_another_class_but_has_not_moved_stays_at_the_entry_of_its_own() {
    let policy = StandardPolicy::new();
    let mut plan = made(&policy, &[alone(Class::Reasoning)], &[], 0);

    // Held as Sourced after a later sort, with no step taken: no move, and
    // no rise from Reasoning's entry to Sourced's.
    let held_as_sourced = entry(&policy, Class::Sourced);
    plan.moved(
        &policy,
        &[moved_to(alone(Class::Reasoning), held_as_sourced)],
    );

    assert_eq!(plan.slots[0].class, Class::Reasoning);
    assert_eq!(plan.ceiling(0), 20);
}

#[test]
fn the_cuts_known_bring_the_flow_repairs_to_the_sections_that_took_one_and_never_raise_them() {
    let policy = StandardPolicy::new();
    let mut plan = made(
        &policy,
        &[alone(Class::Sourced), alone(Class::Common)],
        &[],
        3,
    );
    let before = plan.ceiling(7);
    assert_eq!(before, 7 + 19 + 19 + 3);

    // Of the lesson's three sections that could take cuts, one did.
    plan.cuts_known(1);
    assert_eq!(plan.ceiling(7), before - 2);

    // Knowing them again, or more of them, changes nothing.
    plan.cuts_known(1);
    plan.cuts_known(3);
    assert_eq!(plan.ceiling(7), before - 2);

    // A lesson with no cut has no flow repair.
    plan.cuts_known(0);
    assert_eq!(plan.ceiling(7), before - 3);
}

#[test]
fn a_flow_repair_uses_up_the_call_held_for_it_so_its_call_made_leaves_the_ceiling_where_it_was() {
    let policy = StandardPolicy::new();
    let statements = [ended(alone(Class::Sourced)), ended(alone(Class::Common))];
    let mut plan = made(&policy, &statements, &[], 2);
    let before = plan.ceiling(5);
    assert_eq!(before, 5 + 2);

    // The first section's repair makes its call: one more made, one less held.
    plan.flow_repaired();
    assert_eq!(plan.ceiling(6), before);

    // The second finds no gap and makes none: the ceiling comes down.
    plan.flow_repaired();
    assert_eq!(plan.ceiling(6), before - 1);

    // None is held beyond the sections that took a cut.
    plan.flow_repaired();
    assert_eq!(plan.ceiling(6), before - 1);
}

#[test]
fn a_statement_taken_in_adds_its_share_and_beside_others_the_reserves_a_rewrite_of_their_sentence_needs(
) {
    let policy = StandardPolicy::new();
    let mut plan = made(
        &policy,
        &[alone(Class::Reasoning), alone(Class::Common)],
        &[],
        1,
    );
    let before = plan.ceiling(3);
    let joining = |planned, beside| Joining { planned, beside };

    // Alone on its sentence it brings its longest path from its entry.
    assert_eq!(
        plan.take_in(&policy, &[joining(alone(Class::Common), vec![])]),
        19
    );
    assert_eq!(plan.ceiling(3), before + 19);
    assert_eq!(reserves(&plan), [0, 0, 0]);

    // Beside the Reasoning statement it brings its reserve too, and the
    // Reasoning statement, which held none, gets its own: either one's rewrite
    // has the other read again.
    assert_eq!(
        plan.take_in(&policy, &[joining(alone(Class::Sourced), vec![0])]),
        19 + 17 + 17
    );
    assert_eq!(reserves(&plan), [17, 0, 0, 17]);

    // The two are one sentence now: one ending releases nothing, both do.
    let common = alone(Class::Common);
    plan.moved(
        &policy,
        &[
            ended(alone(Class::Reasoning)),
            common,
            common,
            alone(Class::Sourced),
        ],
    );
    assert_eq!(reserves(&plan), [17, 0, 0, 17]);
    plan.moved(
        &policy,
        &[
            ended(alone(Class::Reasoning)),
            common,
            common,
            ended(alone(Class::Sourced)),
        ],
    );
    assert_eq!(reserves(&plan), [0, 0, 0, 0]);
}

#[test]
fn a_statement_taken_in_beside_statements_of_two_groups_makes_them_one() {
    let policy = StandardPolicy::new();
    let statements = [
        alone(Class::Sourced),
        alone(Class::Common),
        alone(Class::Sourced),
        alone(Class::Common),
    ];
    let mut plan = made(
        &policy,
        &statements,
        &[Some(0), Some(0), Some(1), Some(1)],
        0,
    );
    let groups = |plan: &CallPlan| -> Vec<Option<u32>> {
        plan.slots.iter().map(|slot| slot.group).collect()
    };
    assert_eq!(groups(&plan)[0], groups(&plan)[1]);
    assert_ne!(groups(&plan)[0], groups(&plan)[2]);

    // It stands with one of each: the four stand on one sentence, directly or
    // through it.
    let joining = Joining {
        planned: alone(Class::Reasoning),
        beside: vec![0, 2],
    };
    plan.take_in(&policy, &[joining]);

    let joined = groups(&plan);
    assert!(joined[0].is_some());
    assert!(joined.iter().all(|group| *group == joined[0]), "{joined:?}");
}

#[test]
fn a_cut_that_narrows_its_sentence_holds_the_narrowings_call() {
    let policy = StandardPolicy::new();
    let target = alone(Class::Reasoning);
    let mate = alone(Class::Reasoning);
    let mut plan = made(&policy, &[target, mate], &[Some(0), Some(0)], 0);
    let reading = moved_to(mate, at_node(&policy, Class::Reasoning, "r.final", 0));
    let narrowing = Planned {
        at: PlannedAt::Narrowing(at_node(&policy, Class::Reasoning, "r.cut", 0)),
        ..target
    };

    // The target waits at a cut that narrows the sentence while its mate's
    // successor reads: one call is still to come for the narrowing.
    plan.moved(&policy, &[narrowing, reading]);
    assert_eq!(mosts(&plan), [1, 8]);
    let before = plan.ceiling(0);

    // The narrowing is made and the target is cut: its call came out of what
    // was held for it, and the ceiling stays where it was.
    plan.moved(&policy, &[ended(target), reading]);
    assert_eq!(plan.ceiling(1), before);
}

#[test]
fn the_ledgers_walk_is_where_the_plan_puts_a_statement() {
    let policy = StandardPolicy::new();
    let at = entry(&policy, Class::Reasoning);
    assert_eq!(PlannedAt::of(Some(Walked::At(at))), PlannedAt::At(at));
    for walked in [Walked::Ended(End::Consistent), Walked::Stopped] {
        assert_eq!(PlannedAt::of(Some(walked)), PlannedAt::Ended(None));
    }
    // Nothing known, or a record the graph does not fit: no move on a guess.
    let taken = Taken {
        step: Step::Cut,
        outcome: Outcome::Cut,
    };
    assert_eq!(PlannedAt::of(None), PlannedAt::Entry);
    assert_eq!(
        PlannedAt::of(Some(Walked::Diverged { at, taken })),
        PlannedAt::Entry
    );
}
