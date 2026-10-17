import { describe, expect, it } from 'vitest';

import {
  fromConversationLinkedDocument,
  fromConversationWebSource,
  fromDocumentMetadata,
  fromFilePath,
  fromJournalSource,
  fromLearningEvidencePassage,
  fromLearningSource,
  fromRecentDocument,
  fromSearchResult,
  fromStudySource,
} from '@/features/reading/model/sourceAdapter';
import type {
  ConversationLinkedDocumentDto,
  LearningEvidencePassageDto,
  LearningSourceLibraryItemDto,
  LearningSourceVersionDto,
  RecentDocument,
  StudySourceDto,
} from '@/lib/bindings';
import type { DocumentMetadata, SearchResult } from '@/types';
import type { SourceWithMetadata } from '@/types/conversation';

describe('sourceAdapter canonical mappings', () => {
  describe('fromFilePath', () => {
    it('maps local document path', () => {
      const target = fromFilePath('/path/to/research.pdf');
      expect(target.locator).toBeNull();
      expect(target.source.fileName).toBe('research.pdf');
      expect(target.source.filePath).toBe('/path/to/research.pdf');
      expect(target.source.mimeType).toBe('application/pdf');
      expect(target.source.category).toBe('document');
    });

    it('maps web url with custom title', () => {
      const target = fromFilePath('https://example.com/story', 'Custom Story Title');
      expect(target.locator).toBeNull();
      expect(target.source.documentId).toBe('web:https://example.com/story');
      expect(target.source.fileName).toBe('Custom Story Title');
      expect(target.source.filePath).toBe('https://example.com/story');
      expect(target.source.mimeType).toBe('text/html');
      expect(target.source.category).toBe('web');
    });
  });

  describe('fromDocumentMetadata', () => {
    it('maps library DocumentMetadata', () => {
      const doc = {
        id: 'doc-uuid-1',
        fileName: 'annual_report.docx',
        filePath: '/vault/annual_report.docx',
        fileType: 'docx',
        category: 'document',
        createdAt: '2026-01-01T00:00:00Z',
        modifiedAt: '2026-02-01T00:00:00Z',
        indexedAt: '2026-02-01T00:00:00Z',
      } as unknown as DocumentMetadata;
      const target = fromDocumentMetadata(doc);
      expect(target.locator).toBeNull();
      expect(target.source.documentId).toBe('doc-uuid-1');
      expect(target.source.fileName).toBe('annual_report.docx');
      expect(target.source.filePath).toBe('/vault/annual_report.docx');
      expect(target.source.mimeType).toBe(
        'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
      );
      expect(target.source.category).toBe('document');
    });
  });

  describe('fromSearchResult', () => {
    it('maps SearchResult with page, chunk position, and excerpt locator', () => {
      const result = {
        id: 'chunk-99',
        documentId: 'doc-search-1',
        title: 'Quantum Optics Handbook',
        content: 'Photon entanglement in optical cavities',
        score: 0.92,
        position: 4,
        highlights: ['entanglement'],
        metadata: {
          path: '/vault/quantum.pdf',
          file_type: 'pdf',
          file_size: 4096,
          page_number: 12,
          section: 'Chapter 2: Photons',
        },
      } as unknown as SearchResult;
      const target = fromSearchResult(result);
      expect(target.source.documentId).toBe('doc-search-1');
      expect(target.source.fileName).toBe('Quantum Optics Handbook');
      expect(target.source.filePath).toBe('/vault/quantum.pdf');
      expect(target.source.mimeType).toBe('application/pdf');
      expect(target.source.pageNumber).toBe(12);
      expect(target.source.chunkIndex).toBe(4);

      expect(target.locator).toEqual({
        text: 'Photon entanglement in optical cavities',
        chunkId: 'chunk-99',
        chunkIndex: 4,
        page: 12,
        highlights: ['entanglement'],
        label: 'PDF p. 12',
      });
    });
  });

  describe('fromRecentDocument', () => {
    it('maps dashboard RecentDocument', () => {
      const recent = {
        id: 'recent-1',
        documentId: 'doc-rec-1',
        documentName: 'Project Notes.md',
        documentPath: '/vault/notes/Project Notes.md',
        fileType: 'md',
        lastAccessedAt: '2026-10-10T12:00:00Z',
      } as unknown as RecentDocument;
      const target = fromRecentDocument(recent);
      expect(target.locator).toBeNull();
      expect(target.source.documentId).toBe('doc-rec-1');
      expect(target.source.fileName).toBe('Project Notes.md');
      expect(target.source.filePath).toBe('/vault/notes/Project Notes.md');
      expect(target.source.mimeType).toBe('text/markdown');
    });
  });

  describe('fromStudySource', () => {
    it('maps StudySourceDto with excerpt into locator', () => {
      const study: StudySourceDto = {
        documentId: 'study-doc-1',
        chunkId: 'study-chunk-1',
        fileName: 'Cell Biology.pdf',
        filePath: '/vault/cell_bio.pdf',
        excerpt: 'Mitochondria are the powerhouse of the cell.',
      };
      const target = fromStudySource(study, 2, 'conv-study-1');
      expect(target.source.documentId).toBe('study-doc-1');
      expect(target.source.chunkId).toBe('study-chunk-1');
      expect(target.source.citationId).toBe(3);
      expect(target.source.conversationId).toBe('conv-study-1');
      expect(target.locator).toEqual({
        text: 'Mitochondria are the powerhouse of the cell.',
        chunkId: 'study-chunk-1',
      });
    });

    it('maps StudySourceDto with web URL', () => {
      const study = {
        fileName: 'Cell Biology Online',
        url: 'https://example.org/cell-biology',
        excerpt: 'Mitochondria generate ATP.',
      } as unknown as StudySourceDto;
      const target = fromStudySource(study, 0);
      expect(target.source.filePath).toBe('https://example.org/cell-biology');
      expect(target.source.category).toBe('web');
      expect(target.source.mimeType).toBe('text/html');
      expect(target.locator?.text).toBe('Mitochondria generate ATP.');
    });
  });

  describe('fromConversationLinkedDocument', () => {
    it('maps ConversationLinkedDocumentDto', () => {
      const linked = {
        conversationId: 'conv-123',
        documentId: 'doc-link-1',
        fileName: 'Specs.pdf',
        filePath: '/vault/Specs.pdf',
        fileType: 'pdf',
        fileSizeBytes: 1024,
        category: 'document',
        indexedAt: '2026-09-01T00:00:00Z',
        lastReferencedAt: '2026-10-01T00:00:00Z',
      } as unknown as ConversationLinkedDocumentDto;
      const target = fromConversationLinkedDocument(linked, 'conv-123');
      expect(target.locator).toBeNull();
      expect(target.source.documentId).toBe('doc-link-1');
      expect(target.source.conversationId).toBe('conv-123');
      expect(target.source.mimeType).toBe('application/pdf');
    });
  });

  describe('fromConversationWebSource', () => {
    it('creates web source with webSnapshot when full content is provided', () => {
      const target = fromConversationWebSource(
        {
          url: 'https://example.com/article',
          title: 'Deep Learning Review',
          excerpt: 'Short excerpt',
          content: 'Full article text explaining deep learning architectures.',
          fetchedAt: '2026-10-12T00:00:00Z',
          truncated: false,
        },
        'conv-456',
      );
      expect(target.source.filePath).toBe('https://example.com/article');
      expect(target.source.fileName).toBe('Deep Learning Review');
      expect(target.source.conversationId).toBe('conv-456');
      expect(target.source.webSnapshot?.text).toBe(
        'Full article text explaining deep learning architectures.',
      );
      expect(target.source.webSnapshot?.truncated).toBe(false);
    });

    it('enriches from existing sources when available', () => {
      const existing: SourceWithMetadata = {
        documentId: 'web:https://example.com/article',
        chunkId: 'web:https://example.com/article',
        fileName: 'Cached Review',
        filePath: 'https://example.com/article',
        mimeType: 'text/html',
        category: 'web',
        content: 'Existing cached body',
        score: 1,
        fileSizeBytes: 0,
        modifiedAt: '2026-10-01T00:00:00Z',
        webSnapshot: {
          url: 'https://example.com/article',
          title: 'Cached Review',
          text: 'Existing cached body',
          fetchedAt: '2026-10-01T00:00:00Z',
          truncated: false,
        },
      };

      const target = fromConversationWebSource(
        { url: 'https://example.com/article/' },
        'conv-789',
        [existing],
      );
      expect(target.source.fileName).toBe('Cached Review');
      expect(target.source.conversationId).toBe('conv-789');
      expect(target.source.webSnapshot?.text).toBe('Existing cached body');
    });
  });

  describe('fromJournalSource', () => {
    it('maps local journal source', () => {
      const target = fromJournalSource(
        {
          fileName: 'Lab Log',
          filePath: '/vault/journals/log.md',
          excerpt: 'Observed 50% increase in reaction rate.',
          citationIndex: 2,
        },
        'conv-j',
      );
      expect(target.source.filePath).toBe('/vault/journals/log.md');
      expect(target.source.citationId).toBe(2);
      expect(target.locator).toEqual({
        text: 'Observed 50% increase in reaction rate.',
        chunkId: '/vault/journals/log.md#2',
      });
    });

    it('maps web journal source and matches existing source', () => {
      const existing: SourceWithMetadata = {
        documentId: 'web:https://example.org/nature',
        chunkId: 'web:https://example.org/nature',
        fileName: 'Nature Article',
        filePath: 'https://example.org/nature',
        mimeType: 'text/html',
        category: 'web',
        content: 'Full nature text',
        score: 1,
        fileSizeBytes: 0,
        modifiedAt: '2026-10-01T00:00:00Z',
      };
      const target = fromJournalSource(
        {
          fileName: 'Nature Citation',
          filePath: 'https://example.org/nature',
          excerpt: 'Nature excerpt quote',
          citationIndex: 1,
        },
        'conv-j2',
        [existing],
      );
      expect(target.source.fileName).toBe('Nature Article');
      expect(target.source.citationId).toBe(1);
      expect(target.locator?.text).toBe('Nature excerpt quote');
    });
  });

  describe('fromLearningSource', () => {
    it('maps learning library item with version detail', () => {
      const item = {
        id: 'learn-src-1',
        origin: 'https://learn.example.com/course',
        requestedUrl: 'https://learn.example.com/course',
        kind: 'web',
        freshnessPolicy: 'manual',
        activeVersionId: 'v-1',
        pendingVersionId: null,
        revision: 1,
        deletedAt: null,
        deletionReason: null,
        activeVersion: null,
        pendingVersion: null,
        versions: [],
        latestCheck: null,
        checks: [],
        createdAt: 1760000000000,
        updatedAt: 1760000000000,
      } as unknown as LearningSourceLibraryItemDto;

      const detail = {
        version: {
          id: 'v-1',
          versionNumber: 1,
          title: 'Neural Networks 101',
          publisher: null,
          resolvedUrl: 'https://learn.example.com/course/intro',
          excerpt: 'Introductory neural networks lecture',
          contentSha256: 'hash',
          wordCount: 100,
          truncated: false,
          extractionVersion: 'v1',
          acquiredAt: 1760000000000,
        },
        fullText: 'Full comprehensive text for neural networks lecture.',
      } as unknown as LearningSourceVersionDto;

      const target = fromLearningSource(item, detail);
      expect(target.source.fileName).toBe('Neural Networks 101');
      expect(target.source.filePath).toBe('https://learn.example.com/course/intro');
      expect(target.source.mimeType).toBe('text/html');
      expect(target.source.webSnapshot?.text).toBe(
        'Full comprehensive text for neural networks lecture.',
      );
    });
  });

  describe('fromLearningEvidencePassage', () => {
    it('maps learning evidence passage with locator', () => {
      const passage: LearningEvidencePassageDto = {
        sourceVersionId: 'sv-123',
        title: 'Evidence on Backpropagation',
        text: 'Backpropagation computes the gradient of the loss function.',
        startByte: 100,
        endByte: 165,
        url: 'https://learn.example.com/backprop',
        retrievalKind: 'lexical',
      };

      const target = fromLearningEvidencePassage(passage, 0, 'Full backpropagation textbook text');
      expect(target.source.documentId).toBe('web:https://learn.example.com/backprop');
      expect(target.source.fileName).toBe('Evidence on Backpropagation');
      expect(target.source.webSnapshot?.text).toBe('Full backpropagation textbook text');
      expect(target.locator).toEqual({
        text: 'Backpropagation computes the gradient of the loss function.',
        chunkId: 'sv-123:100-165',
      });
    });
  });
});
