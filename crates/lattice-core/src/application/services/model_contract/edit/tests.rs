use proptest::prelude::*;

use super::super::shown::{Drawn, Scope};
use super::*;

#[derive(Debug, Clone, PartialEq)]
struct Thing {
    index: usize,
}

impl Kind for Thing {
    const PREFIX: &'static str = "t";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.index
    }
}

fn name(index: usize) -> String {
    format!("t{}", index + 1)
}

/// `t1..tn` as a request shows them, resolved the way a reply's pointers are.
fn picks(count: usize) -> Vec<Picked<Thing>> {
    let things: Vec<Thing> = (0..count).map(|index| Thing { index }).collect();
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    showing.show(&things).expect("things are shown");
    let picked: Vec<Picked<Thing>> = (1..=count)
        .map(|ordinal| {
            showing
                .resolve::<Thing>(&format!("t{ordinal}"))
                .expect("a shown thing resolves")
        })
        .collect();
    picked
}

fn nothing() -> Change<Thing, String> {
    Change {
        replace: Vec::new(),
        insert: Vec::new(),
        remove: Vec::new(),
        moved: Vec::new(),
    }
}

fn after(thing: &Picked<Thing>) -> Anchor<Thing> {
    Anchor::After(thing.clone())
}

type Doc = Edited<Thing, String>;

fn edited(count: usize) -> Doc {
    Edited::of((0..count).collect())
}

fn order(doc: &Doc) -> Vec<String> {
    doc.places()
        .iter()
        .map(|place| match &place.laid {
            Laid::Kept(thing) => name(**thing),
            Laid::Wrote { content, .. } => (*content).clone(),
        })
        .collect()
}

/// Every place with what it holds, for comparing two results.
fn snapshot(doc: &Doc) -> Vec<(Option<usize>, String)> {
    doc.places()
        .iter()
        .map(|place| {
            let held = match &place.laid {
                Laid::Kept(thing) => name(**thing),
                Laid::Wrote { content, replaces } => match replaces {
                    Some(thing) => format!("{content}<{}", name(**thing)),
                    None => (*content).clone(),
                },
            };
            (place.at, held)
        })
        .collect()
}

fn run(doc: &mut Doc, change: Change<Thing, String>) -> Settled<Applied<Thing>> {
    doc.apply(consistent(change).expect("the change is consistent"))
}

fn applied(settled: Settled<Applied<Thing>>) -> Applied<Thing> {
    match settled {
        Settled::Answered(applied) => applied,
        Settled::Unanswered(why) => panic!("expected the change to apply, got {why:?}"),
    }
}

fn refusal(settled: Settled<Applied<Thing>>) -> String {
    match settled {
        Settled::Unanswered(Why::Contradicted(text)) => text,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn handles(things: &[Picked<Thing>]) -> Vec<String> {
    things.iter().map(|thing| thing.handle.written()).collect()
}

fn text(value: &str) -> String {
    value.to_owned()
}

#[test]
fn an_empty_change_applies_and_changes_nothing() {
    let mut doc = edited(3);
    let result = applied(run(&mut doc, nothing()));
    assert_eq!(result.wrote, 0);
    assert!(result.claimed.is_empty());
    assert_eq!(snapshot(&doc), snapshot(&edited(3)));
}

#[test]
fn a_replaced_thing_is_its_content_where_it_was() {
    let p = picks(3);
    let mut doc = edited(3);
    let mut change = nothing();
    change.replace.push((p[1].clone(), text("B")));
    let result = applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t1", "B", "t3"]);
    assert_eq!(result.wrote, 1);
    assert_eq!(handles(&result.claimed), ["t2"]);
    let places = doc.places();
    assert_eq!(places[1].at, Some(1));
    assert_eq!(
        places[1].laid,
        Laid::Wrote {
            content: &text("B"),
            replaces: Some(&1)
        }
    );
}

#[test]
fn a_removed_thing_leaves_nothing() {
    let p = picks(3);
    let mut doc = edited(3);
    let mut change = nothing();
    change.remove.push(p[1].clone());
    let result = applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t1", "t3"]);
    assert_eq!(result.wrote, 0);
    assert_eq!(handles(&result.claimed), ["t2"]);
}

#[test]
fn inserts_land_after_their_anchor_in_written_order() {
    let p = picks(3);
    let mut doc = edited(3);
    let mut change = nothing();
    change.insert.push((after(&p[0]), text("X")));
    change.insert.push((Anchor::Start, text("S")));
    change.insert.push((after(&p[0]), text("Y")));
    let result = applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["S", "t1", "X", "Y", "t2", "t3"]);
    assert_eq!(result.wrote, 3);
    assert!(result.claimed.is_empty());
}

#[test]
fn a_moved_thing_lands_after_its_anchor() {
    let p = picks(3);
    let mut doc = edited(3);
    let mut change = nothing();
    change.moved.push((p[0].clone(), after(&p[2])));
    let result = applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t2", "t3", "t1"]);
    assert_eq!(result.wrote, 0);
    assert_eq!(handles(&result.claimed), ["t1"]);

    let mut doc = edited(3);
    let mut change = nothing();
    change.moved.push((p[2].clone(), Anchor::Start));
    applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t3", "t1", "t2"]);
}

#[test]
fn an_insert_after_a_moved_thing_travels_with_it() {
    let p = picks(3);
    let mut doc = edited(3);
    let mut change = nothing();
    change.moved.push((p[0].clone(), after(&p[2])));
    change.insert.push((after(&p[0]), text("X")));
    applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t2", "t3", "t1", "X"]);
}

#[test]
fn an_insert_or_a_move_after_a_removed_thing_lands_where_it_was() {
    let p = picks(4);
    let mut doc = edited(4);
    let mut change = nothing();
    change.remove.push(p[1].clone());
    change.insert.push((after(&p[1]), text("X")));
    applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t1", "X", "t3", "t4"]);

    let mut doc = edited(4);
    let mut change = nothing();
    change.remove.push(p[1].clone());
    change.moved.push((p[3].clone(), after(&p[1])));
    applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t1", "t4", "t3"]);
}

#[test]
fn inserts_come_before_moves_after_the_same_thing() {
    let p = picks(4);
    let mut doc = edited(4);
    let mut change = nothing();
    change.moved.push((p[2].clone(), after(&p[0])));
    change.insert.push((after(&p[0]), text("X")));
    change.insert.push((after(&p[0]), text("Y")));
    applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t1", "X", "Y", "t3", "t2", "t4"]);
}

#[test]
fn moves_after_moved_things_follow_them() {
    let p = picks(4);
    let mut doc = edited(4);
    let mut change = nothing();
    change.moved.push((p[0].clone(), after(&p[2])));
    change.moved.push((p[1].clone(), after(&p[0])));
    applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t3", "t1", "t2", "t4"]);
}

#[test]
fn things_anchored_after_a_replaced_thing_follow_its_content() {
    let p = picks(3);
    let mut doc = edited(3);
    let mut change = nothing();
    change.replace.push((p[1].clone(), text("B")));
    change.insert.push((after(&p[1]), text("X")));
    let result = applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["t1", "B", "X", "t3"]);
    assert_eq!(result.wrote, 2);
}

#[test]
fn a_change_may_replace_remove_insert_and_move_at_once() {
    let p = picks(5);
    let mut doc = edited(5);
    let mut change = nothing();
    change.replace.push((p[0].clone(), text("A")));
    change.remove.push(p[1].clone());
    change.moved.push((p[2].clone(), after(&p[4])));
    change.insert.push((Anchor::Start, text("S")));
    let result = applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["S", "A", "t4", "t5", "t3"]);
    assert_eq!(result.wrote, 2);
    assert_eq!(handles(&result.claimed), ["t1", "t2", "t3"]);
}

#[test]
fn a_thing_claimed_twice_is_inconsistent_and_listed_once() {
    let p = picks(6);
    let mut change = nothing();
    change.replace.push((p[2].clone(), text("a")));
    change.replace.push((p[0].clone(), text("b")));
    change.remove.push(p[2].clone());
    change.moved.push((p[2].clone(), Anchor::Start));
    change.moved.push((p[0].clone(), after(&p[4])));
    let Err(contradiction) = consistent(change) else {
        panic!("expected a contradiction");
    };
    match &contradiction {
        Contradiction::ClaimedTwice(things) => assert_eq!(handles(things), ["t3", "t1"]),
        other => panic!("expected ClaimedTwice, got {other:?}"),
    }
    assert_eq!(contradiction.told(), "t3, t1 are changed more than once.");
}

#[test]
fn one_thing_claimed_twice_is_told_in_the_singular() {
    let p = picks(3);
    let mut change = nothing();
    change.remove.push(p[2].clone());
    change.remove.push(p[2].clone());
    let contradiction = consistent(change).unwrap_err();
    assert_eq!(contradiction.told(), "t3 is changed more than once.");
}

#[test]
fn inserts_never_claim() {
    let p = picks(3);
    let mut change = nothing();
    change.insert.push((after(&p[0]), text("X")));
    change.insert.push((after(&p[0]), text("X")));
    change.replace.push((p[0].clone(), text("A")));
    assert!(consistent(change).is_ok());
}

#[test]
fn two_things_moved_after_each_other_are_a_cycle() {
    let p = picks(6);
    let mut change = nothing();
    change.moved.push((p[2].clone(), after(&p[4])));
    change.moved.push((p[4].clone(), after(&p[2])));
    let contradiction = consistent(change).unwrap_err();
    assert_eq!(
        contradiction.told(),
        "t3 and t5 are each moved after the other."
    );
    assert!(matches!(contradiction, Contradiction::Cycle(_)));
}

#[test]
fn a_longer_cycle_names_every_thing_of_it_and_only_them() {
    let p = picks(6);
    let mut change = nothing();
    change.moved.push((p[5].clone(), after(&p[0])));
    change.moved.push((p[0].clone(), after(&p[1])));
    change.moved.push((p[1].clone(), after(&p[2])));
    change.moved.push((p[2].clone(), after(&p[0])));
    let Err(Contradiction::Cycle(things)) = consistent(change) else {
        panic!("expected a cycle");
    };
    assert_eq!(handles(&things), ["t1", "t2", "t3"]);
    assert_eq!(
        Contradiction::Cycle(things).told(),
        "t1, t2, t3 are moved after one another in a circle."
    );
}

#[test]
fn a_thing_moved_after_itself_is_a_cycle() {
    let p = picks(3);
    let mut change = nothing();
    change.moved.push((p[1].clone(), after(&p[1])));
    let contradiction = consistent(change).unwrap_err();
    assert_eq!(contradiction.told(), "t2 is moved after itself.");
}

#[test]
fn a_chain_of_moves_that_ends_is_consistent() {
    let p = picks(4);
    let mut change = nothing();
    change.moved.push((p[0].clone(), after(&p[1])));
    change.moved.push((p[1].clone(), after(&p[2])));
    change.moved.push((p[2].clone(), Anchor::Start));
    assert!(consistent(change).is_ok());
}

#[test]
fn a_later_change_that_claims_a_claimed_thing_is_refused_and_applies_nothing() {
    let p = picks(3);
    let mut doc = edited(3);
    let mut first = nothing();
    first.replace.push((p[1].clone(), text("B")));
    applied(run(&mut doc, first));
    let before = snapshot(&doc);

    let mut second = nothing();
    second.replace.push((p[1].clone(), text("again")));
    second.replace.push((p[0].clone(), text("A")));
    assert_eq!(
        refusal(run(&mut doc, second)),
        "t2 was already changed by an earlier edit."
    );
    assert_eq!(snapshot(&doc), before);

    let mut removal = nothing();
    removal.remove.push(p[1].clone());
    assert!(refusal(run(&mut doc, removal)).contains("t2"));
    let mut moving = nothing();
    moving.moved.push((p[1].clone(), Anchor::Start));
    assert!(refusal(run(&mut doc, moving)).contains("t2"));
    assert_eq!(snapshot(&doc), before);
}

#[test]
fn a_later_change_anchored_on_a_claimed_thing_is_refused() {
    let p = picks(3);
    let mut doc = edited(3);
    let mut first = nothing();
    first.remove.push(p[1].clone());
    first.moved.push((p[2].clone(), Anchor::Start));
    applied(run(&mut doc, first));
    let before = snapshot(&doc);

    let mut insert = nothing();
    insert.insert.push((after(&p[1]), text("X")));
    assert_eq!(
        refusal(run(&mut doc, insert)),
        "t2 was already changed by an earlier edit."
    );
    let mut moving = nothing();
    moving.moved.push((p[0].clone(), after(&p[2])));
    assert_eq!(
        refusal(run(&mut doc, moving)),
        "t3 was already changed by an earlier edit."
    );
    let mut both = nothing();
    both.insert.push((after(&p[1]), text("X")));
    both.remove.push(p[2].clone());
    assert_eq!(
        refusal(run(&mut doc, both)),
        "t3, t2 were already changed by earlier edits."
    );
    assert_eq!(snapshot(&doc), before);
}

#[test]
fn a_thing_the_target_does_not_hold_cannot_be_placed() {
    let p = picks(6);
    let mut doc = edited(3);
    let mut change = nothing();
    change.remove.push(p[0].clone());
    change.insert.push((after(&p[4]), text("X")));
    assert_eq!(refusal(run(&mut doc, change)), "t5 cannot be placed.");
    assert_eq!(snapshot(&doc), snapshot(&edited(3)));
}

#[test]
fn changes_that_only_insert_after_one_thing_both_apply_the_later_after_the_earlier() {
    let p = picks(3);
    let mut doc = edited(3);
    let mut first = nothing();
    first.insert.push((after(&p[0]), text("X")));
    applied(run(&mut doc, first));
    let mut second = nothing();
    second.insert.push((after(&p[0]), text("Y")));
    applied(run(&mut doc, second));
    assert_eq!(order(&doc), ["t1", "X", "Y", "t2", "t3"]);

    let mut at_start = nothing();
    at_start.insert.push((Anchor::Start, text("S1")));
    applied(run(&mut doc, at_start));
    let mut again = nothing();
    again.insert.push((Anchor::Start, text("S2")));
    applied(run(&mut doc, again));
    assert_eq!(order(&doc), ["S1", "S2", "t1", "X", "Y", "t2", "t3"]);
}

#[test]
fn a_later_move_after_the_same_thing_comes_after_the_earlier_one() {
    let p = picks(8);
    let mut doc = edited(8);
    let mut first = nothing();
    first.moved.push((p[6].clone(), after(&p[4])));
    applied(run(&mut doc, first));
    let mut second = nothing();
    second.moved.push((p[7].clone(), after(&p[4])));
    applied(run(&mut doc, second));
    assert_eq!(order(&doc), ["t1", "t2", "t3", "t4", "t5", "t7", "t8", "t6"]);
}

#[test]
fn what_an_earlier_change_anchored_after_a_thing_follows_it_when_a_later_change_moves_it() {
    let p = picks(5);
    let mut doc = edited(5);
    let mut first = nothing();
    first.insert.push((after(&p[4]), text("X")));
    applied(run(&mut doc, first));
    assert_eq!(order(&doc), ["t1", "t2", "t3", "t4", "t5", "X"]);
    let mut second = nothing();
    second.moved.push((p[4].clone(), Anchor::Start));
    applied(run(&mut doc, second));
    assert_eq!(order(&doc), ["t5", "X", "t1", "t2", "t3", "t4"]);
}

#[test]
fn a_change_may_claim_or_insert_after_a_thing_an_earlier_change_only_anchored_on() {
    let p = picks(4);
    let mut doc = edited(4);
    let mut first = nothing();
    first.insert.push((after(&p[1]), text("X")));
    first.moved.push((p[3].clone(), after(&p[1])));
    applied(run(&mut doc, first));
    assert_eq!(order(&doc), ["t1", "t2", "X", "t4", "t3"]);

    let mut second = nothing();
    second.insert.push((after(&p[1]), text("Y")));
    applied(run(&mut doc, second));
    assert_eq!(order(&doc), ["t1", "t2", "X", "Y", "t4", "t3"]);

    let mut third = nothing();
    third.replace.push((p[1].clone(), text("B")));
    applied(run(&mut doc, third));
    assert_eq!(order(&doc), ["t1", "B", "X", "Y", "t4", "t3"]);
}

#[test]
fn places_stand_at_the_thing_their_chain_starts_from() {
    let p = picks(4);
    let mut doc = edited(4);
    let mut change = nothing();
    change.insert.push((Anchor::Start, text("S")));
    change.insert.push((after(&p[1]), text("X")));
    change.moved.push((p[3].clone(), after(&p[1])));
    change.remove.push(p[2].clone());
    change.insert.push((after(&p[2]), text("R")));
    applied(run(&mut doc, change));
    assert_eq!(order(&doc), ["S", "t1", "t2", "X", "t4", "R"]);
    let at: Vec<Option<usize>> = doc.places().iter().map(|place| place.at).collect();
    assert_eq!(at, [None, Some(0), Some(1), Some(1), Some(1), Some(2)]);
}

#[derive(Debug, Clone)]
enum Op {
    Replace(usize),
    Insert(Option<usize>),
    Remove(usize),
    Move(usize, Option<usize>),
}

fn anchors(count: usize) -> BoxedStrategy<Option<usize>> {
    prop_oneof![Just(None), (0..count).prop_map(Some)].boxed()
}

fn op(count: usize) -> BoxedStrategy<Op> {
    prop_oneof![
        (0..count).prop_map(Op::Replace),
        anchors(count).prop_map(Op::Insert),
        (0..count).prop_map(Op::Remove),
        (0..count, anchors(count)).prop_map(|(thing, anchor)| Op::Move(thing, anchor)),
    ]
    .boxed()
}

/// Up to three changes, each of up to six edits, over up to ten things.
fn sequences() -> BoxedStrategy<(usize, Vec<Vec<Op>>)> {
    (0usize..=10)
        .prop_flat_map(|count| {
            if count == 0 {
                return Just((0, vec![Vec::new()])).boxed();
            }
            (
                Just(count),
                proptest::collection::vec(proptest::collection::vec(op(count), 0..6), 1..=3),
            )
                .boxed()
        })
        .boxed()
}

fn anchor_of(picks: &[Picked<Thing>], anchor: Option<usize>) -> Anchor<Thing> {
    match anchor {
        None => Anchor::Start,
        Some(index) => Anchor::After(picks[index].clone()),
    }
}

/// `ops` as a change; `round` keeps the content of one change apart from another's.
fn written_change(picks: &[Picked<Thing>], ops: &[Op], round: usize) -> Change<Thing, String> {
    let mut change = nothing();
    for (at, op) in ops.iter().enumerate() {
        let content = format!("w{round}_{at}");
        match op {
            Op::Replace(thing) => change.replace.push((picks[*thing].clone(), content)),
            Op::Insert(anchor) => change.insert.push((anchor_of(picks, *anchor), content)),
            Op::Remove(thing) => change.remove.push(picks[*thing].clone()),
            Op::Move(thing, anchor) => change
                .moved
                .push((picks[*thing].clone(), anchor_of(picks, *anchor))),
        }
    }
    change
}

fn check_structure(count: usize, accepted: &[Change<Thing, String>], doc: &Doc) {
    let places = doc.places();
    let removed: Vec<usize> = accepted
        .iter()
        .flat_map(|change| change.remove.iter().map(|thing| thing.reference))
        .collect();
    for thing in 0..count {
        let held = places
            .iter()
            .filter(|place| match &place.laid {
                Laid::Kept(kept) => **kept == thing,
                Laid::Wrote { replaces, .. } => *replaces == Some(&thing),
            })
            .count();
        assert_eq!(held, usize::from(!removed.contains(&thing)), "{}", name(thing));
    }
    let contents = accepted.iter().flat_map(|change| {
        change
            .insert
            .iter()
            .map(|(_, content)| content)
            .chain(change.replace.iter().map(|(_, content)| content))
    });
    for wanted in contents {
        let held = places
            .iter()
            .filter(|place| matches!(&place.laid, Laid::Wrote { content, .. } if *content == wanted))
            .count();
        assert_eq!(held, 1, "{wanted}");
    }
    assert!(places.windows(2).all(|pair| pair[0].at <= pair[1].at));
}

proptest! {
    #[test]
    fn consistency_is_decided_for_every_change((count, rounds) in sequences()) {
        let p = picks(count);
        for (round, ops) in rounds.iter().enumerate() {
            if let Err(contradiction) = consistent(written_change(&p, ops, round)) {
                prop_assert!(!contradiction.told().is_empty());
            }
        }
    }

    #[test]
    fn a_sequence_of_changes_is_all_or_nothing_and_keeps_every_thing_once((count, rounds) in sequences()) {
        let p = picks(count);
        let mut doc = edited(count);
        let mut accepted = Vec::new();
        for (round, ops) in rounds.iter().enumerate() {
            let change = written_change(&p, ops, round);
            let Ok(checked) = consistent(change.clone()) else { continue };
            let before = snapshot(&doc);
            match doc.apply(checked) {
                Settled::Answered(_) => accepted.push(change),
                Settled::Unanswered(Why::Contradicted(_)) => prop_assert_eq!(snapshot(&doc), before),
                other => prop_assert!(false, "unexpected {:?}", other),
            }
        }
        check_structure(count, &accepted, &doc);
    }

    #[test]
    fn two_changes_one_after_another_are_one_change_holding_both((count, rounds) in sequences()) {
        let Some((first_ops, second_ops)) = rounds.first().zip(rounds.get(1)) else {
            return Ok(());
        };
        let p = picks(count);
        let first = written_change(&p, first_ops, 0);
        let second = written_change(&p, second_ops, 1);
        let (Ok(first_checked), Ok(second_checked)) =
            (consistent(first.clone()), consistent(second.clone()))
        else {
            return Ok(());
        };
        let mut one_by_one = edited(count);
        prop_assert!(matches!(one_by_one.apply(first_checked), Settled::Answered(_)));
        if !matches!(one_by_one.apply(second_checked), Settled::Answered(_)) {
            return Ok(());
        }

        let mut together = nothing();
        together.replace.extend(first.replace.into_iter().chain(second.replace));
        together.insert.extend(first.insert.into_iter().chain(second.insert));
        together.remove.extend(first.remove.into_iter().chain(second.remove));
        together.moved.extend(first.moved.into_iter().chain(second.moved));
        let Ok(together) = consistent(together) else {
            prop_assert!(false, "the two changes together contradict themselves");
            return Ok(());
        };
        let mut at_once = edited(count);
        prop_assert!(matches!(at_once.apply(together), Settled::Answered(_)));
        prop_assert_eq!(snapshot(&one_by_one), snapshot(&at_once));
    }

    #[test]
    fn a_change_meeting_a_claimed_thing_is_refused_and_one_meeting_an_anchor_is_accepted((count, rounds) in sequences()) {
        let p = picks(count);
        let mut doc = edited(count);
        let Some(first_ops) = rounds.first() else { return Ok(()) };
        let first = written_change(&p, first_ops, 0);
        let Ok(checked) = consistent(first.clone()) else { return Ok(()) };
        let Settled::Answered(result) = doc.apply(checked) else {
            prop_assert!(false, "a first change is always accepted");
            return Ok(());
        };

        if let Some(claimed) = result.claimed.first() {
            let before = snapshot(&doc);
            let mut again = nothing();
            again.replace.push((claimed.clone(), text("again")));
            prop_assert!(matches!(
                run(&mut doc, again),
                Settled::Unanswered(Why::Contradicted(_))
            ));
            let mut anchored = nothing();
            anchored.insert.push((Anchor::After(claimed.clone()), text("again")));
            prop_assert!(matches!(
                run(&mut doc, anchored),
                Settled::Unanswered(Why::Contradicted(_))
            ));
            prop_assert_eq!(snapshot(&doc), before);
        }

        let claimed_by_first: Vec<usize> =
            result.claimed.iter().map(|thing| thing.reference).collect();
        let only_anchored: Vec<&Picked<Thing>> = first
            .insert
            .iter()
            .map(|(anchor, _)| anchor)
            .chain(first.moved.iter().map(|(_, anchor)| anchor))
            .filter_map(|anchor| match anchor {
                Anchor::After(thing) if !claimed_by_first.contains(&thing.reference) => Some(thing),
                _ => None,
            })
            .collect();
        for thing in only_anchored {
            let mut insert = nothing();
            insert.insert.push((Anchor::After(thing.clone()), text("later")));
            prop_assert!(matches!(run(&mut doc, insert), Settled::Answered(_)));
        }
    }
}
