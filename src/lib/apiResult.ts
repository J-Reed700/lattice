import { type ApiResult, ApiRequestError, unwrapApiResult } from '@/types/api/result';

import type * as Wire from './bindings';
import { parseApiError } from './errorHandling';
import { getErrorMessage } from './errorUtils';

/** The value of a successful result; a failed one throws, keeping the backend's error details. */
export function unwrap<T>(result: ApiResult<T> | Wire.Result<T, unknown>): T {
  if ('ok' in result) return unwrapApiResult(result);
  if (result.status === 'ok') return result.data;
  if (result.error instanceof Error) throw result.error;
  throw new ApiRequestError({ ok: false, error: getErrorMessage(result.error), details: parseApiError(result.error) });
}

/** A fresh id for an operation, session or record. */
export function newId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID();
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (char) => {
    const digit = Math.floor(Math.random() * 16);
    return (char === 'x' ? digit : (digit & 0x3) | 0x8).toString(16);
  });
}

/** Whether the backend refused a write because the record changed since it was read. */
export function isConflict(error: unknown): boolean {
  if (typeof error !== 'object' || error === null) return false;
  if ('code' in error && error.code === 'CONFLICT') return true;
  if (!('details' in error)) return false;
  const { details } = error;
  return typeof details === 'object' && details !== null && 'code' in details && details.code === 'CONFLICT';
}
