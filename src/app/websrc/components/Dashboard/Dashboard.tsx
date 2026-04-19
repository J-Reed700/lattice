import {
  Bookmark,
  FileText,
  FileUp,
  Globe,
  MessageSquare,
  NotebookPen,
  Search,
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { useDashboardQuery } from '@/hooks/queries';
import type {
  DashboardConversationSummary,
  DashboardJournalEntrySummary,
} from '@/hooks/queries/useDashboardQuery';
import type { ConversationMessageBookmarkDto } from '@/types/api/conversation';
import type { RecentDocument } from '@/types';
import { formatBytes, formatRelativeTime } from '@/utils/formatters';

import { DashboardError } from './DashboardError';
import { DashboardSkeleton } from './DashboardSkeleton';

interface DashboardProps {
  onNavigate?: (view: 'search' | 'files' | 'settings') => void;
}

function formatTodayLabel(): string {
  const now = new Date();
  return now.toLocaleDateString(undefined, {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
  });
}

function truncate(value: string, max: number): string {
  const trimmed = value.trim();
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max).trimEnd()}…`;
}

interface QuickAction {
  icon: typeof FileUp;
  label: string;
  description: string;
  onClick: () => void;
}

interface StatEntry {
  label: string;
  value: string;
}

interface SectionHeadingProps {
  children: string;
}

function SectionHeading({ children }: SectionHeadingProps) {
  return (
    <h2 className="pb-3 text-lg font-medium text-[hsl(var(--text-secondary))]">
      {children}
    </h2>
  );
}

interface EmptyRowProps {
  children: string;
}

function EmptyRow({ children }: EmptyRowProps) {
  return (
    <p className="border-b border-[hsl(var(--border-subtle))] px-2 py-4 text-sm text-[hsl(var(--text-tertiary))]">
      {children}
    </p>
  );
}

export const Dashboard = (_props: DashboardProps) => {
  const { data, isLoading, error } = useDashboardQuery();
  const navigate = useNavigate();

  if (isLoading) {
    return <DashboardSkeleton />;
  }

  if (error) {
    return <DashboardError error={error.message} />;
  }

  const stats: StatEntry[] = [
    {
      label: 'Indexed',
      value: String(data?.stats?.documentCount ?? 0),
    },
    {
      label: 'Storage',
      value: formatBytes(data?.stats?.storageUsed ?? 0),
    },
    {
      label: 'Searches today',
      value: String(data?.stats?.searchCount ?? 0),
    },
    {
      label: 'Last indexed',
      value: formatRelativeTime(data?.stats?.lastIndexed, 'Never'),
    },
  ];

  const recentConversations: DashboardConversationSummary[] =
    data?.recentConversations ?? [];
  const todayEntry: DashboardJournalEntrySummary | null =
    data?.todayJournalEntry ?? null;
  const preferredJournalSpaceId = data?.preferredJournalSpaceId ?? null;
  const recentReferences: ConversationMessageBookmarkDto[] =
    data?.recentReferences ?? [];
  const journalSpaceIdSet = new Set(data?.journalSpaceIds ?? []);
  const recentDocuments = (data?.recentDocuments ?? []).slice(0, 5);

  const quickActions: QuickAction[] = [
    {
      icon: MessageSquare,
      label: 'New conversation',
      description: 'Start a chat with your index',
      onClick: () => navigate('/chat'),
    },
    {
      icon: NotebookPen,
      label: 'New journal entry',
      description: "Capture today's thinking",
      onClick: () => navigate('/journals'),
    },
    {
      icon: FileUp,
      label: 'Add files',
      description: 'Import local files',
      onClick: () => navigate('/ingest'),
    },
    {
      icon: Globe,
      label: 'Add web page',
      description: 'Import from URL',
      onClick: () => navigate('/ingest'),
    },
    {
      icon: Search,
      label: 'Search',
      description: 'Find in your index',
      onClick: () => navigate('/search'),
    },
  ];

  const goToConversation = (conversationId: string) => {
    const params = new URLSearchParams({ conversationId });
    navigate(`/chat?${params.toString()}`);
  };

  const goToJournal = () => {
    if (todayEntry) {
      const params = new URLSearchParams({
        journalSpaceId: todayEntry.journalSpaceId,
        entryId: todayEntry.entryId,
      });
      navigate(`/journals?${params.toString()}`);
      return;
    }
    if (preferredJournalSpaceId) {
      const params = new URLSearchParams({
        journalSpaceId: preferredJournalSpaceId,
      });
      navigate(`/journals?${params.toString()}`);
      return;
    }
    navigate('/journals');
  };

  const goToReference = (bookmark: ConversationMessageBookmarkDto) => {
    const params = new URLSearchParams({ referenceId: bookmark.id });
    navigate(`/references?${params.toString()}`);
  };

  return (
    <main className="relative h-full overflow-y-auto bg-[hsl(var(--bg))]">
      <div className="mx-auto w-full max-w-[760px] px-6 pt-10 pb-16">
        <header className="mb-12">
          <h1 className="font-serif text-3xl font-semibold tracking-[-0.02em] text-[hsl(var(--text-primary))]">
            Dashboard
          </h1>
          <p className="mt-1 text-sm text-[hsl(var(--text-tertiary))]">
            {formatTodayLabel()}
          </p>
        </header>

        {/* Stats strip */}
        <section className="mb-12">
          <div className="grid grid-cols-2 gap-x-12 gap-y-8 sm:grid-cols-4">
            {stats.map((stat) => (
              <div key={stat.label}>
                <div className="text-[11px] font-medium uppercase tracking-[0.04em] text-[hsl(var(--text-tertiary))]">
                  {stat.label}
                </div>
                <div className="mt-1 text-2xl font-semibold tabular-nums text-[hsl(var(--text-primary))]">
                  {stat.value}
                </div>
              </div>
            ))}
          </div>
        </section>

        {/* Continue where you left off */}
        <section className="mb-12">
          <SectionHeading>Continue</SectionHeading>
          <div className="border-t border-[hsl(var(--border-subtle))]">
            {recentConversations.length === 0 ? (
              <EmptyRow>No conversations yet. Start one in Chat.</EmptyRow>
            ) : (
              recentConversations.map((conv) => {
                const preview = conv.lastMessagePreview?.trim() ?? '';
                return (
                  <button
                    key={conv.id}
                    type="button"
                    onClick={() => goToConversation(conv.id)}
                    className="flex w-full items-start gap-3 border-b border-[hsl(var(--border-subtle))] px-2 py-3 text-left transition-colors duration-fast hover:bg-[hsl(var(--surface))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
                  >
                    <MessageSquare
                      className="mt-0.5 h-4 w-4 shrink-0 text-[hsl(var(--text-tertiary))]"
                      strokeWidth={1.75}
                    />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm font-medium text-[hsl(var(--text-primary))]">
                        {conv.title || 'Untitled conversation'}
                      </div>
                      {preview && (
                        <div className="mt-0.5 truncate text-xs text-[hsl(var(--text-tertiary))]">
                          {preview}
                        </div>
                      )}
                    </div>
                    <div className="shrink-0 text-xs text-[hsl(var(--text-muted))] tabular-nums">
                      {formatRelativeTime(conv.updatedAt, '')}
                    </div>
                  </button>
                );
              })
            )}
          </div>
        </section>

        {/* Today's journal */}
        <section className="mb-12">
          <SectionHeading>Today</SectionHeading>
          <div className="border-t border-[hsl(var(--border-subtle))]">
            {todayEntry ? (
              <button
                type="button"
                onClick={goToJournal}
                className="flex w-full flex-col gap-3 border-b border-[hsl(var(--border-subtle))] px-2 py-4 text-left transition-colors duration-fast hover:bg-[hsl(var(--surface))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
              >
                <div className="flex items-center justify-between gap-3">
                  <div className="truncate text-sm font-medium text-[hsl(var(--text-primary))]">
                    {todayEntry.title || 'Untitled entry'}
                  </div>
                  <div className="shrink-0 text-xs text-[hsl(var(--text-muted))] tabular-nums">
                    {todayEntry.messageCount}
                    {' '}
                    {todayEntry.messageCount === 1 ? 'message' : 'messages'}
                  </div>
                </div>
                {todayEntry.lastMessagePreview?.trim() && (
                  <p className="line-clamp-3 text-sm text-[hsl(var(--text-tertiary))]">
                    {truncate(todayEntry.lastMessagePreview, 200)}
                  </p>
                )}
                <div className="text-xs font-medium text-[hsl(var(--accent))]">
                  Continue writing
                </div>
              </button>
            ) : (
              <button
                type="button"
                onClick={goToJournal}
                className="flex w-full items-center gap-3 border-b border-[hsl(var(--border-subtle))] px-2 py-4 text-left transition-colors duration-fast hover:bg-[hsl(var(--surface))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
              >
                <NotebookPen
                  className="h-4 w-4 shrink-0 text-[hsl(var(--text-tertiary))]"
                  strokeWidth={1.75}
                />
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium text-[hsl(var(--text-primary))]">
                    Start today's entry
                  </div>
                  <div className="mt-0.5 text-xs text-[hsl(var(--text-muted))]">
                    Capture what's on your mind in your journal.
                  </div>
                </div>
              </button>
            )}
          </div>
        </section>

        {/* Recent references */}
        <section className="mb-12">
          <SectionHeading>Recent references</SectionHeading>
          <div className="border-t border-[hsl(var(--border-subtle))]">
            {recentReferences.length === 0 ? (
              <EmptyRow>
                Reference any message with the bookmark icon to save it here.
              </EmptyRow>
            ) : (
              recentReferences.map((bookmark) => {
                const isJournal = bookmark.spaceId
                  ? journalSpaceIdSet.has(bookmark.spaceId)
                  : false;
                const originLabel = isJournal ? 'JOURNAL' : 'CHAT';
                const excerpt =
                  bookmark.title?.trim() ||
                  bookmark.note?.trim() ||
                  bookmark.messagePreview?.trim() ||
                  '';
                return (
                  <button
                    key={bookmark.id}
                    type="button"
                    onClick={() => goToReference(bookmark)}
                    className="flex w-full items-start gap-3 border-b border-[hsl(var(--border-subtle))] px-2 py-3 text-left transition-colors duration-fast hover:bg-[hsl(var(--surface))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
                  >
                    <Bookmark
                      className="mt-0.5 h-4 w-4 shrink-0 text-[hsl(var(--text-tertiary))]"
                      strokeWidth={1.75}
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                          {originLabel}
                        </span>
                        <span className="truncate text-xs text-[hsl(var(--text-tertiary))]">
                          {bookmark.conversationTitle || 'Untitled'}
                        </span>
                      </div>
                      {excerpt && (
                        <div className="mt-0.5 truncate text-sm text-[hsl(var(--text-primary))]">
                          {excerpt}
                        </div>
                      )}
                    </div>
                    <div className="shrink-0 text-xs text-[hsl(var(--text-muted))] tabular-nums">
                      {formatRelativeTime(bookmark.createdAt, '')}
                    </div>
                  </button>
                );
              })
            )}
          </div>
        </section>

        {/* Recently indexed */}
        <section className="mb-12">
          <SectionHeading>Recently indexed</SectionHeading>
          <div className="border-t border-[hsl(var(--border-subtle))]">
            {recentDocuments.length === 0 ? (
              <EmptyRow>Nothing indexed yet.</EmptyRow>
            ) : (
              recentDocuments.map((doc: RecentDocument, index: number) => (
                <div
                  key={doc.id || index}
                  className="flex items-center gap-3 border-b border-[hsl(var(--border-subtle))] px-2 py-3 transition-colors duration-fast hover:bg-[hsl(var(--surface))]"
                >
                  <FileText
                    className="h-4 w-4 shrink-0 text-[hsl(var(--text-tertiary))]"
                    strokeWidth={1.75}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-[hsl(var(--text-primary))]">
                      {doc.fileName || doc.filePath || 'Untitled'}
                    </div>
                    <div className="text-xs text-[hsl(var(--text-muted))]">
                      {formatRelativeTime(doc.modifiedAt || doc.indexedAt, '')}
                      {doc.fileType ? ` · ${doc.fileType}` : ''}
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
        </section>

        {/* Quick actions */}
        <section>
          <SectionHeading>Quick actions</SectionHeading>
          <div className="border-t border-[hsl(var(--border-subtle))]">
            {quickActions.map((action) => {
              const Icon = action.icon;
              return (
                <button
                  key={action.label}
                  type="button"
                  onClick={action.onClick}
                  className="flex w-full items-center gap-4 border-b border-[hsl(var(--border-subtle))] px-2 py-4 text-left transition-colors duration-fast hover:bg-[hsl(var(--surface))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
                >
                  <Icon
                    className="h-[18px] w-[18px] shrink-0 text-[hsl(var(--text-tertiary))]"
                    strokeWidth={1.75}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-[hsl(var(--text-primary))]">
                      {action.label}
                    </div>
                    <div className="text-xs text-[hsl(var(--text-muted))]">
                      {action.description}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        </section>
      </div>
    </main>
  );
};

export default Dashboard;
