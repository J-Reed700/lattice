//! Combines saved workspace data with the current runtime capabilities.
use crate::features::learning::{
    embedded_runtime::{self, LearningBuiltinRuntime, LearningBuiltinRuntimeCapability},
    lab_runtime::{self, LearningContainerEngine, LearningLabRuntimeCapability},
    practical_dto::{LearningPracticalRuntimeKind, LearningPracticalWorkspaceDto},
    practical_repository::{LearningPracticalRepository, PracticalWorkspaceSnapshot},
};

impl PracticalWorkspaceSnapshot {
    /// Probe runtimes only after persistence has finished, including operation replay.
    pub async fn resolve_runtime(self) -> LearningPracticalWorkspaceDto {
        let (docker, podman) = tokio::join!(
            lab_runtime::detect_container_engine(LearningContainerEngine::Docker),
            lab_runtime::detect_container_engine(LearningContainerEngine::Podman)
        );
        self.with_capabilities(embedded_runtime::capabilities(), vec![docker, podman])
    }

    /// Apply a single capability snapshot consistently to every activity.
    pub(in crate::features::learning) fn with_capabilities(
        self,
        builtin: Vec<LearningBuiltinRuntimeCapability>,
        containers: Vec<LearningLabRuntimeCapability>,
    ) -> LearningPracticalWorkspaceDto {
        let mut workspace = self.workspace;
        for activity in &mut workspace.activities {
            let profile = activity.runtime_profile_id.as_ref().and_then(|id| {
                workspace
                    .runtime_profiles
                    .iter()
                    .find(|profile| &profile.id == id)
            });
            let capability = activity.runtime_engine.and_then(|engine| {
                containers
                    .iter()
                    .find(|capability| capability.engine == engine)
            });
            let saved_version = self
                .builtin_versions
                .get(&activity.id)
                .and_then(|version| version.as_deref());
            let (available, reason) = match activity.runtime_kind {
                LearningPracticalRuntimeKind::Builtin
                    if activity.builtin_runtime.map(|runtime| runtime.version()) != saved_version =>
                    (false, Some("This activity uses an earlier built-in runtime. Its saved results remain available; generate a new activity to run with this app version.".into())),
                LearningPracticalRuntimeKind::Builtin => builtin.iter()
                    .find(|capability| Some(capability.id) == activity.builtin_runtime)
                    .map(|capability| (capability.available, capability.reason.clone()))
                    .unwrap_or((false, Some("Built-in runtime is unavailable.".into()))),
                LearningPracticalRuntimeKind::None => (
                    false,
                    Some("This activity is completed and reviewed without local execution.".into()),
                ),
                LearningPracticalRuntimeKind::Container => match (profile, capability) {
                    (Some(profile), Some(capability)) if profile.enabled && capability.available => (true, None),
                    (Some(profile), _) if !profile.enabled => (false, Some("The frozen runtime profile is disabled.".into())),
                    (_, Some(capability)) => (false, capability.reason.clone().or_else(|| Some("The container runtime is unavailable.".into()))),
                    _ => (false, Some("The frozen runtime profile is unavailable.".into())),
                },
            };
            activity.runtime_available = available;
            activity.runtime_unavailable_reason = reason;
        }
        workspace.builtin_runtimes = builtin;
        workspace.runtime_capabilities = containers;
        workspace
    }
}

use crate::shared::error::{AppError, Result};

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

pub(in crate::features::learning) async fn runtime_generation_context(
    repo: &LearningPracticalRepository,
    profile_id: Option<&str>,
    builtin: Option<LearningBuiltinRuntime>,
) -> Result<Option<crate::features::learning::practical_generation::PracticalRuntimeContext>> {
    if let Some(runtime) = builtin {
        if profile_id.is_some() {
            return Err(invalid("Choose one execution environment for an activity."));
        }
        let capability = embedded_runtime::capabilities()
            .into_iter()
            .find(|item| item.id == runtime)
            .ok_or_else(|| invalid("Unknown built-in runtime."))?;
        if !capability.available {
            return Err(AppError::ServiceNotAvailable(
                capability
                    .reason
                    .unwrap_or_else(|| "The built-in runtime is unavailable.".into()),
            ));
        }
        return Ok(Some(runtime.context()));
    }
    let Some(profile_id) = profile_id else {
        return Ok(None);
    };
    uuid::Uuid::parse_str(profile_id).map_err(|_| invalid("Invalid runtime profile ID"))?;
    let profile = repo
        .profiles()
        .await?
        .into_iter()
        .find(|profile| profile.id == profile_id && profile.enabled)
        .ok_or_else(|| invalid("Selected runtime profile is unavailable."))?;
    let contract = crate::features::learning::runtime_catalog::learning_runtime_catalog().into_iter()
        .find(|preset| preset.command == profile.command)
        .map(|preset| preset.entrypoint_contract)
        .unwrap_or_else(|| "Create files compatible with this exact runtime command. Network access and external dependency installation are unavailable.".into());
    Ok(Some(
        crate::features::learning::practical_generation::PracticalRuntimeContext {
            name: profile.name,
            command: profile.command,
            contract,
        },
    ))
}
