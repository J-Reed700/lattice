import type { GetLearningSourceTrustRequestDto } from '@/lib/bindings';

export const LEARNING_PROGRAMS_KEY = ['learning-programs'] as const;
export const learningProgramKey = (id: string) =>
  ['learning-program', id] as const;
export const learningPlanKey = (programId: string) =>
  ['learning-plan', programId] as const;
export const learningOutlineEvidenceKey = (programId: string, revision: number) =>
  [...learningOutlineEvidencePrefix(programId), revision] as const;
export const learningLessonEvidenceKey = (programId: string, lessonId: string, revision: number) =>
  [...learningLessonEvidencePrefix(programId), lessonId, revision] as const;
export const learningOutlineEvidencePrefix = (programId: string) =>
  ['learning-outline-evidence', programId] as const;
export const learningLessonEvidencePrefix = (programId: string) =>
  ['learning-lesson-evidence', programId] as const;
export const learningMemoryKey = (programId: string) => ['learning-memory', programId] as const;
export const learningCanvasKey = (programId: string) => ['learning-canvas', programId] as const;
export const learningRecallKey = (programId: string) => ['learning-recall-v2', programId] as const;
export const learningPracticeWorkspaceKey = (programId: string) => ['learning-practice-workspace', programId] as const;
export const learningPracticalWorkspaceKey = (programId: string) => ['learning-practical-workspace', programId] as const;
export const learningAssessmentWorkspaceKey = (programId: string) => ['learning-assessment-workspace', programId] as const;
export const learningTangentsKey = (programId?: string | null) =>
  ['learning-tangents', programId ?? 'all'] as const;

/** Every claim review of every course: a site's trust change reaches them all. */
export const LEARNING_CLAIM_REVIEW_ROOT = ['learning-claim-review'] as const;
export const learningClaimReviewPrefix = (programId: string) =>
  [...LEARNING_CLAIM_REVIEW_ROOT, programId] as const;
export const learningClaimReviewKey = (programId: string, lessonId: string) =>
  [...learningClaimReviewPrefix(programId), lessonId] as const;
export const LEARNING_CHECKER_ACCURACY_KEY = ['learning-checker-accuracy'] as const;
export const learningVerifierAuditKey = (programId: string) => ['learning-verifier-audit', programId] as const;
export const learningLessonAuditKey = (programId: string, lessonId: string) =>
  ['learning-lesson-audit', programId, lessonId] as const;

/** Every source trust read: a site override moves levels in every course. */
export const LEARNING_SOURCE_TRUST_PREFIX = ['learning-source-trust'] as const;
export const learningSourceTrustKey = (request: GetLearningSourceTrustRequestDto) => [
  ...LEARNING_SOURCE_TRUST_PREFIX,
  request.programId,
  request.sourceIds.join('\n'),
  request.claim?.statement ?? null,
  request.claim?.sourceVersionIds.join('\n') ?? null,
  request.includeImpact,
] as const;
export const LEARNING_SOURCE_TRUST_OVERRIDES_KEY = ['learning-source-trust-overrides'] as const;
export const LEARNING_SOURCE_TRUST_CALIBRATION_KEY = ['learning-source-trust-calibration'] as const;
