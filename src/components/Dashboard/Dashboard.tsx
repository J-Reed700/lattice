import { ArrowRight, Bookmark, Combine, FileText, MessageSquare, NotebookPen, Plus, Search } from 'lucide-react';
import { useNavigate } from 'react-router';

import { weekPageTitle } from '@/components/Journal/synthesisTargets';
import { describeWeekCandidates } from '@/components/Journal/SynthesizePopover';
import { SectionHeading } from '@/components/ui';
import {
  useCorpusShapeQuery,
  useDashboardQuery,
  useWeeklySynthesisCandidatesQuery,
} from '@/hooks/queries';
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
  'flex w-full items-start gap-3 border-b border-border-subtle px-2 py-3 text-left transition-colors duration-fast hover:bg-surface-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-bg';

function EmptyRow({ children }: { children: string }) {
  return <p className="border-b border-border-subtle px-2 py-4 text-sm text-text-tertiary">{children}</p>;
}

export const Dashboard = () => {
  const { data, isLoading, error } = useDashboardQuery();
  const { data: weekCandidates } = useWeeklySynthesisCandidatesQuery();
  const { data: corpusShape } = useCorpusShapeQuery();
  const navigate = useNavigate();

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

  return (
    <main className="relative h-full overflow-y-auto bg-bg">
      <div className="mx-auto w-full max-w-[1080px] px-8 pt-12 pb-16 lg:px-14">
        <header className="mb-10">
          <div className="mb-5 flex items-center justify-between gap-4">
            <span className="text-[10px] font-medium uppercase tracking-[0.18em] text-accent">Your workspace</span>
            <span className="text-xs text-text-tertiary">{formatTodayLabel()}</span>
          </div>
          <h1 className="font-serif text-[clamp(32px,4vw,48px)] font-medium leading-tight tracking-[-0.035em] text-text-primary">A little space to think.</h1>
          <p className="mt-3 text-sm leading-relaxed text-text-tertiary">Pick up a thread, explore your library, or begin with a blank page.</p>
        </header>

        <button type="button" onClick={() => navigate('/search')} className="group mb-6 flex w-full items-center gap-3 rounded-xl border border-border-default bg-surface px-5 py-4 text-left shadow-sm transition-colors hover:border-accent focus-visible:outline-accent">
          <Search className="h-5 w-5 text-accent" strokeWidth={1.75} />
          <span className="flex-1 text-sm text-text-tertiary">Find a document, an idea, a connection…</span>
          <ArrowRight className="h-4 w-4 text-text-tertiary transition-transform group-hover:translate-x-1" />
        </button>

        <div className="mb-10 grid grid-cols-3 gap-3">
          {[
            { label: 'Write in your journal', detail: 'Make room for a thought', icon: NotebookPen, run: goToJournal },
            { label: 'Start a conversation', detail: 'Explore something further', icon: MessageSquare, run: () => navigate('/chat?new=1') },
            { label: 'Add to your library', detail: 'Files, folders, and links', icon: Plus, run: () => navigate('/ingest') },
          ].map(({ label, detail, icon: Icon, run }) => (
            <button key={label} type="button" onClick={run} className="group rounded-xl border border-border-subtle bg-surface p-4 text-left transition-colors hover:border-border-strong hover:bg-surface-raised">
              <Icon className="mb-4 h-5 w-5 text-accent" strokeWidth={1.5} />
              <span className="block text-sm font-medium text-text-primary">{label}</span>
              <span className="mt-1 block text-xs leading-relaxed text-text-tertiary">{detail}</span>
            </button>
          ))}
        </div>

        {/* Library readout */}
        <section className="mb-10 rounded-xl border border-border-subtle px-5 py-5">
          <div className="grid grid-cols-2 gap-x-6 gap-y-5 sm:grid-cols-4">
            {tiles.map((tile) => (
              <div key={tile.label}>
                <div className="text-xxs uppercase tracking-[0.04em] text-text-tertiary">{tile.label}</div>
                <div className="mt-1 text-xl font-medium tabular-nums text-text-primary">{tile.value}</div>
              </div>
            ))}
          </div>
          {fileTypes.length > 0 ? (
            <p className="mt-6 text-sm text-text-tertiary tabular-nums">
              {fileTypes.map((entry, index) => (
                <span key={entry.label}>
                  {index > 0 ? <span className="text-text-muted"> · </span> : null}
                  {numberFormat.format(entry.count)} {entry.label}
                </span>
              ))}
            </p>
          ) : null}
          {corpusShape && corpusShape.grownLast7Days > 0 ? (
            <p className="mt-1 text-sm tabular-nums text-text-tertiary">
              {corpusShape.grownLast7Days.toLocaleString()} added in the last 7 days
            </p>
          ) : null}
        </section>

        <div className="grid gap-x-10 lg:grid-cols-2">
        {/* Continue */}
        <section className="mb-12">
          <SectionHeading>Continue</SectionHeading>
          <div className="border-t border-border-subtle">
            {recentConversations.length === 0 ? (
              <EmptyRow>Your conversations will appear here. Start one above to explore an idea.</EmptyRow>
            ) : (
              recentConversations.map((conv) => {
                const preview = conv.lastMessagePreview?.trim() ?? '';
                return (
                  <button key={conv.id} type="button" onClick={() => goToConversation(conv.id)} className={rowClass}>
                    <MessageSquare className="mt-0.5 h-4 w-4 shrink-0 text-text-tertiary" strokeWidth={1.75} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium text-text-primary">
                        {conv.title || 'Untitled conversation'}
                      </div>
                      {preview ? <div className="mt-0.5 truncate text-xs text-text-tertiary">{preview}</div> : null}
                    </div>
                    <div className="shrink-0 text-xs tabular-nums text-text-muted">{formatRelativeTime(conv.updatedAt, '')}</div>
                  </button>
                );
              })
            )}
          </div>
        </section>

        {/* Today */}
        <section className="mb-12">
          <SectionHeading>Today</SectionHeading>
          <div className="border-t border-border-subtle">
            {todayEntry ? (
              <button type="button" onClick={goToJournal} className={`${rowClass} flex-col gap-2 py-4`}>
                <div className="flex w-full items-center justify-between gap-3">
                  <div className="truncate text-sm font-medium text-text-primary">{todayEntry.title || 'Untitled entry'}</div>
                  <div className="shrink-0 text-xs tabular-nums text-text-muted">
                    {todayEntry.messageCount} {todayEntry.messageCount === 1 ? 'message' : 'messages'}
                  </div>
                </div>
                {todayEntry.lastMessagePreview?.trim() ? (
                  <p className="line-clamp-3 text-sm text-text-tertiary">{truncate(todayEntry.lastMessagePreview, 200)}</p>
                ) : null}
                <div className="text-xs font-medium text-accent">Continue writing</div>
              </button>
            ) : (
              <button type="button" onClick={goToJournal} className={`${rowClass} items-center py-4`}>
                <NotebookPen className="h-4 w-4 shrink-0 text-text-tertiary" strokeWidth={1.75} />
                <div className="text-sm font-medium text-text-primary">Start today's entry</div>
              </button>
            )}
          </div>
        </section>

        {/* This week */}
        {showWeeklySynthesis ? (
          <section className="mb-12">
            <SectionHeading>This week</SectionHeading>
            <div className="border-t border-border-subtle">
              <button type="button" onClick={goToWeeklySynthesis} className={rowClass}>
                <Combine className="mt-0.5 h-4 w-4 shrink-0 text-text-tertiary" strokeWidth={1.75} />
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium text-text-primary">Synthesize last week</div>
                  <div className="mt-0.5 truncate text-xs text-text-tertiary tabular-nums">
                    {weeklyParts}
                  </div>
                </div>
              </button>
            </div>
          </section>
        ) : null}

        {/* Recent references */}
        <section className="mb-12">
          <SectionHeading>Recent references</SectionHeading>
          <div className="border-t border-border-subtle">
            {recentReferences.length === 0 ? (
              <EmptyRow>Nothing saved yet. Bookmark a message to keep it here.</EmptyRow>
            ) : (
              recentReferences.map((bookmark) => {
                const isJournal = bookmark.spaceId ? journalSpaceIdSet.has(bookmark.spaceId) : false;
                const excerpt =
                  bookmark.title?.trim() || bookmark.note?.trim() || bookmark.messagePreview?.trim() || '';
                return (
                  <button key={bookmark.id} type="button" onClick={() => goToReference(bookmark)} className={rowClass}>
                    <Bookmark className="mt-0.5 h-4 w-4 shrink-0 text-text-tertiary" strokeWidth={1.75} />
                    <div className="min-w-0 flex-1">
                      {excerpt ? <div className="truncate text-sm font-medium text-text-primary">{excerpt}</div> : null}
                      <div className="mt-0.5 truncate text-xs text-text-tertiary">
                        {isJournal ? 'Journal' : 'Chat'}
                        <span className="text-text-muted"> · </span>
                        {bookmark.conversationTitle || 'Untitled'}
                      </div>
                    </div>
                    <div className="shrink-0 text-xs tabular-nums text-text-muted">{formatRelativeTime(bookmark.createdAt, '')}</div>
                  </button>
                );
              })
            )}
          </div>
        </section>

        {/* Recently indexed */}
        <section>
          <SectionHeading>Recently indexed</SectionHeading>
          <div className="border-t border-border-subtle">
            {recentDocuments.length === 0 ? (
              <EmptyRow>Add your first files to make your library searchable.</EmptyRow>
            ) : (
              recentDocuments.map((doc: RecentDocument, index: number) => (
                <div
                  key={doc.id || index}
                  className="flex items-center gap-3 border-b border-border-subtle px-2 py-3 transition-colors duration-fast hover:bg-surface"
                >
                  <FileText className="h-4 w-4 shrink-0 text-text-tertiary" strokeWidth={1.75} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-text-primary">
                      {doc.documentName || doc.documentPath || 'Untitled'}
                    </div>
                    <div className="text-xs text-text-muted">
                      {formatRelativeTime(doc.lastAccessedAt, '')}
                      {doc.fileType ? ` · ${doc.fileType.toUpperCase()}` : ''}
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
        </section>
        </div>
      </div>
    </main>
  );
};
