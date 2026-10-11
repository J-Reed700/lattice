import { useMemo } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { parseMessageSources, type JournalMessageSource } from '@/features/journal/hooks/useJournalSources';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { SnapshotMessage } from '@/types/api/dailyNotes';
import { normalizeAssistantMarkdown } from '@/utils/assistantMarkdown';


interface InsightMessageProps {
  message: SnapshotMessage;
  onOpenSource: (source: JournalMessageSource) => void;
}

/**
 * Read-only assistant-message view for journal insights. Chat's full Message
 * component binds actions to the active conversation, while journal entries
 * may belong to a different conversation.
 */
export function InsightMessage({ message, onOpenSource }: InsightMessageProps) {
  const showCitations = useCitationDisplayStore((state) => state.visible);
  const normalized = useMemo(
    () => normalizeAssistantMarkdown(message.content),
    [message.content],
  );

  const sources = useMemo(() => parseMessageSources(message.metadata), [message.metadata]);
  const citationMap = useMemo(
    () => new Map(sources.map((source, index) => [source.citationId ?? index + 1, source])),
    [sources],
  );
  const citationNumbers = useMemo(() => [...citationMap.keys()], [citationMap]);

  const timestamp = useMemo(() => {
    const d = new Date(message.createdAt);
    if (Number.isNaN(d.getTime())) return '';
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }, [message.createdAt]);

  return (
    <article className="journal-insight group border-t border-[hsl(var(--border-subtle))] py-5 first:border-t-0 first:pt-2">
      <header className="mb-2 flex items-baseline justify-between gap-3">
        <span className="text-xs font-medium text-[hsl(var(--text-muted))]">
          Assistant
        </span>
        {timestamp && (
          <time className="shrink-0 text-xs text-[hsl(var(--text-muted))]">{timestamp}</time>
        )}
      </header>
      <div
        className="journal-insight-body max-w-none wrap-break-word font-serif text-[15px] leading-[1.65] wrap-anywhere"
        onClick={(event) => {
          if (!showCitations || !(event.target instanceof Element)) return;
          const chip = event.target.closest<HTMLElement>('[data-cite]');
          const source = chip ? citationMap.get(Number(chip.dataset.cite)) : undefined;
          if (!source) return;
          event.preventDefault();
          onOpenSource(source);
        }}
      >
        <TiptapViewer
          content={normalized}
          citationNumbers={citationNumbers}
          showEvidence={showCitations}
        />
      </div>
      {showCitations && sources.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-1">
          {sources.map((source, index) => (
            <button
              key={`${source.documentId ?? ''}|${source.filePath}|${index}`}
              type="button"
              onClick={() => onOpenSource(source)}
              className="row-hover inline-flex h-6 items-center gap-1.5 rounded-md bg-[hsl(var(--text-primary)/0.05)] px-2 text-xxs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]"
              title={source.filePath}
            >
              <span className="font-semibold tabular-nums text-[hsl(var(--accent))]">{source.citationId ?? index + 1}</span>
              <span className="max-w-[200px] truncate">{source.fileName}</span>
            </button>
          ))}
        </div>
      )}
    </article>
  );
}
