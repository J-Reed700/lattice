import { type ApiError, commands, type Result, type WebPageSnapshotDto as BindingsSnapshotDto } from '@/lib/bindings';

export type WebPageSnapshotDto = BindingsSnapshotDto & { readerText?: string | null };
export type { WebPageSnapshotStatus } from '@/lib/bindings';

function unwrap<T>(result: Result<T, ApiError>): T {
  if (result.status === 'ok') return result.data;
  throw new Error(result.error?.message ?? 'The snapshot request failed.');
}

export async function getWebPageSnapshot(
  conversationId: string,
  url: string,
): Promise<WebPageSnapshotDto | null> {
  return unwrap(await commands.getWebPageSnapshot(conversationId, url));
}

export async function captureWebPageSnapshot(
  conversationId: string,
  url: string,
): Promise<WebPageSnapshotDto> {
  return unwrap(await commands.captureWebPageSnapshot(conversationId, url));
}

export async function listWebPageSnapshots(conversationId: string): Promise<WebPageSnapshotDto[]> {
  return unwrap(await commands.listWebPageSnapshots(conversationId));
}
