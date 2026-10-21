//! What the models are doing, for the activity view: each backend's queue
//! of requests, with who holds its models, and every model loading now.

use super::instances::{LlmInstances, LoadedInstance, LoadingModel};
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{ModelRole, RequestLoad};
use lattice_core::application::ports::{ActivityItem, ActivitySource, ActivityState};
use lattice_core::shared::Result;
use std::sync::Arc;

/// The loaded models and their backends' schedulers.
pub(crate) struct ModelActivity {
    instances: Arc<LlmInstances>,
}

impl ModelActivity {
    pub(crate) fn new(instances: Arc<LlmInstances>) -> Self {
        Self { instances }
    }
}

/// What a role loads its model for.
fn role_words(role: ModelRole) -> &'static str {
    match role {
        ModelRole::Chat => "for chat",
        ModelRole::Utility => "for quick tasks",
        ModelRole::Router => "for routing questions",
        ModelRole::Learning => "for Learning",
        ModelRole::Maintenance => "for upkeep",
        ModelRole::Verifier => "for checking claims",
    }
}

fn loading_item(loading: &LoadingModel) -> ActivityItem {
    ActivityItem::new(
        format!("model.loading:{}:{:?}", loading.name, loading.role),
        "model.loading",
        format!("Loading {}", loading.name),
        ActivityState::Running,
    )
    .subject(loading.name.clone())
    .detail(format!("Starting the model {}", role_words(loading.role)))
}

/// "2 requests running, 3 queued (1 from a chat)".
fn queue_words(load: &RequestLoad) -> String {
    let requests = |count: usize| if count == 1 { "request" } else { "requests" };
    let mut words = match (load.running, load.queued) {
        (0, queued) => format!("{queued} {} waiting for a slot", requests(queued)),
        (running, 0) => format!("{running} {} running", requests(running)),
        (running, queued) => format!("{running} {} running, {queued} queued", requests(running)),
    };
    if load.queued_interactive > 0 {
        words.push_str(&format!(" ({} from a chat)", load.queued_interactive));
    }
    words
}

/// One item per backend with requests running or queued: two models on one
/// server share its queue, so they are one line.
fn request_items(loaded: Vec<LoadedInstance>) -> Vec<ActivityItem> {
    let mut backends: Vec<(RequestLoad, Vec<String>, Vec<String>)> = Vec::new();
    for instance in loaded {
        let Some(load) = instance.requests else {
            continue;
        };
        if load.running == 0 && load.queued == 0 {
            continue;
        }
        match backends
            .iter_mut()
            .find(|(seen, _, _)| seen.backend == load.backend)
        {
            Some((_, names, holders)) => {
                if !names.contains(&instance.name) {
                    names.push(instance.name);
                }
                holders.extend(instance.pinned_by);
            }
            None => backends.push((load, vec![instance.name], instance.pinned_by)),
        }
    }
    backends
        .into_iter()
        .map(|(load, names, mut holders)| {
            holders.sort();
            holders.dedup();
            let state = if load.running > 0 {
                ActivityState::Running
            } else {
                ActivityState::Waiting
            };
            let mut detail = queue_words(&load);
            if !holders.is_empty() {
                detail.push_str(&format!(" · held by {}", holders.join(", ")));
            }
            let mut item = ActivityItem::new(
                format!("model.requests:{}", load.backend),
                "model.requests",
                names.join(", "),
                state,
            )
            .detail(detail);
            if let Some(first) = names.first() {
                item = item.subject(first.clone());
            }
            item.queued = Some(load.queued);
            item
        })
        .collect()
}

#[async_trait]
impl ActivitySource for ModelActivity {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let mut items: Vec<ActivityItem> =
            self.instances.loading().iter().map(loading_item).collect();
        items.extend(request_items(self.instances.snapshot()));
        Ok(items)
    }
}

#[cfg(test)]
mod tests;
