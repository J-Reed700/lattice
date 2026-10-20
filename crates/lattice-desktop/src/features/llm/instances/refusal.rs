//! What a model refusal says: the true cause, with the numbers, and the fix.

use lattice_core::application::ports::llm_port::{ModelOrigin, ModelRole};
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::AppError;

/// The window a model has, as a refusal describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Window {
    /// Saved for the model in Settings → Models.
    Set(u32),
    /// What the bundled server running now came up with.
    Running(u32),
    /// What a server Lattice does not run reports.
    Reported(u32),
    /// What the model was trained for, which it launches with when no window
    /// is saved for it.
    Trained(u32),
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

/// The refusal for a model whose window is smaller than the work's `floor`.
pub(crate) fn window_too_small(
    role: ModelRole,
    name: &str,
    floor: u32,
    window: Window,
) -> AppError {
    let needs = format!(
        "{} needs a context window of at least {} tokens.",
        subject(role),
        tokens(floor)
    );
    let model = model_phrase(role, name);
    let message = match window {
        Window::Set(set) => format!(
            "{needs} {model} is set to {} in Settings → Models. Set it to {} or more, or {}.",
            tokens(set),
            tokens(floor),
            larger(role)
        ),
        Window::Running(running) => format!(
            "{needs} {model} is running with {}. Set its context window to {} or more in \
             Settings → Models, or {}.",
            tokens(running),
            tokens(floor),
            larger(role)
        ),
        Window::Trained(trained) => format!(
            "{needs} {model} was trained for {} tokens. Set its context window to {} or more in \
             Settings → Models (a stretched window costs some quality), or {}.",
            tokens(trained),
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
        (ModelRole::Verifier, _) | (_, ModelOrigin::VerifierRole) => format!(
            "Your Verifier model, {model_id}, is not downloaded any more. Choose one in \
             Settings → Models under Verifier."
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
        ModelRole::Verifier => "The Verifier audit",
        ModelRole::Utility | ModelRole::Router | ModelRole::Maintenance => "This work",
    }
}

/// What a model is wanted for, after its name.
fn purpose(role: ModelRole) -> &'static str {
    match role {
        ModelRole::Chat => "for this chat",
        ModelRole::Learning => "as the Learning model",
        ModelRole::Verifier => "as the Verifier model",
        ModelRole::Utility => "as the utility model",
        ModelRole::Router => "as the router model",
        ModelRole::Maintenance => "for this work",
    }
}

fn model_phrase(role: ModelRole, name: &str) -> String {
    match role {
        ModelRole::Learning => format!("Your Learning model, {name},"),
        ModelRole::Verifier => format!("Your Verifier model, {name},"),
        _ => name.to_owned(),
    }
}

fn larger(role: ModelRole) -> String {
    match role {
        ModelRole::Chat => "choose a larger model for this chat".into(),
        ModelRole::Learning => "choose a larger Learning model".into(),
        ModelRole::Verifier => "choose a larger Verifier model".into(),
        _ => format!("choose a larger model {}", purpose(role)),
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A token count with its thousands grouped: `8,704`.
pub(crate) fn tokens(count: u32) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}
