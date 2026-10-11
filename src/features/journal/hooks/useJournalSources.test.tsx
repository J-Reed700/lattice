import { renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';

import type { SnapshotMessage } from '@/types/api/dailyNotes';

import { collectEntrySources, getSourceOpenUrl, parseMessageSources, useJournalSources } from './useJournalSources';

function message(sources: unknown[], role: SnapshotMessage['role'] = 'assistant'): SnapshotMessage {
  return { id: crypto.randomUUID(), role, content: 'Reply', createdAt: '2026-10-08T12:00:00Z', metadata: JSON.stringify({ sources }) };
}
const source = { documentId: 'doc-a', filePath: '/docs/café.md', fileName: 'café.md', excerpt: 'Saved evidence', score: 0.7 };

it.each([null, undefined, '', '{broken', 'null', '42', '[]', '{}', '{"sources":{}}'])('treats malformed or absent metadata %s as having no references', metadata => {
  expect(parseMessageSources(metadata)).toEqual([]);
});

it.each(['sources', 'sourceReferences', 'source_references'])('reads %s without losing exact path or document identity', key => {
  expect(parseMessageSources(JSON.stringify({ [key]: [source] }))).toEqual([{ ...source, category: 'Document', mimeType: '' }]);
});

it('normalizes legacy field names, finite scores and bounded excerpts', () => {
  const [parsed] = parseMessageSources(JSON.stringify({ sources: [{ document_id: ' doc-a ', file_path: ' https://example.org/page ', file_name: ' Page ', mime_type: 'text/html', content: 'x'.repeat(700), score: '0.9' }] }));
  expect(parsed).toMatchObject({ documentId: 'doc-a', filePath: 'https://example.org/page', fileName: 'Page', mimeType: 'text/html', category: 'Web Source', score: 0 });
  expect(parsed.excerpt).toHaveLength(600);
});

it('preserves sparse citation IDs independently of source order', () => {
  const parsed = parseMessageSources(JSON.stringify({ sources: [
    { ...source, citationId: 7 },
    { ...source, citation_id: 2 },
    { ...source, citationId: -1 },
  ] }));
  expect(parsed.map(item => item.citationId)).toEqual([7, 2, undefined]);
});

it('ignores non-object entries and arrays inside a sources list', () => {
  expect(parseMessageSources(JSON.stringify({ sources: [null, 0, true, 'source', [], source] }))).toHaveLength(1);
});

it('derives a readable name and safely defaults malformed optional fields', () => {
  const [named, unnamed] = parseMessageSources(JSON.stringify({ sources: [{ uri: '/資料/notes.txt' }, { fileName: ' ', filePath: 123, excerpt: false, score: null }] }));
  expect(named).toMatchObject({ fileName: 'notes.txt', filePath: '/資料/notes.txt' });
  expect(unnamed).toMatchObject({ documentId: null, fileName: 'Untitled source', filePath: 'unknown://source', excerpt: '', score: 0 });
});

it('merges repeated references without merging different document versions', () => {
  const collected = collectEntrySources([
    message([{ ...source, excerpt: '', category: '', score: 0.2 }]),
    message([{ ...source, mimeType: 'text/markdown', score: 0.9 }]),
    message([{ ...source, documentId: 'doc-b', score: 0.5 }]),
  ]);
  expect(collected).toHaveLength(2);
  expect(collected[0]).toMatchObject({ documentId: 'doc-a', score: 0.9, excerpt: 'Saved evidence', category: 'Document', mimeType: 'text/markdown' });
  expect(collected[1].documentId).toBe('doc-b');
});

it('uses filename order to break equal scores deterministically', () => {
  const collected = collectEntrySources([message([{ ...source, fileName: 'z.md' }, { ...source, fileName: 'a.md' }])]);
  expect(collected.map(item => item.fileName)).toEqual(['a.md', 'z.md']);
});

it('opens web references through safe URLs and keeps local sources local', () => {
  const [web, local, unsafe] = parseMessageSources(JSON.stringify({ sources: [
    { ...source, filePath: 'https://example.org/evidence' }, source,
    { ...source, filePath: 'javascript:alert(1)' },
  ] }));
  expect(getSourceOpenUrl(web)).toBe('https://example.org/evidence');
  expect(getSourceOpenUrl(local)).toBeNull();
  expect(getSourceOpenUrl(unsafe)).toBeNull();
});

function options() {
  return {
    enabled: true, entries: [{ id: 'a', title: 'First', updatedAt: '2026-10-08', messageCount: 2 }, { id: 'b', title: 'Second', updatedAt: '2026-10-07', messageCount: 1 }],
    pinnedIds: new Set<string>(), messagesByConversation: {} as Record<string, SnapshotMessage[]>,
    loadingByConversation: {} as Record<string, boolean>, loadMessages: vi.fn(async () => [] as SnapshotMessage[]),
  };
}

it('does no work while disabled and only loads uncached, non-pending conversations', () => {
  const props = options();
  const { result, rerender } = renderHook(useJournalSources, { initialProps: { ...props, enabled: false } });
  expect(props.loadMessages).not.toHaveBeenCalled();
  expect(result.current).toEqual({ summaries: [], isLoading: false, scannedConversationCount: 0 });
  rerender({ ...props, enabled: true, messagesByConversation: { a: [] } });
  expect(props.loadMessages).toHaveBeenCalledExactlyOnceWith('b');
  expect(result.current.isLoading).toBe(true);
  rerender({ ...props, enabled: true, messagesByConversation: { a: [] }, loadingByConversation: { b: true } });
  expect(props.loadMessages).toHaveBeenCalledOnce();
  rerender({ ...props, enabled: true, messagesByConversation: { a: [], b: [] } });
  expect(result.current.isLoading).toBe(false);
});

it('counts assistant citations but records each conversation once', () => {
  const props = options();
  props.messagesByConversation = {
    a: [message([source], 'user'), message([{ ...source, excerpt: '', score: 0.1 }]), message([source])],
    b: [message([{ ...source, score: 0.9, mimeType: 'text/markdown' }])],
  };
  const { result } = renderHook(useJournalSources, { initialProps: props });
  expect(result.current.summaries).toHaveLength(1);
  expect(result.current.summaries[0]).toMatchObject({ referenceCount: 3, conversationIds: ['a', 'b'], conversationTitles: ['First', 'Second'], score: 0.9, excerpt: 'Saved evidence', mimeType: 'text/markdown' });
  expect(result.current.isLoading).toBe(false);
});

it('caps scans at 24 while including an older pinned entry', () => {
  const props = options();
  props.entries = Array.from({ length: 30 }, (_, i) => ({ id: `c-${i}`, title: `Entry ${i}`, updatedAt: new Date(Date.UTC(2026, 9, 30 - i)).toISOString(), messageCount: 1 }));
  props.pinnedIds.add('c-29');
  const { result } = renderHook(useJournalSources, { initialProps: props });
  expect(result.current.scannedConversationCount).toBe(24);
  expect(props.loadMessages).toHaveBeenCalledTimes(24);
  expect(props.loadMessages).toHaveBeenNthCalledWith(1, 'c-29');
  expect(props.loadMessages).not.toHaveBeenCalledWith('c-28');
});
