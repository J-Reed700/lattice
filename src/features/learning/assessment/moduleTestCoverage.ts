import type { LearningModuleDto, LearningModuleTestCoverageDto } from '@/lib/bindings';

/**
 * The lessons a module test asks about now: those with prepared test
 * questions. Mirrors the coverage the backend records with the attempt.
 */
export function moduleTestCoverage(module: LearningModuleDto): LearningModuleTestCoverageDto {
  return {
    coveredLessons: module.lessons.filter((lesson) => lesson.questions.some((question) => question.kind === 'test')).length,
    moduleLessons: module.lessons.length,
  };
}

export function isPartialCoverage(coverage: LearningModuleTestCoverageDto): boolean {
  return coverage.coveredLessons < coverage.moduleLessons;
}

/** "Covers 1 of 2 lessons in this module" — said in words, never by colour alone. */
export function coverageText(coverage: LearningModuleTestCoverageDto): string {
  return `Covers ${coverage.coveredLessons} of ${coverage.moduleLessons} ${coverage.moduleLessons === 1 ? 'lesson' : 'lessons'} in this module`;
}
