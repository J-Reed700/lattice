import { unwrap } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { apiCall, type CommandName } from '@/shared/ipc/transport';

async function call<T>(command: CommandName, args: { request: unknown }): Promise<T> {
  return unwrap(await apiCall<T>(command, args));
}

/** One wrapper per engine command; each throws the backend's message when it fails. */
export const sessionApi = {
  startSession: (request: Wire.StartPracticeSessionRequestDto) =>
    call<Wire.PracticeSessionDto>('start_practice_session', { request }),
  getSession: (request: Wire.PracticeSessionRequestDto) =>
    call<Wire.PracticeSessionDto>('get_practice_session', { request }),
  listSessions: (request: Wire.ListPracticeSessionsRequestDto) =>
    call<Wire.PracticeSessionSummaryDto[]>('list_practice_sessions', { request }),
  moveStage: (request: Wire.MovePracticeStageRequestDto) =>
    call<Wire.PracticeSessionDto>('move_practice_stage', { request }),
  check: (request: Wire.CheckPracticePieceRequestDto) =>
    call<Wire.PracticeSessionDto>('check_practice_piece', { request }),
  askHint: (request: Wire.AskPracticeHintRequestDto) =>
    call<Wire.PracticeSessionDto>('ask_practice_hint', { request }),
  sendTurn: (request: Wire.SendPracticeTurnRequestDto) =>
    call<Wire.PracticeSessionDto>('send_practice_turn', { request }),
  respond: (request: Wire.PracticeSessionRequestDto) =>
    call<Wire.PracticeSessionDto>('respond_practice_session', { request }),
  observeDraft: (request: Wire.ObservePracticeDraftRequestDto) =>
    call<Wire.PracticeSessionDto>('observe_practice_draft', { request }),
  setClock: (request: Wire.SetPracticeClockRequestDto) =>
    call<Wire.ClockDto>('set_practice_clock', { request }),
  setSetting: (request: Wire.SetPracticeSettingRequestDto) =>
    call<Wire.PracticeSessionDto>('set_practice_setting', { request }),
  revealSolution: (request: Wire.RevealPracticeSolutionRequestDto) =>
    call<Wire.PracticeSessionDto>('reveal_practice_solution', { request }),
  finish: (request: Wire.FinishPracticeSessionRequestDto) =>
    call<Wire.PracticeSessionDto>('finish_practice_session', { request }),
};
