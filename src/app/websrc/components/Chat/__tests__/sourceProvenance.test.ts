import { describe, expect, it } from 'vitest';

import {
  isReferencedSource,
  isVaultNoteSource,
  provenanceLabel,
  sourceProvenance,
} from '../sourceProvenance';

import type { SourceWithMetadata } from '../../../types/conversation';

const UUID = '3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d';

const source = (overrides: Partial<SourceWithMetadata> = {}): SourceWithMetadata => ({
  documentId: 'doc-1',
  chunkId: 'chunk-1',
  fileName: 'note.md',
  filePath: `/Users/x/Lattice/notes/${UUID}.md`,
  mimeType: 'text/markdown',
  category: 'Documentation',
  content: 'body',
  score: 0.5,
  fileSizeBytes: 100,
  modifiedAt: '2026-01-01T00:00:00Z',
  ...overrides,
});

describe('isVaultNoteSource', () => {
  it('is true for a note under an explicit vault path', () => {
    expect(
      isVaultNoteSource(`/Users/x/Lattice/notes/${UUID}.md`, '/Users/x/Lattice')
    ).toBe(true);
  });

  it('is true for a note when the vault path is unknown', () => {
    // `vaultPath: ''` means the default `~/Lattice`, which the frontend cannot
    // expand — the UUID basename is what identifies a written-back note.
    expect(isVaultNoteSource(`/Users/x/Lattice/notes/${UUID}.md`, '')).toBe(true);
  });

  it('tolerates a trailing slash on the vault path', () => {
    expect(
      isVaultNoteSource(`/Users/x/Lattice/notes/${UUID}.md`, '/Users/x/Lattice/')
    ).toBe(true);
  });

  it('is false for ordinary markdown that happens to live in a notes folder', () => {
    expect(isVaultNoteSource('/Users/x/Papers/notes/paper.md', '')).toBe(false);
  });

  it('is false for a path that merely starts with "notes"', () => {
    expect(isVaultNoteSource('/Users/x/notesomething/a.md', '')).toBe(false);
  });

  it('is false for an empty path', () => {
    expect(isVaultNoteSource('', '/Users/x/Lattice')).toBe(false);
  });
});

describe('isReferencedSource', () => {
  it('matches on chunk id', () => {
    expect(
      isReferencedSource({ documentId: 'doc-1', chunkId: 'chunk-1' }, [
        { documentId: 'doc-other', chunkId: 'chunk-1' },
      ])
    ).toBe(true);
  });

  it('matches on document id when the reference has no chunk', () => {
    expect(
      isReferencedSource({ documentId: 'doc-1', chunkId: 'chunk-9' }, [
        { documentId: 'doc-1', chunkId: null },
      ])
    ).toBe(true);
  });

  it('does not match a different chunk of a referenced document', () => {
    expect(
      isReferencedSource({ documentId: 'doc-1', chunkId: 'chunk-9' }, [
        { documentId: 'doc-1', chunkId: 'chunk-1' },
      ])
    ).toBe(false);
  });

  it('is false with no references', () => {
    expect(isReferencedSource({ documentId: 'doc-1', chunkId: 'chunk-1' }, [])).toBe(false);
  });
});

describe('sourceProvenance', () => {
  it('prefers journal over reference', () => {
    expect(
      sourceProvenance(source(), '', [{ documentId: 'doc-1', chunkId: 'chunk-1' }])
    ).toBe('journal');
  });

  it('reports a reference for an ordinary file', () => {
    expect(
      sourceProvenance(source({ filePath: '/Users/x/Papers/a.pdf' }), '', [
        { documentId: 'doc-1', chunkId: 'chunk-1' },
      ])
    ).toBe('reference');
  });

  it('reports nothing for an unremarkable citation', () => {
    expect(sourceProvenance(source({ filePath: '/Users/x/Papers/a.pdf' }), '', [])).toBeNull();
  });
});

describe('provenanceLabel', () => {
  it('maps each provenance to its line', () => {
    expect(provenanceLabel('journal')).toBe('from your journal');
    expect(provenanceLabel('reference')).toBe('from your references');
    expect(provenanceLabel(null)).toBeNull();
  });
});
