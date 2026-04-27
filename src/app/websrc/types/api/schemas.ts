/**
 * Zod Schemas for API Results
 *
 * Runtime validation for API responses using Zod.
 * Provides type-safe parsing and validation of backend responses.
 */

import { z } from 'zod';

import { ErrorCode } from './errorCodes';

/**
 * Schema for ApiError structure
 */
export const ApiErrorSchema = z.object({
  code: z.nativeEnum(ErrorCode),
  message: z.string(),
  details: z.record(z.unknown()).optional(),
});

/**
 * Factory function to create ApiResult schema for specific data type
 *
 * Usage:
 * ```typescript
 * const UserResultSchema = ApiResultSchema(UserSchema);
 * const result = UserResultSchema.parse(response);
 * ```
 */
export function ApiResultSchema<T extends z.ZodTypeAny>(dataSchema: T) {
  return z.discriminatedUnion('ok', [
    z.object({
      ok: z.literal(true),
      data: dataSchema,
    }),
    z.object({
      ok: z.literal(false),
      error: ApiErrorSchema,
    }),
  ]);
}
