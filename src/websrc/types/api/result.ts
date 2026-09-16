/**
 * API Result Infrastructure
 *
 * Discriminated union type for API responses, matching Rust backend structure.
 * Provides type-safe error handling and standardized error format.
 */

/**
 * Structured error from backend API
 *
 * Note: code is typed as string to accept any backend error code,
 * including codes not yet added to ErrorCode enum.
 * Frontend should check against ErrorCode constants for known codes.
 */
export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

/**
 * Success result wrapping data payload
 */
export interface ApiSuccess<T> {
  ok: true;
  data: T;
}

/**
 * Error result wrapping error details
 */
export interface ApiFailure {
  ok: false;
  error: ApiError;
}

/**
 * Discriminated union for all API responses
 *
 * Usage:
 * ```typescript
 * const result: ApiResult<User> = await apiCall(...);
 * if (result.ok) {
 *   console.log(result.data.name); // Type-safe access
 * } else {
 *   console.error(result.error.code); // Type-safe error handling
 * }
 * ```
 */
export type ApiResult<T> = ApiSuccess<T> | ApiFailure;

/**
 * Type guard to check if an ApiResult is successful
 */
export function isSuccess<T>(result: ApiResult<T>): result is ApiSuccess<T> {
  return result.ok === true;
}

/**
 * Type guard to check if an ApiResult is a failure
 */
export function isFailure<T>(result: ApiResult<T>): result is ApiFailure {
  return result.ok === false;
}
