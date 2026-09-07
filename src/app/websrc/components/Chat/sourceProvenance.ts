/**
 * Where a cited source came from (BRIEF rank 2a).
 *
 * Two marks, both earned:
 * - `from your journal` — the file is one of the user's own workspace notes,
 *   written to `<vaultRoot>/notes/<uuid>.md` by vault write-through.
 * - `from your references` — the cited chunk is one the user saved.
 *
 * Nothing is inferred beyond that. In particular, ordinary markdown is never
 * marked as journal: the mark is correct when it fires, and simply does not
 * fire otherwise.
 */

import type { SourceWithMetadata } from '../../types/conversation';

export type SourceProvenance = 'journal' | 'reference' | null;

const NOTE_BASENAME =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.md$/i;

/**
 * True when a cited file is one of the user's own workspace notes.
 *
 * `vaultPath` may be an empty string, which means the default `~/Lattice` —
 * a path the frontend cannot expand. Hence the second, path-independent test:
 * a `/notes/` segment plus a UUID basename, which is exactly the shape
 * `features/vault/writeback.rs` writes and nothing else produces.
 */
export function isVaultNoteSource(filePath: string, vaultPath: string): boolean {
  if (!filePath) return false;
  const normalized = filePath.replace(/\\/g, '/');

  const root = vaultPath.replace(/\\/g, '/').replace(/\/$/, '');
  if (root && normalized.startsWith(`${root}/notes/`)) return true;

  if (!normalized.includes('/notes/')) return false;
  const basename = normalized.slice(normalized.lastIndexOf('/') + 1);
  return NOTE_BASENAME.test(basename);
}

/** True when the cited chunk (or, failing that, its document) was saved. */
export function isReferencedSource(
  source: Pick<SourceWithMetadata, 'documentId' | 'chunkId'>,
  references: Array<{ documentId: string; chunkId?: string | null }>
): boolean {
  if (references.length === 0) return false;
  return references.some((reference) => {
    if (reference.chunkId) return reference.chunkId === source.chunkId;
    return reference.documentId === source.documentId;
  });
}

export function sourceProvenance(
  source: SourceWithMetadata,
  vaultPath: string,
  references: Array<{ documentId: string; chunkId?: string | null }>
): SourceProvenance {
  if (isVaultNoteSource(source.filePath, vaultPath)) return 'journal';
  if (isReferencedSource(source, references)) return 'reference';
  return null;
}

export function provenanceLabel(provenance: SourceProvenance): string | null {
  if (provenance === 'journal') return 'from your journal';
  if (provenance === 'reference') return 'from your references';
  return null;
}
