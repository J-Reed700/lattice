import { unwrap } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { apiCall, type CommandName } from '@/shared/ipc/transport';

async function call<T>(command: CommandName, request: unknown): Promise<T> {
  return unwrap(await apiCall<T>(command, { request }));
}

/** One wrapper per coding command; each throws the backend's message when it fails. */
export const codingApi = {
  startBuild: (request: Wire.StartCodingBuildRequestDto) =>
    call<Wire.StartCodingBuildResultDto>('start_coding_build', request),
  startSession: (request: Wire.StartCodingSessionRequestDto) =>
    call<Wire.CodingSessionDto>('start_coding_session', request),
  getSession: (request: Wire.CodingSessionRequestDto) =>
    call<Wire.CodingSessionDto>('get_coding_session', request),
  listSessions: (request: Wire.ListCodingSessionsRequestDto) =>
    call<Wire.CodingSessionSummaryDto[]>('list_coding_sessions', request),
  moveStage: (request: Wire.MoveCodingStageRequestDto) =>
    call<Wire.CodingSessionDto>('move_coding_stage', request),
  checkExample: (request: Wire.CheckCodingExampleRequestDto) =>
    call<Wire.CodingSessionDto>('check_coding_example', request),
  removeExample: (request: Wire.RemoveCodingExampleRequestDto) =>
    call<Wire.CodingSessionDto>('remove_coding_example', request),
  run: (request: Wire.RunCodingSessionRequestDto) =>
    call<Wire.CodingSessionDto>('run_coding_session', request),
  askHint: (request: Wire.AskCodingHintRequestDto) =>
    call<Wire.CodingSessionDto>('ask_coding_hint', request),
  sendTurn: (request: Wire.SendCodingTurnRequestDto) =>
    call<Wire.CodingSessionDto>('send_coding_turn', request),
  respond: (request: Wire.CodingSessionRequestDto) =>
    call<Wire.CodingSessionDto>('respond_coding_session', request),
  observeDraft: (request: Wire.ObserveCodingDraftRequestDto) =>
    call<Wire.CodingSessionDto>('observe_coding_draft', request),
  setClock: (request: Wire.SetCodingClockRequestDto) =>
    call<Wire.ClockDto>('set_coding_clock', request),
  setSetting: (request: Wire.SetCodingSettingRequestDto) =>
    call<Wire.CodingSessionDto>('set_coding_setting', request),
  revealSolution: (request: Wire.RevealCodingSolutionRequestDto) =>
    call<Wire.CodingSessionDto>('reveal_coding_solution', request),
  finish: (request: Wire.FinishCodingSessionRequestDto) =>
    call<Wire.CodingSessionDto>('finish_coding_session', request),

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
