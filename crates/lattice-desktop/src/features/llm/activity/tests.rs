#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! The models' activity: one line per backend, in words.

use super::{loading_item, queue_words, request_items};
use crate::features::llm::instances::{LoadedInstance, LoadingModel};
use lattice_core::application::ports::llm_port::{ModelRole, RequestLoad};
use lattice_core::application::ports::ActivityState;
use lattice_core::domain::models::model_ref::ModelRef;

fn load(backend: &str, running: usize, queued: usize, interactive: usize) -> RequestLoad {
    RequestLoad {
        backend: backend.to_string(),
        slots: 2,
        running,
        queued,
        queued_interactive: interactive,
    }
}

fn loaded(name: &str, holders: &[&str], requests: Option<RequestLoad>) -> LoadedInstance {
    LoadedInstance {
        model: ModelRef::LlamaCpp {
            model: name.to_string(),
        },
        provider: "llama.cpp".to_string(),
        context_tokens: 8192,
        in_flight: 0,
        pinned_by: holders.iter().map(|holder| holder.to_string()).collect(),
        loaded_at: String::new(),
        last_used_at: String::new(),
        gpu_bytes: None,
        degraded: None,
        name: name.to_string(),
        requests,
    }
}

#[test]
fn the_queue_reads_as_people_say_it() {
    assert_eq!(queue_words(&load("a", 1, 0, 0)), "1 request running");
    assert_eq!(
        queue_words(&load("a", 2, 3, 1)),
        "2 requests running, 3 queued (1 from a chat)"
    );
    assert_eq!(
        queue_words(&load("a", 0, 1, 0)),
        "1 request waiting for a slot"
    );
}

#[test]
fn two_models_on_one_backend_are_one_line_and_an_idle_backend_none() {
    let items = request_items(vec![
        loaded("Qwen", &["chat: Notes"], Some(load("server-1", 1, 2, 0))),
        loaded(
            "Qwen Small",
            &["learning: Algebra"],
            Some(load("server-1", 1, 2, 0)),
        ),
        loaded("Idle", &[], Some(load("server-2", 0, 0, 0))),
        loaded("No scheduler", &[], None),
    ]);
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.title, "Qwen, Qwen Small");
    assert_eq!(item.subject_name(), "Qwen");
    assert_eq!(item.state, ActivityState::Running);
    assert_eq!(item.queued, Some(2));
    assert_eq!(
        item.detail.as_deref(),
        Some("1 request running, 2 queued · held by chat: Notes, learning: Algebra")
    );
}

#[test]
fn a_backend_with_only_queued_requests_waits() {
    let items = request_items(vec![loaded("Qwen", &[], Some(load("server-1", 0, 3, 0)))]);
    assert_eq!(items[0].state, ActivityState::Waiting);
}

#[test]
fn a_loading_model_says_what_it_is_for() {
    let item = loading_item(&LoadingModel {
        name: "Qwen".to_string(),
        role: ModelRole::Learning,
    });
    assert_eq!(item.title, "Loading Qwen");
    assert_eq!(item.state, ActivityState::Running);
    assert_eq!(
        item.detail.as_deref(),
        Some("Starting the model for Learning")
    );
}
