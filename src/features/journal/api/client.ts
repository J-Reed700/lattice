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
  /**
   * Retrieves the daily note for today's date.
   * Creates a new daily note if one doesn't exist for today.
   *
   * @returns Daily note object with content and metadata
   */
  getTodayNote: async (): Promise<ApiResult<DailyNote>> =>
    apiCall<Wire.DailyNoteCompatDto>('get_today_note'),

  /**
   * Quickly captures text to today's daily note.
   * Appends content to existing note or creates new note if needed.
   * Ideal for inbox-style quick capture workflows.
   *
   * @param content - Text content to append to today's note
   * @returns Which page the capture landed on, so the UI can name it
   */
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

  /**
   * Retrieves all daily notes within a date range.
   * Useful for calendar views and date-based navigation.
   *
   * @param startDate - Start date in ISO format (YYYY-MM-DD)
   * @param endDate - End date in ISO format (YYYY-MM-DD)
   * @returns Array of daily note objects within the range
   */
  getDailyNotesRange: async (
    startDate: string,
    endDate: string,
  ): Promise<ApiResult<DailyNote[]>> =>
    apiCall<Wire.DailyNoteCompatDto[]>('get_daily_notes_range', {
      request: { startDate, endDate },
    }),

  /**
   * Gets the daily note immediately before the specified date.
   * Returns null if no earlier note exists.
   *
   * @param currentDate - Reference date in ISO format (YYYY-MM-DD)
   * @returns Previous daily note or null
   */
  getPreviousDailyNote: async (
    currentDate: string,
  ): Promise<ApiResult<DailyNote | null>> =>
    apiCall<Wire.DailyNoteCompatDto | null>('get_previous_daily_note', {
      request: { currentDate },
    }),

  /**
   * Gets the daily note immediately after the specified date.
   * Returns null if no later note exists.
   *
   * @param currentDate - Reference date in ISO format (YYYY-MM-DD)
   * @returns Next daily note or null
   */
  getNextDailyNote: async (
    currentDate: string,
  ): Promise<ApiResult<DailyNote | null>> =>
    apiCall<Wire.DailyNoteCompatDto | null>('get_next_daily_note', {
      request: { currentDate },
    }),

  /**
   * Updates the content of an existing daily note.
   * Completely replaces the note's content with new text.
   *
   * @param noteId - Internal identifier of the daily note
   * @param content - New content to save
   * @returns Void on success
   */
  updateDailyNoteContent: async (
    noteId: string,
    content: string,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('update_daily_note_content', {
      request: { noteId, content },
    }),

  /**
   * Lists persisted notes in the Daily Notes workspace.
   *
   * With a `journalId`, only the pages that journal owns — which is what a
   * journal's sidebar wants. Without one, every page across every journal,
   * for the cross-journal surfaces (reference inbox, weekly synthesis).
   */
  listWorkspaceNotes: async (
    journalId?: string,
  ): Promise<ApiResult<ListWorkspaceNotesResponse>> =>
    apiCall<Wire.ListWorkspaceNotesResponseDto>('list_workspace_notes', {
      request: { journalId: journalId ?? null },
    }),

  /**
   * Creates a new persisted workspace note, owned by `journalId` when given.
   * An unowned page is unfiled and appears in no journal's page list.
   */
  createWorkspaceNote: async (
    title?: string,
    journalId?: string,
  ): Promise<ApiResult<WorkspaceNote>> =>
    apiCall<Wire.WorkspaceNoteDto>('create_workspace_note', {
      request: { title, journalId: journalId ?? null },
    }),

  /** Appends a reference atomically without replacing another editor's note. */
  captureReference: (
    request: Wire.CaptureReferenceRequestDto,
  ): Promise<ApiResult<Wire.CaptureReferenceResultDto>> =>
    apiCall('capture_reference', { request }),

  /** Saves an editor snapshot only when its revision is current. */
  updateWorkspaceNote: async (
    note: WorkspaceNote,
  ): Promise<ApiResult<WorkspaceNote>> =>
    apiCall<Wire.WorkspaceNoteDto>('update_workspace_note', { note }),

  /**
   * Deletes a persisted workspace note by id.
   */
  deleteWorkspaceNote: async (noteId: string): Promise<ApiResult<void>> =>
    apiCall<void>('delete_workspace_note', { request: { noteId } }),
};
