import type {
  ContentEditHistory,
  ContentEditOutcome,
  ContentEditPreview,
  ContentEditQuery,
  ContentEditRequest,
  ContentEditSummary,
  ContentRef,
  EditableContent,
  RevertContentEditRequest,
} from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import { unwrapApiResult } from '@/types/api/result';

export const contentEditKeys = {
  open: (c: ContentRef) => ['content-edit', c.kind, c.key] as const,
  history: (c: ContentRef) => ['content-edit-history', c.kind, c.key] as const,
  list: (q: ContentEditQuery) => ['content-edits', q] as const,
};

export async function openContentEdit(content: ContentRef): Promise<EditableContent> {
  return unwrapApiResult(await apiCall('open_content_edit', { content }));
}

export async function previewContentEdit(request: ContentEditRequest): Promise<ContentEditPreview> {
  return unwrapApiResult(await apiCall('preview_content_edit', { request }));
}

export async function applyContentEdit(request: ContentEditRequest): Promise<ContentEditOutcome> {
  return unwrapApiResult(await apiCall('apply_content_edit', { request }));
}

export async function revertContentEdit(request: RevertContentEditRequest): Promise<ContentEditOutcome> {
  return unwrapApiResult(await apiCall('revert_content_edit', { request }));
}

export async function getContentEditHistory(content: ContentRef): Promise<ContentEditHistory> {
  return unwrapApiResult(await apiCall('get_content_edit_history', { content }));
}

export async function listContentEdits(query: ContentEditQuery): Promise<ContentEditSummary[]> {
  return unwrapApiResult(await apiCall('list_content_edits', { query }));
}
