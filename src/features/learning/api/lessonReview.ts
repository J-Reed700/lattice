import { Channel } from '@tauri-apps/api/core';

import type * as Wire from '@/lib/bindings';
import type { ApiResult } from '@/types';

import { learningCall } from './learningCall';

/** The lesson's claim review, and the edits and sources it applies to a published lesson. */
export const lessonReviewApi = {
  /** null before the lesson's first verification pass. */
  getLearningLessonClaimReview: (
    programId: string,
    lessonId: string,
  ): Promise<ApiResult<Wire.LessonClaimReviewDto | null>> =>
    learningCall('get_learning_lesson_claim_review', { programId, lessonId }),

  resolveLearningClaim: (
    request: Wire.ResolveLearningClaimRequestDto,
  ): Promise<ApiResult<Wire.ResolveLearningClaimResultDto>> =>
    learningCall('resolve_learning_claim', { request }),

  resolveLearningClaims: (
    request: Wire.ResolveLearningClaimsRequestDto,
  ): Promise<ApiResult<Wire.ResolveLearningClaimResultDto>> =>
    learningCall('resolve_learning_claims', { request }),

  removeLearningTeachingSentences: (
    request: Wire.RemoveLearningTeachingSentencesRequestDto,
  ): Promise<ApiResult<Wire.RemoveLearningTeachingSentencesResultDto>> =>
    learningCall('remove_learning_teaching_sentences', { request }),

  /**
   * Add a link, file, pasted text or saved passage as a course source and
   * check the claims in scope against it, reporting each stage (§5.3).
   */
  attachLearningClaimEvidence: (
    request: Wire.AttachLearningClaimEvidenceRequestDto,
    onProgress: (update: Wire.ClaimAttachProgressDto) => void,
  ): Promise<ApiResult<Wire.ClaimAttachReportDto>> =>
    learningCall('attach_learning_claim_evidence', {
      request,
      onProgress: new Channel<Wire.ClaimAttachProgressDto>(onProgress),
    }),

  /**
   * Add a link, file, pasted text or saved passage as a course source and
   * check every open claim of the lesson against it, reporting each stage.
   */
  attachLearningLessonSource: (
    request: Wire.AttachLearningLessonSourceRequestDto,
    onProgress: (update: Wire.ClaimAttachProgressDto) => void,
  ): Promise<ApiResult<Wire.ClaimAttachReportDto>> =>
    learningCall('attach_learning_lesson_source', {
      request,
      onProgress: new Channel<Wire.ClaimAttachProgressDto>(onProgress),
    }),

  /** "Keep as written" on teaching findings the repairs did not settle, or taking that back. */
  settleLearningTeachingFindings: (
    request: Wire.SettleLearningTeachingFindingsRequestDto,
  ): Promise<ApiResult<void>> =>
    learningCall('settle_learning_teaching_findings', { request }),

  /** Rewrites a section of a published lesson; with `dryRun` it only answers what saving would do. */
  editLearningLessonSection: (
    request: Wire.EditLearningLessonSectionRequestDto,
  ): Promise<ApiResult<Wire.LessonSectionEditDto>> =>
    learningCall('edit_learning_lesson_section', { request }),

  /** A published lesson's question, as the editor opens it. */
  getLearningLessonQuestion: (
    programId: string,
    lessonId: string,
    questionIndex: number,
  ): Promise<ApiResult<Wire.LessonQuestionDto>> =>
    learningCall('get_learning_lesson_question', { programId, lessonId, questionIndex }),

  /** Rewrites a question of a published lesson; with `dryRun` it only answers what saving would do. */
  editLearningLessonQuestion: (
    request: Wire.EditLearningLessonQuestionRequestDto,
  ): Promise<ApiResult<Wire.LessonQuestionEditDto>> =>
    learningCall('edit_learning_lesson_question', { request }),

  resumeLearningLessonReview: (
    request: Wire.ResumeLearningLessonReviewRequestDto,
  ): Promise<ApiResult<Wire.LearningGenerationJob>> =>
    learningCall('resume_learning_lesson_review', { request }),
};
