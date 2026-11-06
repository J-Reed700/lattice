/**
 * Passage-reference types used by the Reference inbox.
 *
 * `PassageReferenceDto` and `CreatePassageReferenceRequest` are declared in
 * `passageReferences.ts`. This module re-exports them through the `types/api`
 * barrel and adds the annotation update used by the inbox.
 */
export type {
  PassageReferenceDto,
  CreatePassageReferenceRequest,
  CreatePassageReferenceRequestDto,
} from './passageReferences';

export type { UpdatePassageReferenceRequestDto } from '../../lib/bindings';

/**
 * Annotation-only update. Both fields are **replaced** on every call: the
 * backend writes `SET title = ?, note = ?`, so omitting one clears it. Always
 * send both.
 */
export type UpdatePassageReferenceRequest = import('../../lib/bindings').UpdatePassageReferenceRequestDto;
