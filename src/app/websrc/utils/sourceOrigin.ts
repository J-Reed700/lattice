import type { SourceWithMetadata } from '../types/conversation';

/**
 * getSourceOrigin
 *
 * Classifies a citation source by origin so the UI can distinguish
 * user-authored captures (Journal entries, References) from original
 * corpus documents (papers, recipes, legal docs the user ingested).
 *
 * Returns a short typographic label to render as an eyebrow above the
 * citation's file name, or `null` when the source is an original corpus
 * document (the default, unbadged state).
 *
 * TODO(backend): The retrieval pipeline does not yet index workspace
 * notes or conversation captures as citeable sources. When it does,
 * extend this helper to consume the backend's authored/origin signal
 * (likely a new `origin` field on SourceDto, or a stable mime type /
 * category such as "Journal Entry"). Until then, the heuristic below
 * covers the forward-compatible shapes we expect.
 */
export type SourceOriginLabel =
  | 'From your Journal'
  | 'From your References'
  | null;

const WORKSPACE_NOTE_MIME = 'application/x-recall-workspace-note';
const REFERENCE_MIME = 'application/x-recall-reference';

const JOURNAL_PATH_HINTS = [
  '/workspace_notes/',
  '/workspace-notes/',
  '/recall/notes/',
  '/.recall/notes/',
];

const REFERENCE_PATH_HINTS = [
  '/references/',
  '/.recall/references/',
];

const isWebSource = (source: SourceWithMetadata): boolean => {
  const category = source.category?.toLowerCase() ?? '';
  return source.documentId.startsWith('web:') || category.includes('web article');
};

const pathMatches = (filePath: string, hints: readonly string[]): boolean => {
  if (!filePath) return false;
  const normalized = filePath.replace(/\\/g, '/').toLowerCase();
  return hints.some((hint) => normalized.includes(hint));
};

export function getSourceOrigin(source: SourceWithMetadata): SourceOriginLabel {
  if (isWebSource(source)) return null;

  const category = source.category ?? '';
  const mime = source.mimeType ?? '';
  const path = source.filePath ?? '';

  if (
    mime === WORKSPACE_NOTE_MIME ||
    category === 'Journal Entry' ||
    category === 'Workspace Note' ||
    pathMatches(path, JOURNAL_PATH_HINTS)
  ) {
    return 'From your Journal';
  }

  if (
    mime === REFERENCE_MIME ||
    category === 'Reference' ||
    pathMatches(path, REFERENCE_PATH_HINTS)
  ) {
    return 'From your References';
  }

  return null;
}
