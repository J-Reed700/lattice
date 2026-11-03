#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;
use lattice_core::domain::learning::acceptance::{
    walk_from, End, Outcome, StandardPolicy, Step, Taken, CLASSES,
};
use std::collections::{HashSet, VecDeque};

fn alone(class: Class) -> Planned {
    Planned {
        class,
        at: PlannedAt::Entry,
        sentence: None,
    }
}

fn on_sentence(class: Class, sentence: u32) -> Planned {
    Planned {
        sentence: Some(sentence),
        ..alone(class)
    }
}

fn ended(statement: Planned) -> Planned {
    Planned {
        at: PlannedAt::Ended,
        ..statement
    }
}

fn mosts(plan: &CallPlan) -> Vec<u32> {
    plan.slots.iter().map(|slot| slot.most).collect()
}

fn reserves(plan: &CallPlan) -> Vec<u32> {
    plan.slots.iter().map(|slot| slot.reserve).collect()
}

#[test]
fn the_ceiling_at_the_sort_is_the_sorting_calls_each_longest_path_the_reserves_and_the_flow_repairs(
) {
    let policy = StandardPolicy::new();
    // A Sourced and a Reasoning statement share a sentence, two Common ones
    // share another, and a Sourced and a Common one stand alone.
    let statements = [
        on_sentence(Class::Sourced, 0),
        on_sentence(Class::Reasoning, 0),
        alone(Class::Sourced),
        on_sentence(Class::Common, 1),
        on_sentence(Class::Common, 1),
        alone(Class::Common),
    ];
    let plan = CallPlan::new(&policy, 3, &statements, 2);

    assert_eq!(mosts(&plan), [16, 15, 16, 16, 16, 16]);
    assert_eq!(reserves(&plan), [11, 11, 0, 9, 9, 0]);
    // The 3 sorting calls; the longest path from each entry, 16 for a
    // Sourced or a Common statement and 15 for a Reasoning one (95 in all);
    // the reserve of each of the four that share a sentence, two readings
    // from the post-rewrite node and the narrowing call, 2 × 5 + 1 = 11 for
    // the Sourced and the Reasoning one and 2 × 4 + 1 = 9 for a Common one
    // (40 in all); a flow repair for each of the 2 sections.
    assert_eq!(
        plan.ceiling(),
        3 + (16 + 15 + 16 + 16 + 16 + 16) + (11 + 11 + 9 + 9) + 2
    );
}

#[test]
fn a_statement_the_ledger_has_part_way_costs_what_is_left_of_its_path() {
    let policy = StandardPolicy::new();
    let at = |class: Class, node: &str, reasked: bool| {
        PlannedAt::At(Position {
            class,
            changed_class: false,
            reasked,
            node: policy.graph(class).id_of(node).unwrap(),
        })
    };
    let statements = [
        Planned {
            at: at(Class::Sourced, "s.recite", false),
            ..alone(Class::Sourced)
        },
        Planned {
            at: at(Class::Common, "c.ground", false),
            ..alone(Class::Common)
        },
        // A rewrite's successor, which has read once already.
        Planned {
            at: at(Class::Reasoning, "r.final", true),
            ..alone(Class::Reasoning)
        },
        ended(alone(Class::Common)),
    ];
    let plan = CallPlan::new(&policy, 0, &statements, 0);

    // The citation to move and the readings after it; a ground reading and
    // what follows it; the post-rewrite readings alone; a finished statement
    // costs nothing.
    assert_eq!(mosts(&plan), [14, 10, 5, 0]);
    assert_eq!(plan.ceiling(), 14 + 10 + 5);
}

/// One way through the graphs: where it starts and the steps it takes, one
/// edge of a graph each.
struct Journey {
    start: Position,
    taken: Vec<Taken>,
}

fn entry(policy: &dyn AcceptancePolicy, class: Class) -> Position {
    Position {
        class,
        changed_class: false,
        reasked: false,
        node: policy.graph(class).entry(),
    }
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
        Walked::Ended(_) => PlannedAt::Ended,
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
                sentence: None,
            })
            .collect()
    };
    let mut plan = CallPlan::new(&policy, 5, &after(0), 4);
    assert!(plan.ceiling() > 0);
    for steps in 1..=longest {
        let (before, now) = (plan.ceiling(), after(steps));
        let held = mosts(&plan);
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
        assert!(plan.ceiling() <= before, "step {steps}");
    }
    // Every journey is over or at its last step; the ceiling is what is left.
    assert_eq!(
        plan.ceiling(),
        5 + 4 + mosts(&plan).iter().sum::<u32>(),
        "no reserve is held where no statement shares a sentence"
    );
}

#[test]
fn a_statement_that_reads_again_for_an_answer_it_could_not_read_costs_no_more() {
    let policy = StandardPolicy::new();
    let at = entry(&policy, Class::Sourced);
    let mut plan = CallPlan::new(&policy, 0, &[alone(Class::Sourced)], 0);
    let before = plan.ceiling();

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

    assert_eq!(before, 16);
    assert_eq!(plan.ceiling(), 13);
}

#[test]
fn a_groups_reserves_are_released_as_its_other_statements_end_and_never_come_back() {
    let policy = StandardPolicy::new();
    let statements = [
        on_sentence(Class::Sourced, 0),
        on_sentence(Class::Reasoning, 0),
        on_sentence(Class::Common, 0),
        alone(Class::Common),
    ];
    let mut plan = CallPlan::new(&policy, 0, &statements, 0);
    assert_eq!(reserves(&plan), [11, 11, 9, 0]);
    assert_eq!(plan.ceiling(), (16 + 15 + 16 + 16) + (11 + 11 + 9));

    // The Sourced statement ends. Every reserve stays: each of the three
    // still has a neighbour that has not ended.
    plan.moved(
        &policy,
        &[
            ended(statements[0]),
            statements[1],
            statements[2],
            statements[3],
        ],
    );
    assert_eq!(reserves(&plan), [11, 11, 9, 0]);
    assert_eq!(plan.ceiling(), (15 + 16 + 16) + (11 + 11 + 9));

    // The Reasoning one ends too. The Common one has no neighbour left that
    // has not ended, so its reserve goes; the other two keep theirs.
    plan.moved(
        &policy,
        &[
            ended(statements[0]),
            ended(statements[1]),
            statements[2],
            statements[3],
        ],
    );
    assert_eq!(reserves(&plan), [11, 11, 0, 0]);
    assert_eq!(plan.ceiling(), (16 + 16) + (11 + 11));

    // The last of them ends: nothing is reserved.
    let all_over = [
        ended(statements[0]),
        ended(statements[1]),
        ended(statements[2]),
        statements[3],
    ];
    plan.moved(&policy, &all_over);
    assert_eq!(reserves(&plan), [0, 0, 0, 0]);
    assert_eq!(plan.ceiling(), 16);

    // A statement shown at its entry again does not bring anything back.
    plan.moved(&policy, &statements);
    assert_eq!(reserves(&plan), [0, 0, 0, 0]);
    assert_eq!(mosts(&plan), [0, 0, 0, 16]);
    assert_eq!(plan.ceiling(), 16);
}

#[test]
fn a_statement_alone_on_its_sentence_reserves_nothing_and_none_joins_a_group_later() {
    let policy = StandardPolicy::new();
    let mut plan = CallPlan::new(
        &policy,
        0,
        &[alone(Class::Sourced), alone(Class::Common)],
        0,
    );
    assert_eq!(reserves(&plan), [0, 0]);
    let before = plan.ceiling();

    plan.moved(
        &policy,
        &[
            on_sentence(Class::Sourced, 4),
            on_sentence(Class::Common, 4),
        ],
    );

    assert_eq!(reserves(&plan), [0, 0]);
    assert_eq!(plan.ceiling(), before);
}

#[test]
fn a_finished_statement_keeps_its_reserve_while_a_neighbour_stands_and_the_neighbour_holds_none() {
    let policy = StandardPolicy::new();
    let statements = [
        ended(on_sentence(Class::Sourced, 0)),
        on_sentence(Class::Reasoning, 0),
    ];
    let plan = CallPlan::new(&policy, 0, &statements, 0);

    // The finished Sourced statement is read again if the Reasoning one is
    // rewritten; nothing can rewrite the Reasoning one's sentence but itself.
    assert_eq!(reserves(&plan), [11, 0]);
    assert_eq!(mosts(&plan), [0, 15]);
}

#[test]
fn a_statement_the_plan_does_not_hold_is_ignored() {
    let policy = StandardPolicy::new();
    let mut plan = CallPlan::new(&policy, 0, &[alone(Class::Sourced)], 0);

    plan.moved(
        &policy,
        &[ended(alone(Class::Sourced)), ended(alone(Class::Common))],
    );

    assert_eq!(plan.ceiling(), 0);
    assert_eq!(plan.slots.len(), 1);
    // Fewer statements than the plan holds leaves the rest where they were.
    let mut plan = CallPlan::new(
        &policy,
        0,
        &[alone(Class::Sourced), alone(Class::Common)],
        0,
    );
    plan.moved(&policy, &[ended(alone(Class::Sourced))]);
    assert_eq!(mosts(&plan), [0, 16]);
}

#[test]
fn a_number_held_never_rises_when_a_statement_is_shown_at_a_place_that_costs_more() {
    let policy = StandardPolicy::new();
    let mut plan = CallPlan::new(&policy, 0, &[alone(Class::Common)], 0);
    let start = entry(&policy, Class::Common);
    let cut = Position {
        node: policy.graph(Class::Common).id_of("c.cut").unwrap(),
        ..start
    };

    plan.moved(
        &policy,
        &[Planned {
            at: PlannedAt::At(cut),
            ..alone(Class::Common)
        }],
    );
    assert_eq!(plan.ceiling(), 0);
    for again in [PlannedAt::Entry, PlannedAt::At(start)] {
        plan.moved(
            &policy,
            &[Planned {
                at: again,
                ..alone(Class::Common)
            }],
        );
        assert_eq!(plan.ceiling(), 0);
    }
}

#[test]
fn the_cuts_known_bring_the_flow_repairs_to_the_sections_that_took_one_and_never_raise_them() {
    let policy = StandardPolicy::new();
    let mut plan = CallPlan::new(
        &policy,
        7,
        &[alone(Class::Sourced), alone(Class::Common)],
        3,
    );
    let before = plan.ceiling();
    assert_eq!(before, 7 + 16 + 16 + 3);

    // Of the lesson's three sections that could take cuts, one did.
    plan.cuts_known(1);
    assert_eq!(plan.ceiling(), before - 2);

    // Knowing them again, or more of them, changes nothing.
    plan.cuts_known(1);
    plan.cuts_known(3);
    assert_eq!(plan.ceiling(), before - 2);

    // A lesson with no cut has no flow repair.
    plan.cuts_known(0);
    assert_eq!(plan.ceiling(), before - 3);
}

#[test]
fn the_ledgers_walk_is_where_the_plan_puts_a_statement() {
    let policy = StandardPolicy::new();
    let at = entry(&policy, Class::Reasoning);
    assert_eq!(PlannedAt::of(Some(Walked::At(at))), PlannedAt::At(at));
    for walked in [Walked::Ended(End::Consistent), Walked::Stopped] {
        assert_eq!(PlannedAt::of(Some(walked)), PlannedAt::Ended);
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
