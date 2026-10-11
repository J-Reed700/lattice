import { Channel } from '@tauri-apps/api/core';

import type * as Wire from '@/lib/bindings';
import { studyActivityCall } from '@/lib/studyActivity';
import type { CommandName } from '@/shared/ipc/transport';
import type { ApiResult } from '@/types';

const learningCall = <T>(command: CommandName, args?: Record<string, unknown>) =>
  studyActivityCall<T>('learning', command, args);

export const learningApi = {
  listLearningPrograms: (): Promise<
    ApiResult<Wire.LearningProgramSummaryDto[]>
  > => learningCall('list_learning_programs'),

  getLearningPlan: (id: string): Promise<ApiResult<Wire.LearningPlanDto>> =>
    learningCall('get_learning_plan', { id }),

  previewLearningCurriculumRevision: (
    request: Wire.PreviewLearningCurriculumRevisionRequestDto,
  ): Promise<ApiResult<Wire.LearningPlanDto>> =>
    learningCall('preview_learning_curriculum_revision', { request }),

  acceptLearningCurriculumRevision: (
    request: Wire.LearningCurriculumRevisionActionRequestDto,
  ): Promise<ApiResult<Wire.LearningPlanDto>> =>
    learningCall('accept_learning_curriculum_revision', { request }),

  discardLearningCurriculumRevision: (
    request: Wire.DiscardLearningCurriculumRevisionRequestDto,
  ): Promise<ApiResult<Wire.LearningPlanDto>> =>
    learningCall('discard_learning_curriculum_revision', { request }),

  startLearningDiagnostic: (
    request: Wire.StartLearningDiagnosticRequestDto,
  ): Promise<ApiResult<Wire.LearningDiagnosticAttemptDto>> =>
    learningCall('start_learning_diagnostic', { request }),

  submitLearningDiagnostic: (
    request: Wire.SubmitLearningDiagnosticRequestDto,
  ): Promise<ApiResult<Wire.LearningDiagnosticAttemptDto>> =>
    learningCall('submit_learning_diagnostic', { request }),

  skipLearningDiagnostic: (
    request: Wire.SkipLearningDiagnosticRequestDto,
  ): Promise<ApiResult<Wire.LearningDiagnosticAttemptDto>> =>
    learningCall('skip_learning_diagnostic', { request }),

  startLearningGenerationJob: (
    request: Wire.StartLearningGenerationJobRequestDto,
  ): Promise<ApiResult<Wire.LearningGenerationJob>> =>
    learningCall('start_learning_generation_job', { request }),

  cancelLearningGenerationJob: (
    request: Wire.LearningGenerationJobActionRequestDto,
  ): Promise<ApiResult<Wire.LearningGenerationJob>> =>
    learningCall('cancel_learning_generation_job', { request }),

  retryLearningGenerationJob: (
    request: Wire.LearningGenerationJobActionRequestDto,
  ): Promise<ApiResult<Wire.LearningGenerationJob>> =>
    learningCall('retry_learning_generation_job', { request }),

  getLearningGenerationJob: (
    id: string,
  ): Promise<ApiResult<Wire.LearningGenerationJob>> =>
    learningCall('get_learning_generation_job', { id }),

  getLearningProgram: (
    id: string,
  ): Promise<ApiResult<Wire.LearningProgramDto>> =>
    learningCall('get_learning_program', { id }),

  getLearningLessonEvidence: (
    programId: string,
    lessonId: string,
  ): Promise<ApiResult<Wire.LearningLessonEvidenceDto | null>> =>
    learningCall('get_learning_lesson_evidence', { programId, lessonId }),

  getLearningOutlineEvidence: (
    programId: string,
  ): Promise<ApiResult<Wire.LearningOutlineEvidenceDto | null>> =>
    learningCall('get_learning_outline_evidence', { programId }),

  generateLearningProgram: (
    request: Wire.GenerateLearningProgramRequestDto,
    options?: {
      requestId: string;
      onProgress: (update: Wire.LearningOutlineProgressDto) => void;
    },
  ): Promise<ApiResult<Wire.LearningProgramDto>> => {
    const onProgress = new Channel<Wire.LearningOutlineProgressDto>(
      options?.onProgress,
    );
    return learningCall('generate_learning_program', {
      request,
      requestId: options?.requestId ?? null,
      onProgress,
    });
  },

  repairLearningOutline: (
    request: Wire.RepairLearningOutlineRequestDto,
    options: {
      requestId: string;
      onProgress: (update: Wire.LearningOutlineProgressDto) => void;
    },
  ): Promise<ApiResult<Wire.LearningProgramDto>> => {
    const onProgress = new Channel<Wire.LearningOutlineProgressDto>(
      options.onProgress,
    );
    return learningCall('repair_learning_outline', {
      request,
      requestId: options.requestId,
      onProgress,
    });
  },

  cancelLearningOutline: (requestId: string): Promise<ApiResult<boolean>> =>
    learningCall('cancel_learning_outline', { requestId }),

  acceptLearningProgram: (
    request: Wire.AcceptLearningProgramRequestDto,
  ): Promise<ApiResult<Wire.LearningProgramDto>> =>
    learningCall('accept_learning_program', { request }),

  prepareLearningLesson: (
    request: Wire.PrepareLearningLessonRequestDto,
  ): Promise<ApiResult<Wire.LearningProgramDto>> =>
    learningCall('prepare_learning_lesson', { request }),

  completeLearningLesson: (
    request: Wire.CompleteLearningLessonRequestDto,
  ): Promise<ApiResult<Wire.LearningProgramDto>> =>
    learningCall('complete_learning_lesson', { request }),

  submitLearningAttempt: (
    request: Wire.SubmitLearningAttemptRequestDto,
  ): Promise<ApiResult<Wire.LearningProgramDto>> =>
    learningCall('submit_learning_attempt', { request }),

  deleteLearningProgram: (id: string): Promise<ApiResult<void>> =>
    learningCall('delete_learning_program', { id }),

  getLearningMemory: (id: string): Promise<ApiResult<Wire.LearningMemoryDto>> =>
    learningCall('get_learning_memory', { id }),

  ensureLearningLessonNote: (
    request: Wire.EnsureLearningLessonNoteRequestDto,
  ): Promise<ApiResult<Wire.LearningMemoryDto>> =>
    learningCall('ensure_learning_lesson_note', { request }),

  generateLearningCardDrafts: (
    request: Wire.GenerateLearningCardDraftsRequestDto,
  ): Promise<ApiResult<Wire.LearningMemoryDto>> =>
    learningCall('generate_learning_card_drafts', { request }),

  saveLearningCardDraft: (
    request: Wire.SaveLearningCardDraftRequestDto,
  ): Promise<ApiResult<Wire.LearningMemoryDto>> =>
    learningCall('save_learning_card_draft', { request }),

  acceptLearningCardDraft: (
    request: Wire.LearningCardDraftActionRequestDto,
  ): Promise<ApiResult<Wire.LearningMemoryDto>> =>
    learningCall('accept_learning_card_draft', { request }),

  discardLearningCardDraft: (
    request: Wire.LearningCardDraftActionRequestDto,
  ): Promise<ApiResult<Wire.LearningMemoryDto>> =>
    learningCall('discard_learning_card_draft', { request }),

  getLearningCanvasWorkspace: (
    id: string,
  ): Promise<ApiResult<Wire.LearningCanvasWorkspaceDto>> =>
    learningCall('get_learning_canvas_workspace', { id }),

  createLearningCanvas: (
    request: Wire.CreateLearningCanvasRequestDto,
  ): Promise<ApiResult<Wire.LearningCanvasWorkspaceDto>> =>
    learningCall('create_learning_canvas', { request }),

  saveLearningCanvas: (
    request: Wire.SaveLearningCanvasRequestDto,
  ): Promise<ApiResult<Wire.LearningCanvasWorkspaceDto>> =>
    learningCall('save_learning_canvas', { request }),

  createLearningCanvasSnapshot: (
    request: Wire.CreateLearningCanvasSnapshotRequestDto,
  ): Promise<ApiResult<Wire.LearningCanvasWorkspaceDto>> =>
    learningCall('create_learning_canvas_snapshot', { request }),

  restoreLearningCanvasSnapshot: (
    request: Wire.RestoreLearningCanvasSnapshotRequestDto,
  ): Promise<ApiResult<Wire.LearningCanvasWorkspaceDto>> =>
    learningCall('restore_learning_canvas_snapshot', { request }),

  getLearningSourceWorkspace: (
    id: string,
  ): Promise<ApiResult<Wire.LearningSourceWorkspaceDto>> =>
    learningCall('get_learning_source_workspace', { id }),

  getLearningSourceVersion: (
    request: Wire.GetLearningSourceVersionRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceVersionDto>> =>
    learningCall('get_learning_source_version', { request }),

  searchLearningSources: (
    request: Wire.SearchLearningSourcesRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceSearchResultDto[]>> =>
    learningCall('search_learning_sources', { request }),

  addLearningWebSource: (
    request: Wire.AddLearningWebSourceRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceWorkspaceDto>> =>
    learningCall('add_learning_web_source', { request }),

  addLearningDocumentSource: (
    request: Wire.AddLearningDocumentSourceRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceWorkspaceDto>> =>
    learningCall('add_learning_document_source', { request }),

  addLearningTextSource: (
    request: Wire.AddLearningTextSourceRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceWorkspaceDto>> =>
    learningCall('add_learning_text_source', { request }),

  refreshLearningSource: (
    request: Wire.RefreshLearningSourceRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceWorkspaceDto>> =>
    learningCall('refresh_learning_source', { request }),

  adoptLearningSourceVersion: (
    request: Wire.AdoptLearningSourceVersionRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceWorkspaceDto>> =>
    learningCall('adopt_learning_source_version', { request }),

  updateLearningSourcePolicy: (
    request: Wire.UpdateLearningSourcePolicyRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceWorkspaceDto>> =>
    learningCall('update_learning_source_policy', { request }),

  getLearningPracticeWorkspace: (
    id: string,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('get_learning_practice_workspace', { id }),

  getLearningPracticeSession: (
    id: string,
  ): Promise<ApiResult<Wire.LearningPracticeSessionDto>> =>
    learningCall('get_learning_practice_session', { id }),

  startLearningPracticeSession: (
    request: Wire.StartLearningPracticeSessionRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('start_learning_practice_session', { request }),

  saveLearningPracticeArtifact: (
    request: Wire.SaveLearningPracticeArtifactRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('save_learning_practice_artifact', { request }),

  changeLearningPracticeMode: (
    request: Wire.ChangeLearningPracticeModeRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('change_learning_practice_mode', { request }),

  openLearningPracticeSource: (
    request: Wire.OpenLearningPracticeSourceRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('open_learning_practice_source', { request }),

  requestLearningTutorResponse: (
    request: Wire.RequestLearningTutorResponseRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('request_learning_tutor_response', { request }),

  revealLearningPracticeSolution: (
    request: Wire.RevealLearningPracticeSolutionRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('reveal_learning_practice_solution', { request }),

  submitLearningPracticeAttempt: (
    request: Wire.SubmitLearningPracticeAttemptRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('submit_learning_practice_attempt', { request }),

  acceptLearningPracticeProposal: (
    request: Wire.DecideLearningPracticeProposalRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('accept_learning_practice_proposal', { request }),

  rejectLearningPracticeProposal: (
    request: Wire.DecideLearningPracticeProposalRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticeWorkspaceDto>> =>
    learningCall('reject_learning_practice_proposal', { request }),

  getLearningAssessmentWorkspace: (
    id: string,
  ): Promise<ApiResult<Wire.LearningAssessmentWorkspaceDto>> =>
    learningCall<Wire.LearningAssessmentWorkspaceDto>(
      'get_learning_assessment_workspace',
      { id },
    ),

  createLearningAssessmentBlueprint: (
    request: Wire.CreateLearningAssessmentBlueprintRequestDto,
  ): Promise<ApiResult<Wire.LearningAssessmentWorkspaceDto>> =>
    learningCall<Wire.LearningAssessmentWorkspaceDto>(
      'create_learning_assessment_blueprint',
      { request },
    ),

  startLearningAssessmentForm: (
    request: Wire.StartLearningAssessmentFormRequestDto,
  ): Promise<ApiResult<Wire.LearningAssessmentFormDto>> =>
    learningCall<Wire.LearningAssessmentFormDto>('start_learning_assessment_form', {
      request,
    }),

  getLearningAssessmentForm: (
    id: string,
  ): Promise<ApiResult<Wire.LearningAssessmentFormDto>> =>
    learningCall<Wire.LearningAssessmentFormDto>('get_learning_assessment_form', {
      id,
    }),

  saveLearningAssessmentResponse: (
    request: Wire.SaveLearningAssessmentResponseRequestDto,
  ): Promise<ApiResult<Wire.LearningAssessmentFormDto>> =>
    learningCall<Wire.LearningAssessmentFormDto>(
      'save_learning_assessment_response',
      { request },
    ),

  interruptLearningAssessmentForm: (
    request: Wire.MutateLearningAssessmentFormRequestDto,
  ): Promise<ApiResult<Wire.LearningAssessmentFormDto>> =>
    learningCall<Wire.LearningAssessmentFormDto>(
      'interrupt_learning_assessment_form',
      { request },
    ),

  submitLearningAssessmentForm: (
    request: Wire.MutateLearningAssessmentFormRequestDto,
  ): Promise<ApiResult<Wire.LearningAssessmentFormDto>> =>
    learningCall<Wire.LearningAssessmentFormDto>('submit_learning_assessment_form', {
      request,
    }),

  acceptLearningFollowUp: (
    request: Wire.DecideLearningFollowUpRequestDto,
  ): Promise<ApiResult<Wire.LearningAssessmentWorkspaceDto>> =>
    learningCall<Wire.LearningAssessmentWorkspaceDto>('accept_learning_follow_up', {
      request,
    }),

  dismissLearningFollowUp: (
    request: Wire.DecideLearningFollowUpRequestDto,
  ): Promise<ApiResult<Wire.LearningAssessmentWorkspaceDto>> =>
    learningCall<Wire.LearningAssessmentWorkspaceDto>('dismiss_learning_follow_up', {
      request,
    }),

  getLearningPracticalWorkspace: (
    programId: string,
  ): Promise<ApiResult<Wire.LearningPracticalWorkspaceDto>> =>
    learningCall<Wire.LearningPracticalWorkspaceDto>(
      'get_learning_practical_workspace',
      { programId },
    ),

  getLearningPracticalDraft: (
    request: Wire.GetLearningPracticalDraftRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticalDraftDto>> =>
    learningCall('get_learning_practical_draft', { request }),

  saveLearningPracticalDraft: (
    request: Wire.SaveLearningPracticalDraftRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticalDraftDto>> =>
    learningCall('save_learning_practical_draft', { request }),

  getLearningRuntimeCatalog: (): Promise<
    ApiResult<Wire.LearningRuntimePresetDto[]>
  > => learningCall<Wire.LearningRuntimePresetDto[]>('get_learning_runtime_catalog'),

  prepareLearningRuntimePreset: (
    request: Wire.PrepareLearningRuntimePresetRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticalWorkspaceDto>> =>
    learningCall<Wire.LearningPracticalWorkspaceDto>(
      'prepare_learning_runtime_preset',
      { request },
    ),

  saveLearningRuntimeProfile: (
    request: Wire.SaveLearningRuntimeProfileRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticalWorkspaceDto>> =>
    learningCall<Wire.LearningPracticalWorkspaceDto>(
      'save_learning_runtime_profile',
      { request },
    ),

  generateLearningPracticalActivity: (
    request: Wire.GenerateLearningPracticalActivityRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticalWorkspaceDto>> =>
    learningCall<Wire.LearningPracticalWorkspaceDto>(
      'generate_learning_practical_activity',
      { request },
    ),

  startLearningPracticalRun: (
    request: Wire.StartLearningPracticalRunRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticalRunDto>> =>
    learningCall<Wire.LearningPracticalRunDto>('start_learning_practical_run', {
      request,
    }),

  cancelLearningPracticalRun: (
    request: Wire.CancelLearningPracticalRunRequestDto,
  ): Promise<ApiResult<Wire.LearningPracticalRunDto>> =>
    learningCall<Wire.LearningPracticalRunDto>('cancel_learning_practical_run', {
      request,
    }),

  startLearningSimulation: (
    request: Wire.StartLearningSimulationRequestDto,
  ): Promise<ApiResult<Wire.LearningSimulationSessionDto>> =>
    learningCall<Wire.LearningSimulationSessionDto>('start_learning_simulation', {
      request,
    }),

  sendLearningSimulationTurn: (
    request: Wire.SendLearningSimulationTurnRequestDto,
  ): Promise<ApiResult<Wire.LearningSimulationSessionDto>> =>
    learningCall<Wire.LearningSimulationSessionDto>(
      'send_learning_simulation_turn',
      { request },
    ),

  finishLearningSimulation: (
    request: Wire.FinishLearningSimulationRequestDto,
  ): Promise<ApiResult<Wire.LearningSimulationSessionDto>> =>
    learningCall<Wire.LearningSimulationSessionDto>('finish_learning_simulation', {
      request,
    }),

  getLearningPortabilityWorkspace: (
    programId: string,
  ): Promise<ApiResult<Wire.LearningPortabilityWorkspaceDto>> =>
    learningCall<Wire.LearningPortabilityWorkspaceDto>(
      'get_learning_portability_workspace',
      { programId },
    ),

  exportLearningPack: (
    request: Wire.ExportLearningPackRequestDto,
  ): Promise<ApiResult<Wire.LearningPortabilityWorkspaceDto>> =>
    learningCall<Wire.LearningPortabilityWorkspaceDto>('export_learning_pack', {
      request,
    }),

  previewLearningPackImport: (
    request: Wire.PreviewLearningPackImportRequestDto,
  ): Promise<ApiResult<Wire.LearningPortabilityWorkspaceDto>> =>
    learningCall<Wire.LearningPortabilityWorkspaceDto>(
      'preview_learning_pack_import',
      { request },
    ),

  applyLearningPackImport: (
    request: Wire.ApplyLearningPackImportRequestDto,
  ): Promise<ApiResult<Wire.LearningPortabilityWorkspaceDto>> =>
    learningCall<Wire.LearningPortabilityWorkspaceDto>(
      'apply_learning_pack_import',
      { request },
    ),

  cancelLearningPackImportPreview: (
    request: Wire.CancelLearningPackImportPreviewRequestDto,
  ): Promise<ApiResult<Wire.LearningPortabilityWorkspaceDto>> =>
    learningCall<Wire.LearningPortabilityWorkspaceDto>(
      'cancel_learning_pack_import_preview',
      { request },
    ),

  deleteLearningSource: (
    request: Wire.DeleteLearningSourceRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceWorkspaceDto>> =>
    learningCall<Wire.LearningSourceWorkspaceDto>('delete_learning_source', {
      request,
    }),

  reimportLearningSource: (
    request: Wire.ReimportLearningSourceRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceWorkspaceDto>> =>
    learningCall<Wire.LearningSourceWorkspaceDto>('reimport_learning_source', {
      request,
    }),

  createLearningSourceSelector: (
    request: Wire.CreateLearningSourceSelectorRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceSelectorDto>> =>
    learningCall<Wire.LearningSourceSelectorDto>('create_learning_source_selector', {
      request,
    }),

  getLearningSourceSelector: (
    programId: string,
    selectorId: string,
  ): Promise<ApiResult<Wire.LearningSourceSelectorDto>> =>
    learningCall<Wire.LearningSourceSelectorDto>('get_learning_source_selector', {
      programId,
      selectorId,
    }),

  matchLearningSourceSelector: (
    request: Wire.MatchLearningSourceSelectorRequestDto,
  ): Promise<ApiResult<Wire.LearningQuoteMatch>> =>
    learningCall<Wire.LearningQuoteMatch>('match_learning_source_selector', {
      request,
    }),

  searchLearningSourcesSemantically: (
    request: Wire.SearchLearningSourcesSemanticallyRequestDto,
  ): Promise<ApiResult<Wire.LearningSourceSemanticSearchResultDto[]>> =>
    learningCall<Wire.LearningSourceSemanticSearchResultDto[]>(
      'search_learning_sources_semantically',
      { request },
    ),

  getLearningRecallWorkspace: (
    programId: string,
  ): Promise<ApiResult<Wire.LearningRecallWorkspaceDto>> =>
    learningCall<Wire.LearningRecallWorkspaceDto>('get_learning_recall_workspace', {
      programId,
    }),

  saveLearningRecallCard: (
    request: Wire.SaveLearningRecallCardRequestDto,
  ): Promise<ApiResult<Wire.LearningRecallWorkspaceDto>> =>
    learningCall<Wire.LearningRecallWorkspaceDto>('save_learning_recall_card', {
      request,
    }),

  decideLearningRecallDuplicate: (
    request: Wire.DecideLearningRecallDuplicateRequestDto,
  ): Promise<ApiResult<Wire.LearningRecallWorkspaceDto>> =>
    learningCall<Wire.LearningRecallWorkspaceDto>(
      'decide_learning_recall_duplicate',
      { request },
    ),

  reviewLearningRecallCard: (
    request: Wire.ReviewLearningRecallCardRequestDto,
  ): Promise<ApiResult<Wire.LearningRecallWorkspaceDto>> =>
    learningCall<Wire.LearningRecallWorkspaceDto>('review_learning_recall_card', {
      request,
    }),
};
