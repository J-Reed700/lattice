/**
 * The three snapshot commands, unwrapped.
 *
 * A failed command throws, so TanStack Query and the mutation see an error
 * rather than a result object the caller has to remember to check.
 */

import { type ApiError, commands, type Result, type WebPageSnapshotDto as BindingsSnapshotDto } from '@/lib/bindings';

export type WebPageSnapshotDto = BindingsSnapshotDto & { readerText?: string | null };
export type { WebPageSnapshotStatus } from '@/lib/bindings';

function unwrap<T>(result: Result<T, ApiError>): T {
  if (result.status === 'ok') return result.data;
  throw new Error(result.error?.message ?? 'The snapshot request failed.');
}

/** The frozen copy of `url` in this conversation, or null when there is none. */
export async function getWebPageSnapshot(
  conversationId: string,
  url: string,
): Promise<WebPageSnapshotDto | null> {
  return unwrap(await commands.getWebPageSnapshot(conversationId, url));
}

/** Ensure a capture exists, or retry a failed one. Returns the row as it now stands. */
export async function captureWebPageSnapshot(
  conversationId: string,
  url: string,
): Promise<WebPageSnapshotDto> {
  return unwrap(await commands.captureWebPageSnapshot(conversationId, url));
}

/** Every frozen copy in the conversation, for "Open frozen copy". */
export async function listWebPageSnapshots(conversationId: string): Promise<WebPageSnapshotDto[]> {
  return unwrap(await commands.listWebPageSnapshots(conversationId));
}
