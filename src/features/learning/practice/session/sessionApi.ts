import { unwrap } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { apiCall, type CommandArgList, type CommandName } from '@/shared/ipc/transport';

async function call<C extends CommandName>(command: C, ...args: CommandArgList<C>): Promise<Wire.CommandResults[C]> {
  return unwrap(await apiCall(command, ...args));
}

/** One wrapper per engine command; each throws the backend's message when it fails. */
export const sessionApi = {
  startSession: (request: Wire.StartPracticeSessionRequestDto) =>
    call('start_practice_session', { request }),
  getSession: (request: Wire.PracticeSessionRequestDto) =>
    call('get_practice_session', { request }),
  listSessions: (request: Wire.ListPracticeSessionsRequestDto) =>
    call('list_practice_sessions', { request }),
  moveStage: (request: Wire.MovePracticeStageRequestDto) =>
    call('move_practice_stage', { request }),
  check: (request: Wire.CheckPracticePieceRequestDto) =>
    call('check_practice_piece', { request }),
  askHint: (request: Wire.AskPracticeHintRequestDto) =>
    call('ask_practice_hint', { request }),
  sendTurn: (request: Wire.SendPracticeTurnRequestDto) =>
    call('send_practice_turn', { request }),
  respond: (request: Wire.PracticeSessionRequestDto) =>
    call('respond_practice_session', { request }),
  observeDraft: (request: Wire.ObservePracticeDraftRequestDto) =>
    call('observe_practice_draft', { request }),
  setClock: (request: Wire.SetPracticeClockRequestDto) =>
    call('set_practice_clock', { request }),
  setSetting: (request: Wire.SetPracticeSettingRequestDto) =>
    call('set_practice_setting', { request }),
  revealSolution: (request: Wire.RevealPracticeSolutionRequestDto) =>
    call('reveal_practice_solution', { request }),
  finish: (request: Wire.FinishPracticeSessionRequestDto) =>
    call('finish_practice_session', { request }),
};
