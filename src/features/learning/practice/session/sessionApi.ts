import { unwrap } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { queryClient } from '@/lib/queryClient';
import { apiCall, type CommandArgList, type CommandName } from '@/shared/ipc/transport';

async function call<C extends CommandName>(command: C, ...args: CommandArgList<C>): Promise<Wire.CommandResults[C]> {
  return unwrap(await apiCall(command, ...args));
}

export function invalidatePracticeQueries(programId: string) {
  void queryClient.invalidateQueries({ queryKey: ['learning-exercise-sessions', programId] });
  void queryClient.invalidateQueries({ queryKey: ['learning-practice-due', programId] });
  void queryClient.invalidateQueries({ queryKey: ['learning-practice-fit', programId] });
}

/** One wrapper per engine command; each throws the backend's message when it fails. */
export const sessionApi = {
  startSession: async (request: Wire.StartPracticeSessionRequestDto) => {
    const result = await call('start_practice_session', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  getSession: (request: Wire.PracticeSessionRequestDto) =>
    call('get_practice_session', { request }),
  listSessions: (request: Wire.ListPracticeSessionsRequestDto) =>
    call('list_practice_sessions', { request }),
  moveStage: async (request: Wire.MovePracticeStageRequestDto) => {
    const result = await call('move_practice_stage', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  check: async (request: Wire.CheckPracticePieceRequestDto) => {
    const result = await call('check_practice_piece', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  askHint: async (request: Wire.AskPracticeHintRequestDto) => {
    const result = await call('ask_practice_hint', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  sendTurn: async (request: Wire.SendPracticeTurnRequestDto) => {
    const result = await call('send_practice_turn', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  respond: async (request: Wire.PracticeSessionRequestDto) => {
    const result = await call('respond_practice_session', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  observeDraft: async (request: Wire.ObservePracticeDraftRequestDto) => {
    const result = await call('observe_practice_draft', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  setClock: async (request: Wire.SetPracticeClockRequestDto) => {
    const result = await call('set_practice_clock', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  setSetting: async (request: Wire.SetPracticeSettingRequestDto) => {
    const result = await call('set_practice_setting', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  revealSolution: async (request: Wire.RevealPracticeSolutionRequestDto) => {
    const result = await call('reveal_practice_solution', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
  finish: async (request: Wire.FinishPracticeSessionRequestDto) => {
    const result = await call('finish_practice_session', { request });
    invalidatePracticeQueries(request.programId);
    return result;
  },
};
