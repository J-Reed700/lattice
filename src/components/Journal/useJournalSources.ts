import { useEffect, useMemo } from 'react';

import type { SnapshotMessage } from '@/types/api/dailyNotes';
import { getSourceExternalUrl } from '@/utils/sourcePreview';

import type { JournalEntrySummary } from './useJournalEntries';

const JOURNAL_SOURCE_SCAN_LIMIT = 24;

export interface JournalMessageSource {
  documentId: string | null;
  fileName: string;
  filePath: string;
  category: string;
  mimeType: string;
  excerpt: string;
  score: number;
}

export interface JournalSourceSummary {
  key: string;
  documentId: string | null;
  fileName: string;
  filePath: string;
  category: string;
  mimeType: string;
  excerpt: string;
  score: number;
  referenceCount: number;
  conversationIds: string[];
  conversationTitles: string[];
}

function asString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

function asNumber(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

export function parseMessageSources(metadata: string | null | undefined): JournalMessageSource[] {
  if (!metadata) return [];
  try {
    const parsed = JSON.parse(metadata) as Record<string, unknown>;
    const rawSources = Array.isArray(parsed.sources)
      ? parsed.sources
      : Array.isArray(parsed.sourceReferences)
        ? parsed.sourceReferences
        : Array.isArray(parsed.source_references)
          ? parsed.source_references
          : [];
    if (!Array.isArray(rawSources)) return [];

    return rawSources
      .filter((value): value is Record<string, unknown> => Boolean(value) && typeof value === 'object')
      .map((source) => {
        const filePath = asString(
          source.filePath ?? source.file_path ?? source.path ?? source.url ?? source.uri,
        ).trim();
        const derivedName = filePath.split('/').pop() ?? '';
        const fileName = asString(source.fileName ?? source.file_name, derivedName || 'Untitled source').trim();
        const documentId = asString(source.documentId ?? source.document_id).trim();
        const score = asNumber(source.score, 0);
        const excerpt = asString(source.excerpt ?? source.content).trim();
        return {
          documentId: documentId || null,
          fileName: fileName || 'Untitled source',
          filePath: filePath || 'unknown://source',
          category: asString(source.category, filePath.startsWith('http') ? 'Web Source' : 'Document'),
          mimeType: asString(source.mimeType ?? source.mime_type),
          excerpt: excerpt.slice(0, 600),
          score,
        };
      });
  } catch {
    return [];
  }
}

export function collectEntrySources(messages: SnapshotMessage[]): JournalMessageSource[] {
  const byKey = new Map<string, JournalMessageSource>();
  for (const message of messages) {
    const sources = parseMessageSources(message.metadata);
    for (const source of sources) {
      const key = `${source.documentId ?? ''}|${source.filePath}|${source.fileName}`;
      const existing = byKey.get(key);
      if (!existing) {
        byKey.set(key, { ...source });
        continue;
      }
      if (source.score > existing.score) existing.score = source.score;
      if (!existing.excerpt && source.excerpt) existing.excerpt = source.excerpt;
      if (!existing.mimeType && source.mimeType) existing.mimeType = source.mimeType;
      if (!existing.category && source.category) existing.category = source.category;
    }
  }
  return [...byKey.values()].sort((a, b) => {
    if (a.score !== b.score) return b.score - a.score;
    return a.fileName.localeCompare(b.fileName);
  });
}

export function getSourceOpenUrl(source: JournalMessageSource | JournalSourceSummary): string | null {
  return getSourceExternalUrl({
    filePath: source.filePath,
    path: source.documentId?.startsWith('web:') ? source.documentId.slice(4) : undefined,
    documentId: source.documentId,
    category: source.category,
    mimeType: source.mimeType,
  });
}

export interface UseJournalSourcesResult {
  summaries: JournalSourceSummary[];
  scannedConversationCount: number;
  isLoading: boolean;
}

/**
 * Aggregates assistant-cited sources across the recent N journal entries.
 */
export function useJournalSources(options: {
  entries: JournalEntrySummary[];
  pinnedIds: Set<string>;
  messagesByConversation: Record<string, SnapshotMessage[]>;
  loadingByConversation: Record<string, boolean>;
  loadMessages: (conversationId: string) => Promise<SnapshotMessage[]>;
  enabled: boolean;
}): UseJournalSourcesResult {
  const { entries, pinnedIds, messagesByConversation, loadingByConversation, loadMessages, enabled } = options;

  const scanConversations = useMemo(() => {
    if (!enabled || entries.length === 0) return [];
    return [...entries]
      .map((entry, index) => ({ entry, index, pinned: pinnedIds.has(entry.id) }))
      .sort((a, b) => {
        if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
        const aT = new Date(a.entry.updatedAt).getTime();
        const bT = new Date(b.entry.updatedAt).getTime();
        if (!Number.isNaN(aT) && !Number.isNaN(bT) && aT !== bT) return bT - aT;
        return a.index - b.index;
      })
      .map((item) => item.entry)
      .slice(0, JOURNAL_SOURCE_SCAN_LIMIT);
  }, [enabled, entries, pinnedIds]);

  useEffect(() => {
    if (!enabled || scanConversations.length === 0) return;
    const toLoad = scanConversations.filter(
      (entry) =>
        messagesByConversation[entry.id] === undefined && !loadingByConversation[entry.id],
    );
    if (toLoad.length === 0) return;
    void Promise.all(toLoad.map((entry) => loadMessages(entry.id)));
  }, [enabled, scanConversations, messagesByConversation, loadingByConversation, loadMessages]);

  const isLoading = useMemo(() => {
    if (!enabled || scanConversations.length === 0) return false;
    return scanConversations.some(
      (entry) =>
        loadingByConversation[entry.id] || messagesByConversation[entry.id] === undefined,
    );
  }, [enabled, scanConversations, loadingByConversation, messagesByConversation]);

  const summaries = useMemo(() => {
    if (!enabled || scanConversations.length === 0) return [];
    const byKey = new Map<string, JournalSourceSummary>();
    for (const conversation of scanConversations) {
      const messages = messagesByConversation[conversation.id];
      if (!messages || messages.length === 0) continue;
      for (const message of messages) {
        if (message.role !== 'assistant') continue;
        const sources = parseMessageSources(message.metadata);
        for (const source of sources) {
          const key = `${source.documentId ?? ''}|${source.filePath}|${source.fileName}`;
          const existing = byKey.get(key);
          if (!existing) {
            byKey.set(key, {
              key,
              documentId: source.documentId,
              fileName: source.fileName,
              filePath: source.filePath,
              category: source.category,
              mimeType: source.mimeType,
              excerpt: source.excerpt,
              score: source.score,
              referenceCount: 1,
              conversationIds: [conversation.id],
              conversationTitles: [conversation.title],
            });
            continue;
          }
          existing.referenceCount += 1;
          if (source.score > existing.score) existing.score = source.score;
          if (!existing.excerpt && source.excerpt) existing.excerpt = source.excerpt;
          if (!existing.documentId && source.documentId) existing.documentId = source.documentId;
          if (!existing.mimeType && source.mimeType) existing.mimeType = source.mimeType;
          if (!existing.category && source.category) existing.category = source.category;
          if (!existing.conversationIds.includes(conversation.id)) {
            existing.conversationIds.push(conversation.id);
            existing.conversationTitles.push(conversation.title);
          }
        }
      }
    }
    return [...byKey.values()].sort((a, b) => {
      if (a.referenceCount !== b.referenceCount) return b.referenceCount - a.referenceCount;
      if (a.score !== b.score) return b.score - a.score;
      return a.fileName.localeCompare(b.fileName);
    });
  }, [enabled, scanConversations, messagesByConversation]);

  return {
    summaries,
    scannedConversationCount: scanConversations.length,
    isLoading,
  };
}
