import { unwrap } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { apiCall, type CommandName } from '@/shared/ipc/transport';

async function call<T>(command: CommandName, args: { request: unknown }): Promise<T> {
  return unwrap(await apiCall<T>(command, args));
}

/** Program's own commands; each throws the backend's message when it fails. */
export const programApi = {
  startBuild: (request: Wire.StartPracticeBuildRequestDto) =>
    call<Wire.StartPracticeBuildResultDto>('start_practice_build', { request }),
  checkExample: (request: Wire.CheckCodingExampleRequestDto) =>
    call<Wire.PracticeSessionDto>('check_coding_example', { request }),
  removeExample: (request: Wire.RemoveCodingExampleRequestDto) =>
    call<Wire.PracticeSessionDto>('remove_coding_example', { request }),
  runInput: (request: Wire.RunCodingInputRequestDto) =>
    call<Wire.PracticeSessionDto>('run_coding_input', { request }),

  /** The revision of the activity whose draft the learner's code is saved under. */
  async activityRevision(programId: string, activityId: string): Promise<number> {
    const workspace = unwrap(
      await apiCall<Wire.LearningPracticalWorkspaceDto>('get_learning_practical_workspace', { programId }),
    );
    const activity = workspace.activities.find((candidate) => candidate.id === activityId);
    if (!activity) throw new Error('This problem is no longer in the course.');
    return activity.revision;
  },
};
