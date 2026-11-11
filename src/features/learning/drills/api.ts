import type * as Wire from '@/lib/bindings';
import { studyActivityCall } from '@/lib/studyActivity';
import type { CommandName } from '@/shared/ipc/transport';

async function invokeCommand<C extends CommandName>(
  command: C,
  args: Wire.CommandArgs[C],
): Promise<Wire.CommandResults[C]> {
  const result = await studyActivityCall('learning', command, args);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}

/** The review drills' commands. Each throws the backend's message when it fails. */
export const drillsApi = {
  /** The languages a drill can be written in, in the catalogue's order. */
  codeReviewLanguages: (): Promise<string[]> =>
    invokeCommand('code_review_languages'),

  /**
   * A course's drills: the language code drills are offered in, what a start with no lesson comes to, its lessons
   * by module with their standing, and its latest attempts.
   */
  courseDrills: (programId: string, language: string | null): Promise<Wire.CourseDrillsDto> =>
    invokeCommand('course_drills', { programId, language }),

  /** An attempt on a ready drill of the lesson, or of the one next up when none is given, or the build that will make one. */
  startCourseDrill: (
    programId: string,
    lessonId: string | null,
    material: Wire.DrillMaterialDto,
    mode: Wire.AttemptMode,
    lines: number,
    issues: number,
  ): Promise<Wire.StartedCodeReviewDto> =>
    invokeCommand('start_course_drill', { programId, lessonId, material, mode, lines, issues }),

  submitCodeReview: (
    attemptId: string,
    findings: Wire.CodeReviewFindingDto[],
    fixFiles: Wire.CodeReviewFileDto[] | null,
    timeSpentMs: number,
  ): Promise<Wire.CodeReviewResultDto> =>
    invokeCommand('submit_code_review', {
      attemptId,
      findings,
      fixFiles,
      timeSpentMs,
    }),

  /** Where an attempt stands; once it is graded, the reveal comes with it. */
  codeReviewResult: (attemptId: string): Promise<Wire.CodeReviewResultDto> =>
    invokeCommand('code_review_result', { attemptId }),

  /** Grades an attempt that could not be graded again. */
  regradeCodeReview: (attemptId: string): Promise<Wire.CodeReviewResultDto> =>
    invokeCommand('regrade_code_review', { attemptId }),
};
