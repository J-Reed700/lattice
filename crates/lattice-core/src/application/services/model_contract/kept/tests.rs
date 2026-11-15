use super::super::shown::{Drawn, Kind};
use super::*;
use serde_json::json;

struct Passage(&'static str);
struct Note(&'static str);

impl Kind for Passage {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Number;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.to_owned()
    }
}

impl Kind for Note {
    const PREFIX: &'static str = "n";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.to_owned()
    }
}

fn passage(names: &TurnNames, id: &'static str) -> String {
    names.with(|scope| {
        let things = [Passage(id)];
        let shown = scope.request().show(&things).unwrap();
        shown.handles()[0].bracketed()
    })
}

fn note(names: &TurnNames, id: &'static str) -> String {
    names.with(|scope| {
        let things = [Note(id)];
        let shown = scope.request().show(&things).unwrap();
        shown.handles()[0].bracketed()
    })
}

fn store() -> Arc<MemoryNames> {
    Arc::new(MemoryNames::new())
}

fn kept(store: &Arc<MemoryNames>) -> KeptNames {
    KeptNames::new(store.clone())
}

#[tokio::test]
async fn names_survive_a_turn_and_are_the_same_in_the_next() {
    let store = store();
    let kept = kept(&store);
    let first = kept.open("chat").await.unwrap();
    assert_eq!(passage(&first, "alpha"), "[s1]");
    assert_eq!(passage(&first, "beta"), "[s2]");
    first.save().await.unwrap();
    drop(first);

    let next = kept.open("chat").await.unwrap();
    assert_eq!(passage(&next, "beta"), "[s2]");
    assert_eq!(passage(&next, "gamma"), "[s3]");
    let resolved = next.with(|scope| scope.resolve_any("s1"));
    assert_eq!(resolved, Some(("s".to_owned(), 1, json!("alpha"))));
}

#[tokio::test]
async fn two_kinds_keep_their_own_numbering() {
    let store = store();
    let kept = kept(&store);
    let names = kept.open("chat").await.unwrap();
    assert_eq!(passage(&names, "alpha"), "[s1]");
    assert_eq!(note(&names, "one"), "[n1]");
    assert_eq!(passage(&names, "beta"), "[s2]");
    assert_eq!(note(&names, "two"), "[n2]");
    names.save().await.unwrap();
    drop(names);

    let next = kept.open("chat").await.unwrap();
    assert_eq!(note(&next, "three"), "[n3]");
    assert_eq!(passage(&next, "gamma"), "[s3]");
}

#[tokio::test]
async fn saving_twice_writes_once() {
    let store = store();
    let names = kept(&store).open("chat").await.unwrap();
    passage(&names, "alpha");
    names.save().await.unwrap();
    names.save().await.unwrap();
    assert_eq!(store.writes(), 1);

    passage(&names, "beta");
    names.save().await.unwrap();
    assert_eq!(store.writes(), 2);
    assert_eq!(store.names("chat").await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_later_save_continues_after_the_names_a_turn_opened_with() {
    let store = store();
    let kept = kept(&store);
    let first = kept.open("chat").await.unwrap();
    passage(&first, "alpha");
    first.save().await.unwrap();
    drop(first);

    let next = kept.open("chat").await.unwrap();
    passage(&next, "beta");
    next.save().await.unwrap();

    let ordinals: Vec<u32> = store
        .names("chat")
        .await
        .unwrap()
        .iter()
        .map(|record| record.ordinal)
        .collect();
    assert_eq!(ordinals, vec![1, 2]);
}

#[tokio::test]
async fn two_turns_on_one_conversation_share_one_scope() {
    let store = store();
    let kept = kept(&store);
    let first = kept.open("chat").await.unwrap();
    let second = kept.open("chat").await.unwrap();
    assert!(Arc::ptr_eq(&first, &second));

    assert_eq!(passage(&first, "alpha"), "[s1]");
    assert_eq!(passage(&second, "beta"), "[s2]");
    let resolved = second.with(|scope| scope.resolve_any("s1"));
    assert_eq!(resolved, Some(("s".to_owned(), 1, json!("alpha"))));

    first.save().await.unwrap();
    second.save().await.unwrap();
    assert_eq!(store.writes(), 1);
    assert_eq!(store.names("chat").await.unwrap().len(), 2);
}

#[tokio::test]
async fn once_every_holder_is_gone_the_next_open_loads_what_was_kept() {
    let store = store();
    let kept = kept(&store);
    let first = kept.open("chat").await.unwrap();
    passage(&first, "alpha");
    first.save().await.unwrap();
    let second = kept.open("chat").await.unwrap();
    drop(first);
    drop(second);

    let next = kept.open("chat").await.unwrap();
    assert_eq!(passage(&next, "beta"), "[s2]");
}

#[tokio::test]
async fn a_copy_has_the_names_of_its_parent_and_goes_on_from_them() {
    let store = store();
    let kept = kept(&store);
    let parent = kept.open("parent").await.unwrap();
    passage(&parent, "alpha");
    note(&parent, "one");
    parent.save().await.unwrap();
    store.copy("parent", "branch");

    let branch = kept.open("branch").await.unwrap();
    assert_eq!(passage(&branch, "alpha"), "[s1]");
    assert_eq!(passage(&branch, "beta"), "[s2]");
    branch.save().await.unwrap();
    assert_eq!(store.names("parent").await.unwrap().len(), 2);
    assert_eq!(store.names("branch").await.unwrap().len(), 3);
}

#[tokio::test]
async fn an_unstored_turn_keeps_nothing() {
    let names = TurnNames::unstored();
    assert_eq!(passage(&names, "alpha"), "[s1]");
    names.save().await.unwrap();
}

#[tokio::test]
async fn a_written_position_cannot_be_written_again() {
    let store = store();
    let record = |reference: &str| Record {
        prefix: "s".into(),
        ordinal: 1,
        under: None,
        reference: json!(reference),
    };
    store.keep("chat", 0, &[record("alpha")]).await.unwrap();
    assert!(store.keep("chat", 0, &[record("other")]).await.is_err());
}

#[tokio::test]
async fn names_out_of_order_are_refused() {
    let store = store();
    store
        .keep(
            "chat",
            0,
            &[Record {
                prefix: "s".into(),
                ordinal: 2,
                under: None,
                reference: json!("alpha"),
            }],
        )
        .await
        .unwrap();
    assert!(kept(&store).open("chat").await.is_err());
}
