//! What a model refusal says: the true cause, with the numbers, and the fix.

use lattice_core::application::ports::llm_port::{ModelOrigin, ModelRole};
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::AppError;

use super::Holder;
use crate::features::llm::engine::sidecar_manager::Footprint;

/// A model held by running work, named in a refusal for memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pinner {
    pub(crate) name: String,
    pub(crate) gpu_bytes: u64,
    pub(crate) holders: Vec<Holder>,
}

/// The window a model has, as a refusal describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Window {
    /// Saved for the model in Settings → Models.
    Set(u32),
    /// What Auto gave the bundled server running now.
    Running(u32),
    /// What a server Lattice does not run reports.
    Reported(u32),
}

/// Log a refusal of `model`, or of a role that names none, as
/// `model.refused` and return it.
pub(crate) fn refused(
    role: ModelRole,
    model: Option<&ModelRef>,
    reason_code: &'static str,
    error: AppError,
) -> AppError {
    tracing::warn!(
        role = ?role,
        model = model.map(tracing::field::display),
        reason_code,
        error = %error,
        "model.refused"
    );
    error
}

/// The refusal for a model whose server does not fit in GPU memory.
pub(crate) fn no_memory(
    role: ModelRole,
    name: &str,
    needed: Footprint,
    spare: u64,
    pinners: &[Pinner],
) -> AppError {
    let mut message = format!(
        "Can't load {name} {purpose}: it needs {total} ({weights} weights + {cache} context at \
         {window} tokens) and {spare} is free.",
        purpose = purpose(role),
        total = gigabytes(needed.total()),
        weights = gigabytes(needed.weights),
        cache = gigabytes(needed.cache),
        window = tokens(needed.window),
        spare = gigabytes(spare),
    );
    match pinners.first() {
        Some(first) => {
            let busy: Vec<String> = pinners
                .iter()
                .map(|pinner| {
                    format!(
                        "{} ({}) is {}",
                        pinner.name,
                        gigabytes(pinner.gpu_bytes),
                        activities(&pinner.holders)
                    )
                })
                .collect();
            message.push_str(&format!(
                " {}. Wait for them to finish, stop them, or use {} {}.",
                busy.join("; "),
                first.name,
                purpose(role)
            ));
        }
        None => message.push_str(&format!(
            " Close other work using the GPU, or choose a smaller model {}.",
            purpose(role)
        )),
    }
    AppError::ServiceNotAvailable(message)
}

/// The refusal for a model whose window is smaller than the work's `floor`.
/// `auto` is the window Auto would give it on this machine now.
pub(crate) fn window_too_small(
    role: ModelRole,
    name: &str,
    floor: u32,
    window: Window,
    auto: Option<u32>,
) -> AppError {
    let needs = format!(
        "{} needs a context window of at least {} tokens.",
        subject(role),
        tokens(floor)
    );
    let model = model_phrase(role, name);
    let message = match window {
        Window::Set(set) => {
            let auto = auto
                .filter(|auto| *auto >= floor)
                .map(|auto| format!("Auto (≈{} on this Mac right now) or ", tokens(auto)))
                .unwrap_or_default();
            format!(
                "{needs} {model} is set to {} in Settings → Models. Set it to {auto}{} or more, \
                 or {}.",
                tokens(set),
                tokens(floor),
                larger(role)
            )
        }
        Window::Running(running) => format!(
            "{needs} {model} is running with {} on this Mac right now. Set its context window \
             to {} or more in Settings → Models, or {}.",
            tokens(running),
            tokens(floor),
            larger(role)
        ),
        Window::Reported(reported) => format!(
            "{needs} {model} has a window of {}. {}.",
            tokens(reported),
            capitalized(&larger(role))
        ),
    };
    AppError::InvalidConfig(message)
}

/// The refusal for work that needs tools from a model that cannot call them.
pub(crate) fn no_tools(role: ModelRole, name: &str) -> AppError {
    AppError::InvalidConfig(format!(
        "{name} cannot call tools, and {} needs them. Choose a tool-capable model {}.",
        subject(role).to_lowercase(),
        purpose(role)
    ))
}

/// The refusal for a model whose downloaded row is gone.
pub(crate) fn not_downloaded(role: ModelRole, origin: ModelOrigin, model_id: &str) -> AppError {
    AppError::AiModelsNotInstalled(match (role, origin) {
        (ModelRole::Learning, _) | (_, ModelOrigin::LearningRole) => format!(
            "Your Learning model, {model_id}, is not downloaded any more. Choose one in \
             Settings → Models under Learning."
        ),
        (_, ModelOrigin::ChatOverride) => {
            format!("This chat is set to {model_id}, which is no longer downloaded.")
        }
        _ => format!(
            "{model_id} is not downloaded any more. Download it in Settings → Models or pick \
             another model."
        ),
    })
}

/// What the work a model is wanted for is called, as a sentence's subject.
fn subject(role: ModelRole) -> &'static str {
    match role {
        ModelRole::Chat => "This chat",
        ModelRole::Learning => "Learning work",
        ModelRole::Utility | ModelRole::Router | ModelRole::Maintenance => "This work",
    }
}

/// What a model is wanted for, after its name.
fn purpose(role: ModelRole) -> &'static str {
    match role {
        ModelRole::Chat => "for this chat",
        ModelRole::Learning => "as the Learning model",
        ModelRole::Utility => "as the utility model",
        ModelRole::Router => "as the router model",
        ModelRole::Maintenance => "for this work",
    }
}

fn model_phrase(role: ModelRole, name: &str) -> String {
    match role {
        ModelRole::Learning => format!("Your Learning model, {name},"),
        _ => name.to_owned(),
    }
}

fn larger(role: ModelRole) -> String {
    match role {
        ModelRole::Chat => "choose a larger model for this chat".into(),
        ModelRole::Learning => "choose a larger Learning model".into(),
        _ => format!("choose a larger model {}", purpose(role)),
    }
}

/// What `holders` are doing with a model, as one phrase.
fn activities(holders: &[Holder]) -> String {
    let mut phrases: Vec<String> = Vec::new();
    for holder in holders {
        let phrase = match holder.role {
            ModelRole::Chat if !holder.work.is_empty() => format!("answering in '{}'", holder.work),
            ModelRole::Chat => "answering in a chat".to_owned(),
            ModelRole::Learning => "running a learning job".to_owned(),
            ModelRole::Utility | ModelRole::Router | ModelRole::Maintenance => {
                "running background work".to_owned()
            }
        };
        if !phrases.contains(&phrase) {
            phrases.push(phrase);
        }
    }
    phrases.join(" and ")
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Bytes in gigabytes to one decimal, as the GPU's memory is reported.
pub(crate) fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

/// A token count with its thousands grouped: `8,704`.
pub(crate) fn tokens(count: u32) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}
