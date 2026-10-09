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
