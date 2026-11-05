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

/** The code review drills' commands. Each throws the backend's message when it fails. */
export const drillsApi = {
  /** The languages a drill can be written in, in the catalogue's order. */
  codeReviewLanguages: (): Promise<string[]> =>
    invokeCommand('code_review_languages'),

  /** An attempt on a ready drill, or the build that will make one. */
  startCodeReviewDrill: (
    language: string,
    mode: Wire.AttemptMode,
    lines: number,
    patternsCount: number,
  ): Promise<Wire.StartedCodeReviewDto> =>
    invokeCommand('start_code_review_drill', {
      language,
      mode,
      lines,
      patternsCount,
    }),

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
