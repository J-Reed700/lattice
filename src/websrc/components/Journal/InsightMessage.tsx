import { useMemo } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import type { SnapshotMessage } from '@/types/api/dailyNotes';
import { normalizeAssistantMarkdown } from '@/utils/assistantMarkdown';

import { parseMessageSources, type JournalMessageSource } from './useJournalSources';

interface InsightMessageProps {
  message: SnapshotMessage;
  onOpenSource: (source: JournalMessageSource) => void;
}

/**
 * Presentational read-only assistant-message view for the journal "insights"
 * list. Matches Chat's Message look (serif prose, citation footer), but
 * without the live-conversation action row (bookmark/delete/copy) since
 * the journal rendering is read-only.
 *
 * Defended in implementation report: Chat's Message.tsx couples to
 * useConversationsStore for bookmark/delete on the currently *active*
 * conversation. In Journal mode the rendered entry is rarely the active
 * conversation, so those actions would silently no-op. Rather than refactor
 * Message to accept a store adapter prop (a churn-y change across Chat),
 * we built a lean read-only variant here.
 */
export function InsightMessage({ message, onOpenSource }: InsightMessageProps) {
  const normalized = useMemo(
    () => normalizeAssistantMarkdown(message.content),
    [message.content],
  );

  const sources = useMemo(() => parseMessageSources(message.metadata), [message.metadata]);

  const timestamp = useMemo(() => {
    const d = new Date(message.createdAt);
    if (Number.isNaN(d.getTime())) return '';
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }, [message.createdAt]);

  return (
    <article className="group border-t border-[hsl(var(--border-subtle))] py-6">
      <header className="mb-2 flex items-baseline justify-between gap-3">
        <span className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-tertiary))] font-medium">
          Assistant
        </span>
        {timestamp && (
          <time className="shrink-0 text-xs text-[hsl(var(--text-muted))]">{timestamp}</time>
        )}
      </header>
      <div className="max-w-none break-words text-base font-serif leading-[1.65] [overflow-wrap:anywhere]">
        <TiptapViewer content={normalized} />
      </div>
      {sources.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-1">
          {sources.map((source, index) => (
            <button
              key={`${source.documentId ?? ''}|${source.filePath}|${index}`}
              type="button"
              onClick={() => onOpenSource(source)}
              className="inline-flex items-center gap-1 rounded-sm border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] px-2 py-0.5 text-xxs text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
              title={source.filePath}
            >
              <span className="font-mono text-[hsl(var(--text-muted))]">{index + 1}</span>
              <span className="max-w-[200px] truncate">{source.fileName}</span>
            </button>
          ))}
        </div>
      )}
    </article>
  );
}
