use super::*;
use crate::domain::provenance::policy::POLICY;

const TEXT: &str = "Python's asyncio module runs coroutines on a single-threaded event loop. A coroutine is declared with async def and suspends itself at every await expression, handing control back to the loop.";

#[test]
fn copies_resemble_and_excerpts_are_contained() {
    let a = shingles(TEXT, &POLICY);
    let reformatted = shingles(
        &format!("## Asyncio\n\n{}", TEXT.replace(". ", ".\n\n")),
        &POLICY,
    );
    assert!(resemblance(&a, &reformatted) > 0.9);
    let excerpt = shingles(
        "A coroutine is declared with async def and suspends itself at every await expression",
        &POLICY,
    );
    assert_eq!(containment(&excerpt, &a), 1.0);
    assert!(resemblance(&excerpt, &a) < 0.6);
}

#[test]
fn link_targets_do_not_count_as_shared_text() {
    let a = shingles(
        "Read [the guide](https://example.org/a/very/long/path/to/the/guide) today please now",
        &POLICY,
    );
    let b = shingles("Read the guide today please now", &POLICY);
    assert_eq!(resemblance(&a, &b), 1.0);
}

#[test]
fn scripts_without_word_breaks_are_shingled_by_characters() {
    let zh = shingles(
        "茶树原产于中国西南地区，早在唐代以前人们就已经开始饮茶。",
        &POLICY,
    );
    assert!(zh.len() > 10, "character shingles, not one word");
    let copy = shingles(
        "首页。茶树原产于中国西南地区，早在唐代以前人们就已经开始饮茶。",
        &POLICY,
    );
    assert!(containment(&zh, &copy) > 0.95);
    assert!(shingles("", &POLICY).is_empty());
}

#[test]
fn sketches_estimate_resemblance_and_round_trip() {
    let a = shingles(TEXT, &POLICY);
    let b = shingles(&TEXT.replace("single-threaded", "one-thread"), &POLICY);
    let (sa, sb) = (Sketch::of(&a), Sketch::of(&b));
    assert!((sa.resemblance(&sb) - resemblance(&a, &b)).abs() < 0.2);
    assert_eq!(Sketch::from_le_bytes(&sa.to_le_bytes()), Some(sa.clone()));
    assert_eq!(Sketch::from_le_bytes(&[0; 3]), None);
    let json = serde_json::to_string(&sa).unwrap();
    assert_eq!(serde_json::from_str::<Sketch>(&json).unwrap(), sa);
    assert_eq!(Sketch::of(&a), sa, "deterministic");
}

#[test]
fn page_relations_point_copies_at_the_older_and_containers_at_the_contained() {
    let old = shingles(TEXT, &POLICY);
    let copy = shingles(TEXT, &POLICY);
    let relations = page_relations("new", &copy, &[("old", &old)], &POLICY);
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].kind, RelationKind::CopyOf);
    assert_eq!(relations[0].from, "new");

    let wrapped = shingles(&format!("{TEXT} {}", "Editor's note: we republish this with permission and add our own view below, because readers asked for more context on how the event loop schedules work and what blocks it in practice, with examples from our projects."), &POLICY);
    let relations = page_relations("wrapper", &wrapped, &[("old", &old)], &POLICY);
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].kind, RelationKind::ContainsCopyOf);
    assert_eq!(
        (relations[0].from.as_str(), &relations[0].to),
        ("wrapper", &RelationTarget::Version("old".into()))
    );

    let relations = page_relations("old", &old, &[("wrapper", &wrapped)], &POLICY);
    assert_eq!(relations[0].from, "wrapper", "either direction is found");
}

#[test]
fn clusters_fold_origins_copies_and_shared_targets() {
    let origins: HashMap<String, OriginKey> = [
        ("a", "web:python.org"),
        ("b", "web:python.org"),
        ("c", "web:example.org"),
        ("d", "web:mirror.example"),
        ("e", "web:blog.example"),
        ("f", "web:other.example"),
    ]
    .into_iter()
    .map(|(id, key)| (id.to_owned(), OriginKey(key.to_owned())))
    .collect();
    let relations = vec![
        SourceRelation {
            from: "d".into(),
            to: RelationTarget::Version("x".into()),
            kind: RelationKind::CopyOf,
            evidence: RelationEvidence::Text {
                resemblance: 0.9,
                containment: 0.95,
            },
        },
        SourceRelation {
            from: "c".into(),
            to: RelationTarget::Version("x".into()),
            kind: RelationKind::CopyOf,
            evidence: RelationEvidence::Text {
                resemblance: 0.9,
                containment: 0.95,
            },
        },
        SourceRelation {
            from: "e".into(),
            to: RelationTarget::Url("https://canonical.example/post".into()),
            kind: RelationKind::Cites,
            evidence: RelationEvidence::Citation {
                start_byte: 0,
                end_byte: 1,
                anchor: "x".into(),
            },
        },
    ];
    let merges = vec![("e".to_owned(), "f".to_owned())];
    let found = clusters(
        &["f", "e", "d", "c", "b", "a"],
        &relations,
        &origins,
        &merges,
    );
    let members: Vec<Vec<String>> = found.iter().map(|c| c.members.clone()).collect();
    assert_eq!(
        members,
        vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["c".to_string(), "d".to_string()],
            vec!["e".to_string(), "f".to_string()],
        ],
        "same origin, copies of one page via a non-support, and a passage merge; a citation never merges"
    );
    assert_eq!(found[1].id, ClusterId("c".into()));
}

#[test]
fn passage_merges_find_syndicated_paragraphs() {
    let merges = passage_merges(
        &[
            ("a", TEXT),
            ("b", TEXT),
            (
                "c",
                "An unrelated sentence about bread and fermentation in cold kitchens.",
            ),
        ],
        &POLICY,
    );
    assert_eq!(merges, vec![("a".to_string(), "b".to_string())]);
}
