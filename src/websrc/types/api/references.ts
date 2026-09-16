/**
 * Passage references — barrel surface for the Reference inbox (contract §4.3).
 *
 * `PassageReferenceDto` and `CreatePassageReferenceRequest` are declared once,
 * in `passageReferences.ts` (Track A's reading surface writes them, Track C's
 * inbox reads them). One entity, one type — CLAUDE.md SSOT rule 4. This file
 * re-exports them through the `types/api` barrel and adds the annotation-update
 * request the inbox needs.
 */
export type {
  PassageReferenceDto,
  CreatePassageReferenceRequest,
} from './passageReferences';

/**
 * Annotation-only update. Both fields are **replaced** on every call: the
 * backend writes `SET title = ?, note = ?`, so omitting one clears it. Always
 * send both.
 */
export interface UpdatePassageReferenceRequest {
  id: string;
  title?: string | null;
  note?: string | null;
}
