import { useMemo, useState } from 'react';

import { open as openExternal } from '@tauri-apps/plugin-shell';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import {
  ArrowUpRight,
  ChevronDown,
  ChevronRight,
  FileText,
  MessageSquare,
  MoreHorizontal,
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { SnapshotMessage } from '@/types/api/dailyNotes';

import { InsightMessage } from './InsightMessage';
import {
  collectEntrySources,
  getSourceOpenUrl,
  parseMessageSources,
  type JournalMessageSource,
  type JournalSourceSummary,
} from './useJournalSources';

import type { JournalEntrySummary } from './useJournalEntries';

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

interface EntryFromConversationProps {
  entry: JournalEntrySummary | null;
  messages: SnapshotMessage[];
  isLoading: boolean;
  crossEntrySources: JournalSourceSummary[];
  crossEntryLoading: boolean;
  scannedConversationCount: number;
  onJumpToEntry: (entryId: string) => void;
  onNotify: (tone: 'info' | 'success' | 'error', message: string) => void;
}

/**
 * Trailing "From this conversation" region: insights (assistant messages),
 * sources for this entry, and the cross-entry sources expansion.
 * Spec §6.
 */
export function EntryFromConversation({
  entry,
  messages,
  isLoading,
  crossEntrySources,
  crossEntryLoading,
  scannedConversationCount,
  onJumpToEntry,
  onNotify,
}: EntryFromConversationProps) {
  const prefersReducedMotion = useReducedMotion();
  const navigate = useNavigate();
  const [isExpanded, setIsExpanded] = useState(false);
  const [isInsightsExpanded, setIsInsightsExpanded] = useState(false);
  const [isCrossEntryExpanded, setIsCrossEntryExpanded] = useState(false);
  const [isMoreOpen, setIsMoreOpen] = useState(false);

  const createConversation = useConversationsStore((s) => s.createConversation);
  const addConversationWebSource = useConversationsStore((s) => s.addConversationWebSource);

  const assistantMessages = useMemo(
    () => messages.filter((m) => m.role === 'assistant'),
    [messages],
  );
  const entrySources = useMemo(() => collectEntrySources(messages), [messages]);

  const openSource = async (source: JournalMessageSource | JournalSourceSummary) => {
    const sourceUrl = getSourceOpenUrl(source);
    if (sourceUrl) {
      try {
        await openExternal(sourceUrl);
      } catch {
        const opened = window.open(sourceUrl, '_blank', 'noopener,noreferrer');
        if (!opened) onNotify('error', 'Unable to open source URL in browser.');
      }
      return;
    }
    const rawPath = source.filePath.trim();
    const pathFromFileUri = rawPath.startsWith('file://')
      ? decodeURIComponent(rawPath.replace(/^file:\/\//, ''))
      : rawPath;

    if (source.documentId && UUID_PATTERN.test(source.documentId)) {
      const byId = await VaultAPI.openFileById(source.documentId);
      if (byId.ok) {
        if (byId.data.action === 'render_internal' && byId.data.contentPath) {
          const internal = await VaultAPI.openFile(byId.data.contentPath);
          if (!internal.ok) onNotify('error', `Unable to open source: ${internal.error}`);
        }
        return;
      }
      const metadataResult = await VaultAPI.getDocument(source.documentId);
      if (metadataResult.ok && metadataResult.data.filePath) {
        const byMetadata = await VaultAPI.openFile(metadataResult.data.filePath);
        if (byMetadata.ok) return;
      }
    }
    if (!pathFromFileUri || pathFromFileUri === 'unknown://source') {
      onNotify('error', 'Source path unavailable for this citation.');
      return;
    }
    const byPath = await VaultAPI.openFile(pathFromFileUri);
    if (!byPath.ok) onNotify('error', `Unable to open source: ${byPath.error}`);
  };

  const startNewChatFromEntrySources = async () => {
    if (!entry) return;
    const sources = entrySources.slice(0, 24);
    if (sources.length === 0) {
      onNotify('error', 'No citation sources found for this journal entry yet.');
      return;
    }
    try {
      const titleBase = entry.title.trim() || 'Journal Entry';
      const conversationTitle = `Sources · ${titleBase}`.slice(0, 90);
      const newConversationId = await createConversation(conversationTitle);

      const webSources = sources
        .map((source) => ({ source, url: getSourceOpenUrl(source) }))
        .filter(
          (item): item is { source: JournalMessageSource; url: string } => Boolean(item.url),
        );
      const linkedResults = await Promise.all(
        webSources.map(({ source, url }) =>
          addConversationWebSource(newConversationId, url, {
            title: source.fileName,
            excerpt: source.excerpt || undefined,
            relevanceScore: source.score,
          }),
        ),
      );
      if (linkedResults.filter(Boolean).length === 0) {
        onNotify(
          'info',
          'Opened new chat. No web links were attached from this journal entry.',
        );
      }
      navigate(`/chat?conversationId=${encodeURIComponent(newConversationId)}`);
    } catch (error) {
      onNotify(
        'error',
        error instanceof Error ? error.message : 'Failed to start new chat from sources.',
      );
    }
  };

  if (!entry) return null;

  const insightCount = assistantMessages.length;
  const sourceCount = entrySources.length;
  const hasAssistantYet = insightCount > 0;

  return (
    <section className="mt-12 border-t border-[hsl(var(--border-subtle))] pt-6">
      <button
        type="button"
        onClick={() => setIsExpanded((v) => !v)}
        disabled={isLoading && !hasAssistantYet}
        className="group flex w-full items-center justify-between text-sm text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast disabled:hover:text-[hsl(var(--text-tertiary))]"
        aria-expanded={isExpanded}
      >
        <span>
          {isLoading && !hasAssistantYet ? (
            <span className="text-[hsl(var(--text-muted))]">
              From this conversation · loading…
            </span>
          ) : !hasAssistantYet ? (
            <span className="text-[hsl(var(--text-muted))]">
              From this conversation · waiting for assistant
            </span>
          ) : (
            <>
              From this conversation <span aria-hidden="true">·</span> {insightCount} insight
              {insightCount === 1 ? '' : 's'} <span aria-hidden="true">·</span> {sourceCount} cited
              source{sourceCount === 1 ? '' : 's'}
            </>
          )}
        </span>
        {hasAssistantYet &&
          (isExpanded ? (
            <ChevronDown className="h-4 w-4" strokeWidth={1.75} />
          ) : (
            <ChevronRight className="h-4 w-4" strokeWidth={1.75} />
          ))}
      </button>

      <AnimatePresence initial={false}>
        {isExpanded && hasAssistantYet && (
          <motion.div
            key="from-conversation"
            initial={prefersReducedMotion ? undefined : { height: 0, opacity: 0 }}
            animate={prefersReducedMotion ? undefined : { height: 'auto', opacity: 1 }}
            exit={prefersReducedMotion ? undefined : { height: 0, opacity: 0 }}
            transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <div className="mt-4 flex items-center justify-between">
              <button
                type="button"
                onClick={() => setIsInsightsExpanded((v) => !v)}
                className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
              >
                {isInsightsExpanded ? (
                  <ChevronDown className="h-3.5 w-3.5" strokeWidth={1.75} />
                ) : (
                  <ChevronRight className="h-3.5 w-3.5" strokeWidth={1.75} />
                )}
                Insights ({insightCount})
              </button>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() =>
                    navigate(`/chat?conversationId=${encodeURIComponent(entry.id)}`)
                  }
                  className="inline-flex items-center gap-1 rounded-sm px-2 py-1 text-xs text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                >
                  <ArrowUpRight className="h-3.5 w-3.5" strokeWidth={1.75} />
                  Open in Chat
                </button>
                <Popover open={isMoreOpen} onOpenChange={setIsMoreOpen}>
                  <PopoverTrigger asChild>
                    <button
                      type="button"
                      className="rounded-sm p-1 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                      aria-label="More actions"
                    >
                      <MoreHorizontal className="h-4 w-4" strokeWidth={1.75} />
                    </button>
                  </PopoverTrigger>
                  <PopoverContent align="end" className="w-60 p-2">
                    <button
                      type="button"
                      onClick={() => {
                        setIsMoreOpen(false);
                        void startNewChatFromEntrySources();
                      }}
                      className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--surface))] transition-colors duration-fast"
                    >
                      <MessageSquare className="h-3.5 w-3.5" strokeWidth={1.75} />
                      New chat from sources
                    </button>
                  </PopoverContent>
                </Popover>
              </div>
            </div>

            <AnimatePresence initial={false}>
              {isInsightsExpanded && (
                <motion.div
                  key="insights-body"
                  initial={prefersReducedMotion ? undefined : { height: 0, opacity: 0 }}
                  animate={prefersReducedMotion ? undefined : { height: 'auto', opacity: 1 }}
                  exit={prefersReducedMotion ? undefined : { height: 0, opacity: 0 }}
                  transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
                  className="overflow-hidden"
                >
                  <div className="mt-4 space-y-0">
                    {assistantMessages.map((message) => (
                      <InsightMessage
                        key={message.id}
                        message={message}
                        onOpenSource={(src) => void openSource(src)}
                      />
                    ))}
                  </div>
                </motion.div>
              )}
            </AnimatePresence>

            {/* Sources for this entry */}
            {sourceCount > 0 && (
              <div className="mt-6">
                <p className="text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                  Sources in this entry
                </p>
                <ul className="mt-3 space-y-2">
                  {entrySources.map((source) => (
                    <li key={`${source.documentId ?? ''}|${source.filePath}`}>
                      <button
                        type="button"
                        onClick={() => void openSource(source)}
                        className="group flex w-full items-start gap-2 rounded-sm px-2 py-1.5 text-left text-sm transition-colors duration-fast hover:bg-[hsl(var(--surface))]"
                      >
                        <FileText
                          className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[hsl(var(--text-tertiary))] group-hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                          strokeWidth={1.75}
                        />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-[hsl(var(--text-primary))]">
                            {source.fileName}
                          </span>
                          <span className="block truncate text-xs text-[hsl(var(--text-muted))]">
                            {source.filePath}
                          </span>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Cross-entry sources expansion */}
            <div className="mt-6">
              <button
                type="button"
                onClick={() => setIsCrossEntryExpanded((v) => !v)}
                className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
              >
                {isCrossEntryExpanded ? (
                  <ChevronDown className="h-3.5 w-3.5" strokeWidth={1.75} />
                ) : (
                  <ChevronRight className="h-3.5 w-3.5" strokeWidth={1.75} />
                )}
                View sources across all entries
              </button>
              <AnimatePresence initial={false}>
                {isCrossEntryExpanded && (
                  <motion.div
                    key="cross-entry-sources"
                    initial={prefersReducedMotion ? undefined : { height: 0, opacity: 0 }}
                    animate={prefersReducedMotion ? undefined : { height: 'auto', opacity: 1 }}
                    exit={prefersReducedMotion ? undefined : { height: 0, opacity: 0 }}
                    transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
                    className="overflow-hidden"
                  >
                    <p className="mt-3 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                      Scanning {scannedConversationCount} recent entr
                      {scannedConversationCount === 1 ? 'y' : 'ies'}
                    </p>
                    {crossEntryLoading ? (
                      <p className="mt-3 text-xs text-[hsl(var(--text-muted))]">
                        Loading sources…
                      </p>
                    ) : crossEntrySources.length === 0 ? (
                      <p className="mt-3 text-xs text-[hsl(var(--text-muted))]">
                        No sources found across recent entries.
                      </p>
                    ) : (
                      <ul className="mt-3 space-y-2">
                        {crossEntrySources.map((source) => (
                          <li key={source.key}>
                            <button
                              type="button"
                              onClick={() => void openSource(source)}
                              className="group flex w-full items-start gap-2 rounded-sm px-2 py-1.5 text-left text-sm transition-colors duration-fast hover:bg-[hsl(var(--surface))]"
                            >
                              <FileText
                                className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[hsl(var(--text-tertiary))] group-hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                                strokeWidth={1.75}
                              />
                              <span className="min-w-0 flex-1">
                                <span className="block truncate text-[hsl(var(--text-primary))]">
                                  {source.fileName}
                                </span>
                                <span className="block truncate text-xs text-[hsl(var(--text-muted))]">
                                  Referenced in {source.conversationIds.length} entr
                                  {source.conversationIds.length === 1 ? 'y' : 'ies'}
                                </span>
                              </span>
                              {source.conversationIds.length > 0 && (
                                <button
                                  type="button"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    onJumpToEntry(source.conversationIds[0]);
                                  }}
                                  className="shrink-0 rounded-sm px-2 py-0.5 text-xs text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                                >
                                  Open entry
                                </button>
                              )}
                            </button>
                          </li>
                        ))}
                      </ul>
                    )}
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </section>
  );
}

// Re-export the helper so index.ts can use the types
export { parseMessageSources };
