#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! The snapshot: who holds behind whom in the turn order, what each waiting
//! item is queued behind, what each holds, and the order of the list.

use super::assemble::assemble;
use super::dto::ActivityEntryDto;
use crate::infrastructure::passage_index::Standing;
use lattice_core::application::ports::{
    ActivityItem, ActivityProgress, ActivityResource, ActivityState,
};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    PathBuf::from("/indexes").join(name)
}

fn item(id: &str, title: &str, state: ActivityState) -> ActivityItem {
    ActivityItem::new(id, "test", title, state)
}

fn find<'a>(items: &'a [ActivityEntryDto], id: &str) -> &'a ActivityEntryDto {
    items.iter().find(|item| item.id == id).unwrap()
}

#[test]
fn nothing_running_is_an_empty_snapshot() {
    let snapshot = assemble(Vec::new(), &[]);
    assert!(snapshot.items.is_empty());
    assert_eq!(snapshot.running, 0);
}

#[test]
fn an_index_behind_another_is_paused_and_names_the_one_that_goes_first() {
    let course = item(
        "course",
        "Indexing evidence for Calculus",
        ActivityState::Running,
    )
    .subject("Calculus")
    .index_dir(Some(dir("course")))
    .uses(ActivityResource::EmbeddingModel);
    let folder = item("folder", "Indexing notes", ActivityState::Running)
        .subject("notes")
        .index_dir(Some(dir("folder")))
        .progress(Some(ActivityProgress::of(10, 100, "passages")))
        .pace(Some(5.0))
        .uses(ActivityResource::EmbeddingModel);
    let standing = vec![
        Standing {
            dir: dir("course"),
            ahead: vec![],
        },
        Standing {
            dir: dir("folder"),
            ahead: vec![dir("course")],
        },
    ];

    let snapshot = assemble(vec![folder, course], &standing);

    let folder = find(&snapshot.items, "folder");
    assert_eq!(folder.state, ActivityState::Paused);
    assert_eq!(
        folder.detail.as_deref(),
        Some("Paused while Calculus embeds first")
    );
    // A paused build has no pace, and holds nothing while it waits.
    assert_eq!(folder.per_second, None);
    assert_eq!(folder.seconds_left, None);
    assert_eq!(folder.holding, None);
    assert_eq!(folder.progress.as_ref().map(|p| p.done), Some(10));

    let course = find(&snapshot.items, "course");
    assert_eq!(course.state, ActivityState::Running);
    assert_eq!(course.holding.as_deref(), Some("Using the embedding model"));
    assert_eq!(snapshot.running, 1);
}

#[test]
fn an_index_behind_one_nobody_names_says_another_index() {
    let folder =
        item("folder", "Indexing notes", ActivityState::Running).index_dir(Some(dir("folder")));
    let standing = vec![Standing {
        dir: dir("folder"),
        ahead: vec![dir("lesson-reader")],
    }];
    let snapshot = assemble(vec![folder], &standing);
    assert_eq!(
        snapshot.items[0].detail.as_deref(),
        Some("Paused while another index embeds first")
    );
}

#[test]
fn a_waiting_item_is_queued_behind_the_running_one_then_those_before_it() {
    let running = item("a", "Indexing evidence for Algebra", ActivityState::Running)
        .subject("Algebra")
        .queue("learning.course_index")
        .since(Some(10));
    let first = item("b", "Indexing evidence for Biology", ActivityState::Waiting)
        .subject("Biology")
        .queue("learning.course_index")
        .since(Some(20));
    let second = item(
        "c",
        "Indexing evidence for Chemistry",
        ActivityState::Waiting,
    )
    .subject("Chemistry")
    .queue("learning.course_index")
    .since(Some(30));
    let elsewhere = item("d", "Preparing “Limits”", ActivityState::Waiting)
        .detail("Waiting to prepare")
        .queue("learning.lesson_preparation:other")
        .since(Some(5));

    let snapshot = assemble(vec![second, elsewhere, first, running], &[]);

    assert_eq!(
        find(&snapshot.items, "b").detail.as_deref(),
        Some("Queued behind Algebra")
    );
    assert_eq!(
        find(&snapshot.items, "c").detail.as_deref(),
        Some("Queued behind Algebra and 1 more")
    );
    // Alone in its line, it keeps its own words.
    assert_eq!(
        find(&snapshot.items, "d").detail.as_deref(),
        Some("Waiting to prepare")
    );
}

#[test]
fn a_job_put_off_by_an_outage_keeps_its_reason() {
    let running = item("a", "Preparing “Limits”", ActivityState::Running).queue("q");
    let mut deferred = item("b", "Preparing “Series”", ActivityState::Waiting)
        .detail("Connection interrupted; saved work will resume automatically")
        .queue("q");
    deferred.retry_at = Some(1_000);
    let snapshot = assemble(vec![running, deferred], &[]);
    let deferred = find(&snapshot.items, "b");
    assert_eq!(
        deferred.detail.as_deref(),
        Some("Connection interrupted; saved work will resume automatically")
    );
    assert_eq!(deferred.retry_at, Some(1_000));
}

#[test]
fn an_item_using_a_model_is_told_how_many_requests_wait_for_it() {
    let mut model = item("model", "Qwen", ActivityState::Running).subject("Qwen");
    model.queued = Some(2);
    let lesson = item("lesson", "Preparing “Limits”", ActivityState::Running).uses(
        ActivityResource::Model {
            name: "Qwen".to_string(),
        },
    );
    let outline = item("outline", "Outlining Algebra", ActivityState::Running).uses(
        ActivityResource::Model {
            name: "Other".to_string(),
        },
    );
    let snapshot = assemble(vec![model, lesson, outline], &[]);
    assert_eq!(
        find(&snapshot.items, "lesson").holding.as_deref(),
        Some("Using Qwen (2 requests queued)")
    );
    assert_eq!(
        find(&snapshot.items, "outline").holding.as_deref(),
        Some("Using Other")
    );
    assert_eq!(find(&snapshot.items, "model").holding, None);
}

#[test]
fn running_comes_first_then_paused_then_waiting_each_oldest_first() {
    let items = vec![
        item("waiting", "W", ActivityState::Waiting).since(Some(1)),
        item("paused", "P", ActivityState::Paused).since(Some(1)),
        item("newer", "N", ActivityState::Running).since(Some(20)),
        item("older", "O", ActivityState::Running).since(Some(10)),
        item("unknown", "U", ActivityState::Running),
    ];
    let snapshot = assemble(items, &[]);
    let order: Vec<&str> = snapshot.items.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(
        order,
        vec!["older", "newer", "unknown", "paused", "waiting"]
    );
    assert_eq!(snapshot.running, 3);
}

#[test]
fn a_measured_pace_leaves_a_time_and_no_pace_leaves_none() {
    let measured = ActivityItem::new("a", "test", "A", ActivityState::Running)
        .progress(Some(ActivityProgress::of(40, 100, "passages")))
        .pace(Some(2.0));
    assert_eq!(measured.seconds_left, Some(30));
    let unmeasured = ActivityItem::new("b", "test", "B", ActivityState::Running)
        .progress(Some(ActivityProgress::of(40, 100, "passages")))
        .pace(None);
    assert_eq!(unmeasured.seconds_left, None);
    let stalled = ActivityItem::new("c", "test", "C", ActivityState::Running)
        .progress(Some(ActivityProgress::of(40, 100, "passages")))
        .pace(Some(0.0));
    assert_eq!(stalled.per_second, None);
    assert_eq!(stalled.seconds_left, None);
}
