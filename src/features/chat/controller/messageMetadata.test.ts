import { describe, expect, it } from 'vitest';

import type { ConversationMessage } from '@/types/conversation';

import {
  applyCapturedSources,
  applyVerificationPatch,
  deriveMessageMetadata,
  isCapturePending,
  isVerificationPending,
  markVerificationInterrupted,
  mergeImages,
  mergeStep,
  messageImages,
  normalizeChunkExcerpt,
  normalizeSource,
  parseImages,
  parseSources,
  parseVerification,
} from './messageMetadata';

const message = (id: string, metadata?: unknown, extra: Record<string, unknown> = {}) => ({
  id, conversationId: 'c1', role: 'assistant', content: 'Answer', status: 'completed', createdAt: '',
  metadata: metadata === undefined ? null : typeof metadata === 'string' ? metadata : JSON.stringify(metadata),
  ...extra,
}) as unknown as ConversationMessage;

const image = (ref: string, caption = 'A heron') => ({
  id: 'a'.repeat(32), ref, fileName: `${'b'.repeat(32)}.png`, mime: 'image/png', width: 10, height: 10, byteSize: 100,
  caption, sourceUrl: null, sourceTitle: null, origin: 'attachment', license: null, attribution: null,
});

describe('messageMetadata', () => {
  describe('normalizeChunkExcerpt', () => {
    it('accepts camel or snake case and clamps the score', () => {
      expect(normalizeChunkExcerpt({ chunk_id: 'k1', excerpt: ' Herons wade. ', chunk_index: 2, page_number: 7, score: -3 })).toEqual({
        chunkId: 'k1', excerpt: 'Herons wade.', section: undefined, chunkIndex: 2, pageNumber: 7, score: 0, highlights: undefined,
      });
      expect(normalizeChunkExcerpt({ chunkId: 'k1', excerpt: 'x', score: 'NaN' })?.score).toBe(0);
    });

    it('drops excerpts with no id or text', () => {
      expect(normalizeChunkExcerpt(null)).toBeNull();
      expect(normalizeChunkExcerpt('text')).toBeNull();
      expect(normalizeChunkExcerpt({ chunkId: 'k1', excerpt: '  ' })).toBeNull();
    });
  });

  describe('normalizeSource', () => {
    it('fills a source with nothing but a web address', () => {
      expect(normalizeSource({ file_path: 'https://example.org/heron', score: 0.4 })).toMatchObject({
        documentId: 'source:https://example.org/heron',
        chunkId: 'source:https://example.org/heron#chunk',
        fileName: 'https://example.org/heron',
        category: 'Web Article',
        mimeType: 'text/plain',
        score: 0.4,
        chunkExcerpts: [],
      });
    });

    it('reads snake case fields and excerpts', () => {
      const source = normalizeSource({
        document_id: 'd1', chunk_id: 'k1', file_name: 'Birds.pdf', file_path: '/lib/birds.pdf', mime_type: 'application/pdf',
        chunk_index: 3, page_number: 12, from_ocr: true, file_size_bytes: 9, modified_at: 'today', citation_id: 2, web_snapshot: { id: 's' },
        chunk_excerpts: [{ chunk_id: 'k1', excerpt: 'Wading birds' }, { chunk_id: '', excerpt: 'dropped' }],
      });
      expect(source).toMatchObject({ documentId: 'd1', category: 'Unknown', pageNumber: 12, fromOcr: true, citationId: 2, webSnapshot: { id: 's' } });
      expect(source?.chunkExcerpts).toHaveLength(1);
    });

    it('names an unknown source and rejects non-objects', () => {
      expect(normalizeSource({})).toMatchObject({ filePath: 'unknown://source', fileName: 'unknown://source' });
      expect(normalizeSource(undefined)).toBeNull();
    });
  });

  describe('parseSources', () => {
    it('keeps the valid ones when one entry does not parse', () => {
      const sources = parseSources([
        { documentId: 'd1', filePath: '/lib/a.md', fileName: 'a.md' },
        { documentId: 'd2', filePath: '/lib/b.md', fileName: 'b.md', score: 1, citationId: 'not-a-number' },
        null,
      ]);
      expect(sources.map((source) => source.documentId)).toContain('d1');
      expect(parseSources('nope')).toEqual([]);
    });
  });

  describe('parseVerification', () => {
    it('reads snake case', () => {
      expect(parseVerification({ enabled: 1, claims_evaluated: 3, supported_claims: 2, unsupported_claims: ['Rome fell in 1200'], grounded_ratio: 0.66 })).toMatchObject({
        enabled: true, claimsEvaluated: 3, supportedClaims: 2,
      });
    });

    it('is null for nothing or nonsense', () => {
      expect(parseVerification(null)).toBeNull();
      expect(parseVerification({ claimsEvaluated: 'many' })).toBeNull();
    });
  });

  describe('captured sources', () => {
    it('writes captured pages into metadata and the direct sources', () => {
      const messages = [
        message('m1', { capture: { pending: true } }, { sources: [] }),
        message('m2'),
      ];
      const patched = applyCapturedSources(messages, { messageId: 'm1', sources: [{ documentId: 'd1', filePath: '/a.md', fileName: 'a.md' }] });
      expect(JSON.parse(patched[0].metadata as string)).toMatchObject({ capture: { pending: false }, sources: [{ documentId: 'd1' }] });
      expect(patched[0].sources).toHaveLength(1);
      expect(patched[1]).toBe(messages[1]);
    });

    it('starts fresh metadata when none or broken, and leaves sources it never had', () => {
      const [fromNone] = applyCapturedSources([message('m1')], { messageId: 'm1', sources: [] });
      expect(JSON.parse(fromNone.metadata as string)).toEqual({ sources: [], capture: { pending: false } });
      expect('sources' in fromNone && fromNone.sources).toBeFalsy();
      const [fromBroken] = applyCapturedSources([message('m1', '{broken')], { messageId: 'm1', sources: [] });
      expect(JSON.parse(fromBroken.metadata as string).capture).toEqual({ pending: false });
    });

    it('knows when a capture is still running', () => {
      expect(isCapturePending(message('m1', { capture: { pending: true } }))).toBe(true);
      expect(isCapturePending(message('m1', { capture: { pending: false } }))).toBe(false);
      expect(isCapturePending(message('m1', '{broken'))).toBe(false);
      expect(isCapturePending(message('m1'))).toBe(false);
      expect(isCapturePending(undefined)).toBe(false);
    });
  });

  describe('verification', () => {
    it('patches a finished check and its turn into metadata', () => {
      const [patched, other] = applyVerificationPatch([message('m1', { sources: [] }), message('m2')], { messageId: 'm1', verification: { enabled: true }, turn: { steps: [] } });
      expect(JSON.parse(patched.metadata as string)).toEqual({ sources: [], verification: { enabled: true }, turn: { steps: [] } });
      expect(other.metadata).toBeNull();
      const [fromBroken] = applyVerificationPatch([message('m1', '{broken')], { messageId: 'm1', verification: { enabled: false } });
      expect(JSON.parse(fromBroken.metadata as string)).toEqual({ verification: { enabled: false } });
      const [fromNone] = applyVerificationPatch([message('m1')], { messageId: 'm1', verification: null });
      expect(JSON.parse(fromNone.metadata as string)).toEqual({ verification: null });
    });

    it('marks checks still pending as interrupted, and leaves the rest', () => {
      const pending = message('m1', { verification: { enabled: true, pending: true } });
      const done = message('m2', { verification: { enabled: true, pending: false } });
      const [interrupted, untouched] = markVerificationInterrupted([pending, done]);
      expect(JSON.parse(interrupted.metadata as string).verification).toEqual({ enabled: true, pending: false, interrupted: true });
      expect(untouched).toBe(done);
      expect(isVerificationPending(pending)).toBe(true);
      expect(isVerificationPending(message('m3', '{broken'))).toBe(false);
      expect(isVerificationPending(undefined)).toBe(false);
    });
  });

  describe('images', () => {
    it('keeps the well-formed pictures', () => {
      expect(parseImages([image('img-1'), { ref: 'bad' }])).toHaveLength(1);
      expect(parseImages(null)).toEqual([]);
    });

    it('replaces a picture by ref and appends new ones', () => {
      const merged = mergeImages([image('img-1', 'old')] as never, [image('img-1', 'new'), image('img-2')] as never);
      expect(merged.map((item) => `${item.ref}:${item.caption}`)).toEqual(['img-1:new', 'img-2:A heron']);
      expect(mergeImages(undefined, [image('img-3')] as never)).toHaveLength(1);
    });

    it('reads pictures streamed onto a message or stored in its metadata', () => {
      expect(messageImages({ tempId: 't', images: [image('img-1')] } as never)).toHaveLength(1);
      expect(messageImages(message('m1', { images: [image('img-2')] }) as never)).toHaveLength(1);
      expect(messageImages(message('m1') as never)).toEqual([]);
      expect(messageImages(message('m1', '{broken') as never)).toEqual([]);
    });
  });

  it('collects sources, checks, traces and turns per message, surviving broken metadata', () => {
    const derived = deriveMessageMetadata([
      message('m1', undefined, { sources: [{ documentId: 'd1', filePath: '/a.md', fileName: 'a.md' }] }),
      message('m2', { sources: [{ documentId: 'd2', filePath: '/b.md', fileName: 'b.md' }], verification: { enabled: true }, retrieval: 'nope', turn: 'nope' }),
      message('m3', '{broken'),
    ]);
    expect([...derived.sources.keys()]).toEqual(['m1', 'm2']);
    expect([...derived.verification.keys()]).toEqual(['m2']);
    expect(derived.retrieval.size).toBe(0);
    expect(derived.turn.size).toBe(0);
  });

  it('replaces a finished step in place and appends a new one', () => {
    const start = { id: 's1', kind: 'search', status: 'running' } as never;
    const finish = { id: 's1', kind: 'search', status: 'done' } as never;
    const next = { id: 's2', kind: 'read', status: 'running' } as never;
    expect(mergeStep(undefined, start)).toEqual([start]);
    expect(mergeStep([start, next], finish)).toEqual([finish, next]);
  });
});
