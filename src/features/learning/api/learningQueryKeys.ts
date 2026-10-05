export const LEARNING_PROGRAMS_KEY = ['learning-programs'] as const;
export const learningProgramKey = (id: string) =>
  ['learning-program', id] as const;
export const learningPlanKey = (programId: string) =>
  ['learning-plan', programId] as const;
