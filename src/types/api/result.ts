/**
 * Structured error from the backend. Errors that never reached a command
 * (Tauri's own rejections, renderer exceptions) are carried as `UNKNOWN`.
 */
export type ApiError = import('../../lib/bindings').ApiError;

export interface ApiSuccess<T> {
  ok: true;
  data: T;
}

export interface ApiFailure {
  ok: false;
  error: string;
  details?: ApiError;
}

export type ApiResult<T> = ApiSuccess<T> | ApiFailure;

export function isSuccess<T>(result: ApiResult<T>): result is ApiSuccess<T> {
  return result.ok === true;
}

export function isFailure<T>(result: ApiResult<T>): result is ApiFailure {
  return result.ok === false;
}

/** Preserve structured backend details when crossing a promise-based boundary. */
export class ApiRequestError extends Error {
  readonly details?: ApiError;
  constructor(failure: ApiFailure) {
    super(failure.error);
    this.name = 'ApiRequestError';
    this.details = failure.details;
  }
}

export function unwrapApiResult<T>(result: ApiResult<T>): T {
  if (!result.ok) throw new ApiRequestError(result);
  return result.data;
}
