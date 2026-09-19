import { useState } from 'react';

import { Bookmark, Combine, FileText, MessageSquare, NotebookPen, Plus, Search } from 'lucide-react';
import { useNavigate } from 'react-router';

import { ContentViewer } from '@/components/ContentViewer';
import { weekPageTitle } from '@/components/Journal/synthesisTargets';
import { describeWeekCandidates } from '@/components/Journal/SynthesizePopover';
import {
  useCorpusShapeQuery,
  useDashboardQuery,
  useWeeklySynthesisCandidatesQuery,
} from '@/hooks/queries';
import { OPEN_PALETTE_EVENT } from '@/hooks/useCommandPalette';
import VaultAPI from '@/lib/api';
import { cn } from '@/lib/utils';
import type { RecentDocument } from '@/types';
import type { ConversationMessageBookmarkDto } from '@/types/api/conversation';
import { formatBytes, formatRelativeTime } from '@/utils/formatters';

import { DashboardError } from './DashboardError';
import { DashboardSkeleton } from './DashboardSkeleton';

/**
 * Home — a quiet readout of the library and the threads worth resuming.
 * Every number here is real; if the backend can't report one, the tile is
 * left out rather than shown as zero.
 */

function formatTodayLabel(): string {
  return new Date().toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' });
}

function greeting(): string {
  const hour = new Date().getHours();
  if (hour < 5) return 'Still up.';
  if (hour < 12) return 'Good morning.';
  if (hour < 18) return 'Good afternoon.';
  return 'Good evening.';
}

function truncate(value: string, max: number): string {
  const trimmed = value.trim();
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max).trimEnd()}…`;
}

const numberFormat = new Intl.NumberFormat();

/** Per-viewer UI state only — the backend never reads it. */
const WEEKLY_SYNTHESIS_DISMISSED_KEY = 'home.weeklySynthesis.dismissedWeek';

function readDismissedWeek(): string | null {
  try {
    return localStorage.getItem(WEEKLY_SYNTHESIS_DISMISSED_KEY);
  } catch {
    return null;
  }
}

function writeDismissedWeek(value: string): void {
  try {
    localStorage.setItem(WEEKLY_SYNTHESIS_DISMISSED_KEY, value);
  } catch {
    // Ignore
  }
}

const rowClass =
  'row-hover group flex w-full items-start gap-3 rounded-lg px-2.5 py-2.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

/** Warm, low-chroma steps for the file-type bar: proportion, not category colour. */
const TYPE_BAR_OPACITY = [0.9, 0.62, 0.42, 0.28, 0.18];

function SectionTitle({ children, action }: { children: string; action?: { label: string; run: () => void } }) {
  return (
    <div className="mb-1 flex items-baseline justify-between px-2.5">
      <h2 className="text-ui font-medium text-text-tertiary">{children}</h2>
      {action ? (
        <button
          type="button"
          onClick={action.run}
          className="text-xs text-text-muted transition-colors duration-fast hover:text-text-primary"
        >
          {action.label}
        </button>
      ) : null}
    </div>
  );
}

function RowGlyph({ icon: Icon, accent = false }: { icon: typeof FileText; accent?: boolean }) {
  return (
    <span
      className={cn(
        'mt-px flex h-7 w-7 shrink-0 items-center justify-center rounded-md',
        accent ? 'bg-accent-muted text-accent' : 'bg-[hsl(var(--text-primary)/0.055)] text-text-tertiary',
      )}
    >
      <Icon className="h-[15px] w-[15px]" strokeWidth={1.6} />
    </span>
  );
}

function EmptyRow({ children }: { children: string }) {
  return <p className="px-2.5 py-3 text-ui leading-relaxed text-text-muted">{children}</p>;
}

export const Dashboard = () => {
  const { data, isLoading, error } = useDashboardQuery();
  const { data: weekCandidates } = useWeeklySynthesisCandidatesQuery();
  const { data: corpusShape } = useCorpusShapeQuery();
  const navigate = useNavigate();
  const [viewerFilePath, setViewerFilePath] = useState<string | null>(null);

  if (isLoading) return <DashboardSkeleton />;
  if (error) return <DashboardError error={error.message} />;

  const stats = data?.stats;
  const tiles: Array<{ label: string; value: string }> = [];
  if (stats) {
    tiles.push({ label: 'Documents', value: numberFormat.format(stats.documentCount) });
    if (stats.folderCount !== null) {
      tiles.push({ label: stats.folderCount === 1 ? 'Folder' : 'Folders', value: numberFormat.format(stats.folderCount) });
    }
    if (stats.storageBytes !== null && stats.storageBytes > 0) {
      tiles.push({ label: 'On disk', value: formatBytes(stats.storageBytes) });
    }
    tiles.push({
      label: 'Last indexed',
      value: stats.lastIndexedAt ? formatRelativeTime(stats.lastIndexedAt, 'Never') : 'Never',
    });
  }
  const fileTypes = stats?.fileTypes ?? [];

  const recentConversations = data?.recentConversations ?? [];
  const todayEntry = data?.todayJournalEntry ?? null;
  const preferredJournalSpaceId = data?.preferredJournalSpaceId ?? null;
  const recentReferences: ConversationMessageBookmarkDto[] = data?.recentReferences ?? [];
  const journalSpaceIdSet = new Set(data?.journalSpaceIds ?? []);
  const recentDocuments = (data?.recentDocuments ?? []).slice(0, 5);

  const openDocument = async (doc: RecentDocument) => {
    const id = doc.documentId || doc.id;
    if (!id) return;
    const opened = await VaultAPI.openFileById(id);
    if (opened.ok && opened.data.action === 'render_internal') setViewerFilePath(opened.data.contentPath);
  };

  const goToConversation = (conversationId: string) => {
    navigate(`/chat?${new URLSearchParams({ conversationId }).toString()}`);
  };

  const goToJournal = () => {
    if (todayEntry) {
      const params = new URLSearchParams({ journalSpaceId: todayEntry.journalSpaceId, entryId: todayEntry.entryId });
      navigate(`/journals?${params.toString()}`);
      return;
    }
    if (preferredJournalSpaceId) {
      navigate(`/journals?${new URLSearchParams({ journalSpaceId: preferredJournalSpaceId }).toString()}`);
      return;
    }
    navigate('/journals');
  };

  const goToReference = (bookmark: ConversationMessageBookmarkDto) => {
    navigate(`/references?${new URLSearchParams({ referenceId: bookmark.id }).toString()}`);
  };

  // Offered once a week, and only when there is genuinely something there.
  const currentWeek = weekPageTitle();
  const showWeeklySynthesis =
    (weekCandidates?.total ?? 0) > 0 && readDismissedWeek() !== currentWeek;
  const weeklyParts = weekCandidates ? describeWeekCandidates(weekCandidates) : '';

  const goToWeeklySynthesis = () => {
    writeDismissedWeek(currentWeek);
    navigate('/journals?synthesize=week');
  };

  const typeTotal = fileTypes.reduce((sum, entry) => sum + entry.count, 0);

  const actions = [
    { label: 'Write', detail: todayEntry ? 'Continue today\u2019s entry' : 'Start today\u2019s entry', icon: NotebookPen, shortcut: '⌘3', run: goToJournal },
    { label: 'Ask', detail: 'A new conversation', icon: MessageSquare, shortcut: '⌘N', run: () => navigate('/chat?new=1') },
    { label: 'Add', detail: 'Files, folders, links', icon: Plus, shortcut: '⌘I', run: () => navigate('/ingest') },
  ];

  return (
    <main className="relative h-full overflow-y-auto bg-bg">
      <div className="mx-auto w-full max-w-[1000px] px-8 pb-16 pt-10 lg:px-12">
        <header className="mb-7">
          <p className="mb-2 text-xs text-text-muted">{formatTodayLabel()}</p>
          <h1 className="font-serif text-[34px] font-normal leading-[1.1] tracking-[-0.03em] text-text-primary">{greeting()}</h1>
        </header>

        {/* The palette is the real search; this is its front door. */}
        <button
          type="button"
          onClick={() => window.dispatchEvent(new Event(OPEN_PALETTE_EVENT))}
          className="group mb-3 flex h-12 w-full items-center gap-3 rounded-xl bg-surface px-4 text-left shadow-sheet transition-shadow duration-base hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Search className="h-[17px] w-[17px] text-text-tertiary transition-colors duration-fast group-hover:text-accent" strokeWidth={1.6} />
          <span className="flex-1 text-[15px] text-text-muted">Find a document, an idea, a connection…</span>
          <kbd className="kbd">⌘K</kbd>
        </button>

        <div className="mb-10 grid grid-cols-3 gap-3">
          {actions.map(({ label, detail, icon: Icon, shortcut, run }) => (
            <button
              key={label}
              type="button"
              onClick={run}
              className="pressable group flex items-center gap-3 rounded-xl border border-border-subtle px-3.5 py-3 text-left transition-[background-color,border-color,scale] duration-fast hover:border-border-default hover:bg-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent-muted text-accent">
                <Icon className="h-4 w-4" strokeWidth={1.6} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-ui font-medium text-text-primary">{label}</span>
                <span className="block truncate text-xs text-text-muted">{detail}</span>
              </span>
              <kbd className="kbd opacity-0 transition-opacity duration-fast group-hover:opacity-100">{shortcut}</kbd>
            </button>
          ))}
        </div>

        <div className="grid gap-x-10 gap-y-9 lg:grid-cols-[minmax(0,1.5fr)_minmax(0,1fr)]">
          <div className="space-y-9">
            {/* Today */}
            <section>
              <SectionTitle>Today</SectionTitle>
              {todayEntry ? (
                <button
                  type="button"
                  onClick={goToJournal}
                  className="pressable group w-full rounded-xl bg-surface p-5 text-left shadow-sheet transition-[box-shadow,scale] duration-base hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <div className="flex items-baseline justify-between gap-3">
                    <div className="truncate font-serif text-[19px] font-medium tracking-[-0.015em] text-text-primary">
                      {todayEntry.title || 'Untitled entry'}
                    </div>
                    <div className="shrink-0 text-xs tabular-nums text-text-muted">
                      {todayEntry.messageCount} {todayEntry.messageCount === 1 ? 'message' : 'messages'}
                    </div>
                  </div>
                  {todayEntry.lastMessagePreview?.trim() ? (
                    <p className="mt-2 line-clamp-3 font-serif text-[15px] leading-relaxed text-text-secondary">
                      {truncate(todayEntry.lastMessagePreview, 240)}
                    </p>
                  ) : null}
                  <div className="mt-3 text-xs font-medium text-accent">Continue writing →</div>
                </button>
              ) : (
                <button type="button" onClick={goToJournal} className={cn(rowClass, 'items-center')}>
                  <RowGlyph icon={NotebookPen} accent />
                  <div className="text-ui font-medium text-text-primary">Start today&rsquo;s entry</div>
                </button>
              )}
            </section>

            {/* Continue */}
            <section>
              <SectionTitle action={{ label: 'All conversations', run: () => navigate('/chat') }}>Continue</SectionTitle>
              {recentConversations.length === 0 ? (
                <EmptyRow>Your conversations will appear here. Start one above to explore an idea.</EmptyRow>
              ) : (
                recentConversations.map((conv) => {
                  const preview = conv.lastMessagePreview?.trim() ?? '';
                  return (
                    <button key={conv.id} type="button" onClick={() => goToConversation(conv.id)} className={rowClass}>
                      <RowGlyph icon={MessageSquare} />
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-ui font-medium text-text-primary">
                          {conv.title || 'Untitled conversation'}
                        </div>
                        {preview ? <div className="mt-0.5 truncate text-xs text-text-muted">{preview}</div> : null}
                      </div>
                      <div className="shrink-0 pt-0.5 text-xs tabular-nums text-text-muted">{formatRelativeTime(conv.updatedAt, '')}</div>
                    </button>
                  );
                })
              )}
            </section>

            {/* This week */}
            {showWeeklySynthesis ? (
              <section>
                <SectionTitle>This week</SectionTitle>
                <button type="button" onClick={goToWeeklySynthesis} className={rowClass}>
                  <RowGlyph icon={Combine} accent />
                  <div className="min-w-0 flex-1">
                    <div className="text-ui font-medium text-text-primary">Synthesize last week</div>
                    <div className="mt-0.5 truncate text-xs tabular-nums text-text-muted">{weeklyParts}</div>
                  </div>
                </button>
              </section>
            ) : null}

            {/* Recent references */}
            <section>
              <SectionTitle action={{ label: 'All references', run: () => navigate('/references') }}>Saved</SectionTitle>
              {recentReferences.length === 0 ? (
                <EmptyRow>Nothing saved yet. Bookmark a message to keep it here.</EmptyRow>
              ) : (
                recentReferences.map((bookmark) => {
                  const isJournal = bookmark.spaceId ? journalSpaceIdSet.has(bookmark.spaceId) : false;
                  const excerpt =
                    bookmark.title?.trim() || bookmark.note?.trim() || bookmark.messagePreview?.trim() || '';
                  return (
                    <button key={bookmark.id} type="button" onClick={() => goToReference(bookmark)} className={rowClass}>
                      <RowGlyph icon={Bookmark} />
                      <div className="min-w-0 flex-1">
                        {excerpt ? <div className="truncate text-ui font-medium text-text-primary">{excerpt}</div> : null}
                        <div className="mt-0.5 truncate text-xs text-text-muted">
                          {isJournal ? 'Journal' : 'Chat'}
                          <span> · </span>
                          {bookmark.conversationTitle || 'Untitled'}
                        </div>
                      </div>
                      <div className="shrink-0 pt-0.5 text-xs tabular-nums text-text-muted">{formatRelativeTime(bookmark.createdAt, '')}</div>
                    </button>
                  );
                })
              )}
            </section>
          </div>

          <div className="space-y-9">
            {/* Library readout */}
            <section>
              <SectionTitle action={{ label: 'Open library', run: () => navigate('/files') }}>Library</SectionTitle>
              <div className="rounded-xl border border-border-subtle p-4">
                <div className="grid grid-cols-2 gap-x-4 gap-y-4">
                  {tiles.map((tile) => (
                    <div key={tile.label}>
                      <div className="text-[22px] font-medium leading-none tracking-[-0.02em] tabular-nums text-text-primary">{tile.value}</div>
                      <div className="mt-1.5 text-xs text-text-muted">{tile.label}</div>
                    </div>
                  ))}
                </div>
                {typeTotal > 0 ? (
                  <div className="mt-5">
                    <div className="flex h-1.5 gap-px overflow-hidden rounded-full" aria-hidden="true">
                      {fileTypes.map((entry, index) => (
                        <span
                          key={entry.label}
                          className="bg-accent"
                          style={{
                            flexGrow: entry.count,
                            opacity: TYPE_BAR_OPACITY[Math.min(index, TYPE_BAR_OPACITY.length - 1)],
                          }}
                        />
                      ))}
                    </div>
                    <p className="mt-2.5 text-xs leading-relaxed tabular-nums text-text-tertiary">
                      {fileTypes.map((entry, index) => (
                        <span key={entry.label}>
                          {index > 0 ? <span className="text-text-muted"> · </span> : null}
                          {numberFormat.format(entry.count)} {entry.label}
                        </span>
                      ))}
                    </p>
                  </div>
                ) : null}
                {corpusShape && corpusShape.grownLast7Days > 0 ? (
                  <p className="mt-1 text-xs tabular-nums text-text-muted">
                    {corpusShape.grownLast7Days.toLocaleString()} added in the last 7 days
                  </p>
                ) : null}
              </div>
            </section>

            {/* Recently indexed */}
            <section>
              <SectionTitle>Recently indexed</SectionTitle>
              {recentDocuments.length === 0 ? (
                <EmptyRow>Add your first files to make your library searchable.</EmptyRow>
              ) : (
                recentDocuments.map((doc: RecentDocument, index: number) => (
                  <button
                    key={doc.id || index}
                    type="button"
                    onClick={() => void openDocument(doc)}
                    className={cn(rowClass, 'items-center py-2')}
                  >
                    <FileText className="h-4 w-4 shrink-0 text-text-muted" strokeWidth={1.6} />
                    <div className="min-w-0 flex-1 truncate text-ui text-text-primary">
                      {doc.documentName || doc.documentPath || 'Untitled'}
                    </div>
                    <div className="shrink-0 text-xs tabular-nums text-text-muted">
                      {formatRelativeTime(doc.lastAccessedAt, '')}
                    </div>
                  </button>
                ))
              )}
            </section>
          </div>
        </div>
      </div>

      <ContentViewer filePath={viewerFilePath} onClose={() => setViewerFilePath(null)} />
    </main>
  );
};
