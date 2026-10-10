use super::{
    curriculum::LearningGenerationJob, dto::*, plan_dto::*, portability_dto::*, practical_dto::*,
    repository::LearningRepository, service,
};
use crate::features::web::traits::WebServiceTrait;
use crate::{interfaces::di::Container, shared::ipc::ApiError};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime, State,
};

pub(super) use super::source_identity::source_request_hash;
use super::source_identity::validate_source_ids;

fn source_library(container: &Container) -> super::source_library::LearningSourceLibraryRepository {
    super::source_library::LearningSourceLibraryRepository::new(container.db_pool().clone())
}

mod program;
pub use program::*;
mod memory;
pub use memory::*;
mod canvas;
pub use canvas::*;
mod sources;
pub use sources::*;
mod recall;
pub use recall::*;
mod practice;
pub use practice::*;
mod planning;
pub use planning::*;
mod portability;
pub use portability::*;
mod diagnostics;
pub use diagnostics::*;
mod generation;
pub use generation::*;
mod assessment;
pub use assessment::*;
mod practical;
pub use practical::*;
mod study;
pub use study::*;

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("learning")
        .setup(|app, _api| {
            let container = app.state::<Container>().inner().clone();
            // Registration settles lesson jobs the last process left running,
            // then starts delivering saved work.
            tauri::async_runtime::block_on(container.jobs().register(
                super::curriculum_repository::LESSON_PREPARATION,
                std::sync::Arc::new(generation_worker(&container)),
                super::curriculum_repository::lesson_job_config(),
            ))?;
            let practical = super::practical_repository::LearningPracticalRepository::new(
                container.db_pool().clone(),
            );
            tauri::async_runtime::block_on(super::practical_runs::recover_running_runs(
                &practical,
            ))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_learning_plan,
            get_learning_portability_workspace,
            export_learning_pack,
            preview_learning_pack_import,
            apply_learning_pack_import,
            cancel_learning_pack_import_preview,
            preview_learning_curriculum_revision,
            accept_learning_curriculum_revision,
            discard_learning_curriculum_revision,
            start_learning_diagnostic,
            submit_learning_diagnostic,
            skip_learning_diagnostic,
            start_learning_generation_job,
            cancel_learning_generation_job,
            retry_learning_generation_job,
            get_learning_generation_job,
            list_learning_programs,
            get_learning_program,
            get_learning_lesson_evidence,
            get_learning_outline_evidence,
            generate_learning_program,
            cancel_learning_outline,
            repair_learning_outline,
            accept_learning_program,
            prepare_learning_lesson,
            complete_learning_lesson,
            submit_learning_attempt,
            delete_learning_program,
            get_learning_memory,
            ensure_learning_lesson_note,
            generate_learning_card_drafts,
            save_learning_card_draft,
            accept_learning_card_draft,
            discard_learning_card_draft,
            get_learning_canvas_workspace,
            create_learning_canvas,
            save_learning_canvas,
            create_learning_canvas_snapshot,
            restore_learning_canvas_snapshot,
            get_learning_source_workspace,
            get_learning_source_version,
            search_learning_sources,
            add_learning_web_source,
            add_learning_document_source,
            add_learning_text_source,
            refresh_learning_source,
            adopt_learning_source_version,
            update_learning_source_policy,
            delete_learning_source,
            reimport_learning_source,
            create_learning_source_selector,
            get_learning_source_selector,
            match_learning_source_selector,
            search_learning_sources_semantically,
            get_learning_recall_workspace,
            save_learning_recall_card,
            decide_learning_recall_duplicate,
            review_learning_recall_card,
            get_learning_practice_workspace,
            get_learning_practice_session,
            start_learning_practice_session,
            save_learning_practice_artifact,
            change_learning_practice_mode,
            open_learning_practice_source,
            request_learning_tutor_response,
            reveal_learning_practice_solution,
            submit_learning_practice_attempt,
            accept_learning_practice_proposal,
            reject_learning_practice_proposal,
            get_learning_assessment_workspace,
            create_learning_assessment_blueprint,
            start_learning_assessment_form,
            get_learning_assessment_form,
            save_learning_assessment_response,
            interrupt_learning_assessment_form,
            submit_learning_assessment_form,
            accept_learning_follow_up,
            dismiss_learning_follow_up,
            get_learning_practical_workspace,
            get_learning_practical_draft,
            save_learning_practical_draft,
            get_learning_runtime_catalog,
            prepare_learning_runtime_preset,
            save_learning_runtime_profile,
            generate_learning_practical_activity,
            start_learning_practical_run,
            cancel_learning_practical_run,
            start_learning_simulation,
            send_learning_simulation_turn,
            finish_learning_simulation,
            list_study_decks,
            get_study_deck,
            generate_study_deck,
            generate_conversation_study_deck,
            review_study_card,
            update_study_card,
            delete_study_deck
        ])
        .build()
}
