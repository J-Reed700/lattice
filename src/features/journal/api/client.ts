import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  DailyNote,
  WorkspaceNote,
  ListWorkspaceNotesResponse,
  QuickCaptureResultDto,
} from '@/types';

export const journalApi = {
  getTodayNote: async (): Promise<ApiResult<DailyNote>> =>
    apiCall<Wire.DailyNoteCompatDto>('get_today_note'),

  quickCapture: async (
    content: string,
    sources?: Wire.SourceDto[],
    conversationIds?: string[],
  ): Promise<ApiResult<QuickCaptureResultDto>> =>
    apiCall<Wire.QuickCaptureResultDto>('quick_capture', {
      content,
      ...(sources !== undefined ? { sources } : {}),
      ...(conversationIds !== undefined ? { conversationIds } : {}),
    }),

  getDailyNotesRange: async (
    startDate: string,
    endDate: string,
  ): Promise<ApiResult<DailyNote[]>> =>
    apiCall<Wire.DailyNoteCompatDto[]>('get_daily_notes_range', {
      request: { startDate, endDate },
    }),

  getPreviousDailyNote: async (
    currentDate: string,
  ): Promise<ApiResult<DailyNote | null>> =>
    apiCall<Wire.DailyNoteCompatDto | null>('get_previous_daily_note', {
      request: { currentDate },
    }),

  getNextDailyNote: async (
    currentDate: string,
  ): Promise<ApiResult<DailyNote | null>> =>
    apiCall<Wire.DailyNoteCompatDto | null>('get_next_daily_note', {
      request: { currentDate },
    }),

  updateDailyNoteContent: async (
    noteId: string,
    content: string,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('update_daily_note_content', {
      request: { noteId, content },
    }),

  listWorkspaceNotes: async (
    journalId?: string,
  ): Promise<ApiResult<ListWorkspaceNotesResponse>> =>
    apiCall<Wire.ListWorkspaceNotesResponseDto>('list_workspace_notes', {
      request: { journalId: journalId ?? null },
    }),

  createWorkspaceNote: async (
    title?: string,
    journalId?: string,
  ): Promise<ApiResult<WorkspaceNote>> =>
    apiCall<Wire.WorkspaceNoteDto>('create_workspace_note', {
      request: { title, journalId: journalId ?? null },
    }),

  captureReference: (
    request: Wire.CaptureReferenceRequestDto,
  ): Promise<ApiResult<Wire.CaptureReferenceResultDto>> =>
    apiCall('capture_reference', { request }),

  updateWorkspaceNote: async (
    note: WorkspaceNote,
  ): Promise<ApiResult<WorkspaceNote>> =>
    apiCall<Wire.WorkspaceNoteDto>('update_workspace_note', { note }),

  deleteWorkspaceNote: async (noteId: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_workspace_note', { request: { noteId } }),
};
