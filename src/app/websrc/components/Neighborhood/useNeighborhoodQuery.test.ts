import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { loadNeighborhood } from './useNeighborhoodQuery';


const getMentionsForDocument = vi.fn();
const getBacklinksForMention = vi.fn();
const findSimilarDocuments = vi.fn();
const listConversationsCitingDocument = vi.fn();

vi.mock('@/lib/api', () => ({
  default: {
    getMentionsForDocument: (...args: unknown[]) => getMentionsForDocument(...args),
    getBacklinksForMention: (...args: unknown[]) => getBacklinksForMention(...args),
    findSimilarDocuments: (...args: unknown[]) => findSimilarDocuments(...args),
    listConversationsCitingDocument: (...args: unknown[]) => listConversationsCitingDocument(...args),
  },
}));

const doc = (id: string, fileName: string): DocumentMetadata => ({
  id,
  fileName,
  filePath: `/vault/${fileName}`,
  fileType: 'md',
  category: 'document',
  language: 'en',
  modifiedAt: '2026-01-01',
  indexedAt: '2026-01-01',
  wordCount: 10,
});

const subject = doc('doc-subject', 'Subject.md');
const alpha = doc('doc-alpha', 'Alpha.md');
const beta = doc('doc-beta', 'Beta.md');
const documentsById = new Map([
  [subject.id, subject],
  [alpha.id, alpha],
  [beta.id, beta],
]);

describe('loadNeighborhood', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getMentionsForDocument.mockResolvedValue({ ok: true, data: { documentId: subject.id, mentions: [], count: 0 } });
    getBacklinksForMention.mockResolvedValue({ ok: true, data: { documentIds: [] } });
    findSimilarDocuments.mockResolvedValue({ ok: true, data: [] });
    listConversationsCitingDocument.mockResolvedValue({ ok: true, data: [] });
  });

  it('drops mention names that resolve to nothing', async () => {
    getMentionsForDocument.mockResolvedValue({
      ok: true,
      data: {
        documentId: subject.id,
        count: 2,
        mentions: [
          { id: 'm1', name: 'Alpha', mentionType: 'wikilink', documentId: subject.id, context: '', position: 0, createdAt: '' },
          { id: 'm2', name: 'Nowhere', mentionType: 'wikilink', documentId: subject.id, context: '', position: 0, createdAt: '' },
        ],
      },
    });

    const data = await loadNeighborhood(subject.id, subject.fileName, documentsById);
    expect(data.links.map((link) => link.documentId)).toEqual(['doc-alpha']);
  });

  it('dedupes a document that both links in and is linked to', async () => {
    getBacklinksForMention.mockResolvedValue({ ok: true, data: { documentIds: ['doc-alpha'] } });
    getMentionsForDocument.mockResolvedValue({
      ok: true,
      data: {
        documentId: subject.id,
        count: 1,
        mentions: [
          { id: 'm1', name: 'Alpha', mentionType: 'wikilink', documentId: subject.id, context: '', position: 0, createdAt: '' },
        ],
      },
    });

    const data = await loadNeighborhood(subject.id, subject.fileName, documentsById);
    expect(data.links).toHaveLength(1);
    expect(data.links[0]?.title).toBe('Alpha');
  });

  it('never lists the subject document as its own neighbour', async () => {
    getBacklinksForMention.mockResolvedValue({ ok: true, data: { documentIds: ['doc-subject', 'doc-beta'] } });
    findSimilarDocuments.mockResolvedValue({
      ok: true,
      data: [
        { documentId: 'doc-subject', title: 'Subject.md', filePath: null, score: 1 },
        { documentId: 'doc-beta', title: 'Beta.md', filePath: null, score: 0.4 },
      ],
    });

    const data = await loadNeighborhood(subject.id, subject.fileName, documentsById);
    expect(data.links.map((link) => link.documentId)).toEqual(['doc-beta']);
    expect(data.similar.map((hit) => hit.documentId)).toEqual(['doc-beta']);
  });

  it('lets one failing call empty its own section without failing the rest', async () => {
    findSimilarDocuments.mockResolvedValue({ ok: false, error: 'no embedding model' });
    listConversationsCitingDocument.mockResolvedValue({
      ok: true,
      data: [{ conversationId: 'conv-1', title: 'Where it came up', updatedAt: '', passageCount: 2 }],
    });
    getBacklinksForMention.mockResolvedValue({ ok: true, data: { documentIds: ['doc-alpha'] } });

    const data = await loadNeighborhood(subject.id, subject.fileName, documentsById);
    expect(data.similar).toEqual([]);
    expect(data.links).toHaveLength(1);
    expect(data.citedIn).toEqual([{ conversationId: 'conv-1', title: 'Where it came up' }]);
  });

  it('looks backlinks up by the title, not the file name', async () => {
    await loadNeighborhood(subject.id, subject.fileName, documentsById);
    expect(getBacklinksForMention).toHaveBeenCalledWith('Subject');
  });
});
