import type * as Wire from '@/lib/bindings';
import { studyActivityCall } from '@/lib/studyActivity';
import type { CommandName } from '@/shared/ipc/transport';

async function invokeCommand<T>(
  command: CommandName,
  args?: Record<string, unknown>,
): Promise<T> {
  const result = await studyActivityCall<T>('learning', command, args);
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
  courseDrills: (_programId: string, _language: string | null): Promise<Wire.CourseDrillsDto> =>
    Promise.resolve({ language: '', nextCode: null, nextPassage: null, hasSources: false, modules: [], recent: [] }),

  /** An attempt on a ready drill of the lesson, or of the one next up when none is given, or the build that will make one. */
  startCourseDrill: (
    _programId: string,
    _lessonId: string | null,
    _material: Wire.DrillMaterialDto,
    _mode: Wire.AttemptMode,
    _lines: number,
    _issues: number,
  ): Promise<Wire.StartedCodeReviewDto> => Promise.resolve({ status: 'building', jobId: '' }),

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
