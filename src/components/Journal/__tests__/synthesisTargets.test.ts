import { describe, expect, it, vi } from 'vitest';

import type { SourceDto } from '@/lib/bindings';
import type { SynthesisCitationDto } from '@/types/api/conversation';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { appendSynthesisContent, buildSynthesisBlock, weekPageTitle } from '../synthesisTargets';


vi.mock('@/lib/api', () => ({
  default: {
    listWorkspaceNotes: vi.fn(),
    createWorkspaceNote: vi.fn(),
    updateWorkspaceNote: vi.fn(),
  },
}));

describe('weekPageTitle', () => {
  it('is Monday-anchored across the whole week', () => {
    // Wednesday 2026-09-02 and the following Sunday 2026-09-06 share a week.
    const wednesday = new Date(2026, 8, 2);
    const sunday = new Date(2026, 8, 6);
    expect(weekPageTitle(wednesday)).toBe(weekPageTitle(sunday));
  });

  it('changes on the next Monday', () => {
    const sunday = new Date(2026, 8, 6);
    const monday = new Date(2026, 8, 7);
    expect(weekPageTitle(monday)).not.toBe(weekPageTitle(sunday));
  });

  it('formats as "Week of Sep 1" — no year, no weekday', () => {
    expect(weekPageTitle(new Date(2026, 8, 2))).toBe('Week of Aug 31');
    expect(weekPageTitle(new Date(2026, 8, 7))).toBe('Week of Sep 7');
  });
});

describe('buildSynthesisBlock', () => {
  const generatedAt = new Date(2026, 8, 6, 12, 0, 0);

  it('includes the heading, the count and the body', () => {
    const block = buildSynthesisBlock({
      heading: 'Past Week',
      entryCount: 3,
      synthesis: '### Entry Highlights\n- Something happened.',
      generatedAt,
    });
    expect(block).toContain('## Journal Synthesis · Past Week');
    expect(block).toContain('from 3 entries.');
    expect(block).toContain('- Something happened.');
  });

  it('pluralises a single entry correctly', () => {
    const block = buildSynthesisBlock({
      heading: 'This Conversation',
      entryCount: 1,
      synthesis: 'Body.',
      generatedAt,
    });
    expect(block).toContain('from 1 entry.');
  });

  it('omits the Sources heading when there are no citations', () => {
    const withNone = buildSynthesisBlock({
      heading: 'Past Week',
      entryCount: 1,
      synthesis: 'Body.',
      citations: [],
      generatedAt,
    });
    expect(withNone).not.toContain('### Sources');

    const citations: SynthesisCitationDto[] = [
      { kind: 'conversation', id: 'c1', title: 'Sleep study' },
      { kind: 'reference', id: 'p1', title: 'paper.pdf' },
      { kind: 'note', id: 'n1', title: 'Sep 4' },
    ];
    const withSome = buildSynthesisBlock({
      heading: 'Past Week',
      entryCount: 3,
      synthesis: 'Body.',
      citations,
      generatedAt,
    });
    expect(withSome).toContain('### Sources');
    expect(withSome).toContain('- Sleep study — conversation');
    expect(withSome).toContain('- paper.pdf — saved passage');
    expect(withSome).toContain('- Sep 4 — journal page');
  });

  it('preserves every source title in the provenance list', () => {
    const citations: SynthesisCitationDto[] = Array.from({ length: 23 }, (_v, i) => ({
      kind: 'conversation' as const,
      id: `c${i}`,
      title: `Conversation ${i}`,
    }));
    const block = buildSynthesisBlock({
      heading: 'Past Week',
      entryCount: 23,
      synthesis: 'Body.',
      citations,
      generatedAt,
    });
    const sourceLines = block
      .split('\n')
      .filter((line) => line.startsWith('- Conversation '));
    expect(sourceLines).toHaveLength(23);
    expect(block).toContain('- Conversation 22 — conversation');
  });
});

describe('appendSynthesisContent', () => {
  const note = {
    id: 'page', title: 'Notes', content: 'Earlier claim [1].', sources: [
      { documentId: 'old', chunkId: 'old-chunk', fileName: 'Earlier.pdf', filePath: '/Earlier.pdf',
        mimeType: 'application/pdf', category: 'PDF', content: 'Earlier evidence', score: 1,
        fileSizeBytes: 0, modifiedAt: '', citationId: 1, path: null, position: null },
    ],
  } as unknown as WorkspaceNote;
  const source = {
    documentId: 'new', chunkId: 'new-chunk', fileName: 'New.pdf', filePath: '/New.pdf',
    mimeType: 'application/pdf', category: 'PDF', content: 'New evidence', score: 1,
    fileSizeBytes: 0, modifiedAt: '', citationId: 1, path: null, position: null,
  } as SourceDto;

  it('remaps conflicting source IDs and just the matching block citations', () => {
    const updated = appendSynthesisContent(note, 'New claim [1], leave [8] alone.', [source]);
    expect(updated.content).toBe('Earlier claim [1].\n\nNew claim [2], leave [8] alone.');
    expect(updated.sources?.map((citation) => citation.citationId)).toEqual([1, 2]);
    expect(updated.sources?.[1]?.chunkId).toBe('new-chunk');
  });

  it('keeps citation IDs stable when the page has no collision', () => {
    const emptyNote = { ...note, content: '', sources: [] } as unknown as WorkspaceNote;
    const updated = appendSynthesisContent(emptyNote, 'Claim [7].', [{ ...source, citationId: 7 }]);
    expect(updated.content).toBe('Claim [7].');
    expect(updated.sources?.[0]?.citationId).toBe(7);
  });
});
