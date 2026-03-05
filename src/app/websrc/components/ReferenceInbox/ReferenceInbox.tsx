import { useCallback, useEffect, useMemo, useState } from 'react';

import {
  ArrowUpRight,
  Check,
  Copy,
  Loader2,
  MessageSquare,
  NotebookPen,
  RefreshCw,
  Search,
  Sparkles,
  Trash2,
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { TiptapViewer } from '../TiptapEditor';

import { useDebounce } from '@/hooks/useDebounce';
import { VaultAPI } from '@/lib/api';
import { toast } from '@/stores/toastStore';
import type { ConversationMessageBookmarkDto } from '@/types';
import type {
  ConversationJournalDto,
  ConversationSpaceDto,
} from '@/types/api/conversation';
import { resolveBookmarkPayload, type BookmarkPayload } from '@/utils/chatBookmarks';
import { captureChatReferenceToWorkspaceNote } from '@/utils/chatReferenceCapture';
import {
  buildCapturedChatReferenceIndex,
  chatReferenceKey,
  type CapturedChatReference,
} from '@/utils/chatReferenceIndex';

type RoleFilter = 'all' | 'assistant' | 'user' | 'system';
type StatusFilter = 'all' | 'pending' | 'captured';

const ROLE_FILTERS: Array<{ value: RoleFilter; label: string }> = [
  { value: 'all', label: 'All' },
  { value: 'assistant', label: 'AI' },
  { value: 'user', label: 'You' },
  { value: 'system', label: 'System' },
];

const STATUS_FILTERS: Array<{ value: StatusFilter; label: string }> = [
  { value: 'all', label: 'All' },
  { value: 'pending', label: 'Pending' },
  { value: 'captured', label: 'Captured' },
];

export function ReferenceInbox() {
  const navigate = useNavigate();
  const [query, setQuery] = useState('');
  const [roleFilter, setRoleFilter] = useState<RoleFilter>('all');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [bookmarks, setBookmarks] = useState<ConversationMessageBookmarkDto[]>([]);
  const [spacesById, setSpacesById] = useState<Map<string, ConversationSpaceDto>>(new Map());
  const [journalsById, setJournalsById] = useState<Map<string, ConversationJournalDto>>(new Map());
  const [capturedIndex, setCapturedIndex] = useState<Map<string, CapturedChatReference>>(new Map());
  const [payloadCache, setPayloadCache] = useState<Map<string, BookmarkPayload>>(new Map());
  const [selectedBookmarkId, setSelectedBookmarkId] = useState<string | null>(null);
  const [titleDraft, setTitleDraft] = useState('');
  const [noteDraft, setNoteDraft] = useState('');
  const [isLoading, setIsLoading] = useState(true);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isRemoving, setIsRemoving] = useState(false);
  const [isCapturingOne, setIsCapturingOne] = useState(false);
  const [isCapturingPending, setIsCapturingPending] = useState(false);
  const [isResolvingPreview, setIsResolvingPreview] = useState(false);
  const [isCopied, setIsCopied] = useState(false);

  const debouncedQuery = useDebounce(query, 220);

  const loadInbox = useCallback(
    async (showRefreshState: boolean) => {
      if (showRefreshState) {
        setIsRefreshing(true);
      } else {
        setIsLoading(true);
      }

      const [bookmarksResult, notesResult, spacesResult, journalsResult] = await Promise.all([
        VaultAPI.listMessageBookmarks({
          query: debouncedQuery.trim() ? debouncedQuery.trim() : undefined,
          limit: 200,
          offset: 0,
        }),
        VaultAPI.listWorkspaceNotes(),
        VaultAPI.listConversationSpaces(),
        VaultAPI.listJournals(),
      ]);

      if (!bookmarksResult.ok) {
        toast.error('Failed to load references', { message: bookmarksResult.error });
      } else {
        setBookmarks(bookmarksResult.data.bookmarks);
      }

      if (notesResult.ok) {
        setCapturedIndex(buildCapturedChatReferenceIndex(notesResult.data.notes));
      } else {
        setCapturedIndex(new Map());
      }

      if (spacesResult.ok) {
        setSpacesById(new Map(spacesResult.data.map((space) => [space.id, space])));
      } else {
        setSpacesById(new Map());
      }

      if (journalsResult.ok) {
        setJournalsById(new Map(journalsResult.data.map((journal) => [journal.id, journal])));
      } else {
        setJournalsById(new Map());
      }

      setIsLoading(false);
      setIsRefreshing(false);
    },
    [debouncedQuery]
  );

  useEffect(() => {
    void loadInbox(false);
  }, [loadInbox]);

  const filteredBookmarks = useMemo(
    () =>
      bookmarks.filter((bookmark) => {
        if (roleFilter !== 'all' && bookmark.messageRole !== roleFilter) {
          return false;
        }
        if (statusFilter === 'all') {
          return true;
        }
        const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
        const captured = capturedIndex.has(key);
        return statusFilter === 'captured' ? captured : !captured;
      }),
    [bookmarks, capturedIndex, roleFilter, statusFilter]
  );

  const stats = useMemo(() => {
    const total = bookmarks.length;
    const captured = bookmarks.reduce((count, bookmark) => {
      const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
      return capturedIndex.has(key) ? count + 1 : count;
    }, 0);
    return {
      total,
      captured,
      pending: Math.max(0, total - captured),
    };
  }, [bookmarks, capturedIndex]);

  const selectedBookmark = useMemo(() => {
    if (!selectedBookmarkId) return null;
    return filteredBookmarks.find((bookmark) => bookmark.id === selectedBookmarkId) ?? null;
  }, [filteredBookmarks, selectedBookmarkId]);

  useEffect(() => {
    if (filteredBookmarks.length === 0) {
      setSelectedBookmarkId(null);
      setTitleDraft('');
      setNoteDraft('');
      return;
    }

    const resolved =
      filteredBookmarks.find((bookmark) => bookmark.id === selectedBookmarkId) ?? filteredBookmarks[0];
    setSelectedBookmarkId(resolved.id);
    setTitleDraft(resolved.title ?? '');
    setNoteDraft(resolved.note ?? '');
  }, [filteredBookmarks, selectedBookmarkId]);

  const selectedCapture = selectedBookmark
    ? capturedIndex.get(chatReferenceKey(selectedBookmark.conversationId, selectedBookmark.messageId)) ?? null
    : null;
  const selectedBookmarkSpace = selectedBookmark
    ? spacesById.get(selectedBookmark.spaceId) ?? null
    : null;

  const getJournalNoteIdForSpace = useCallback((spaceId: string): string | null => {
    try {
      const value = localStorage.getItem(`journal.noteBySpace.${spaceId}`);
      const resolved = value?.trim() ?? '';
      return resolved || null;
    } catch {
      return null;
    }
  }, []);

  const resolveCaptureDestinationForBookmark = useCallback((bookmark: ConversationMessageBookmarkDto) => {
    const journal = journalsById.get(bookmark.spaceId) ?? null;
    if (journal) {
      const journalNoteId = getJournalNoteIdForSpace(journal.id);
      return {
        type: 'journal' as const,
        space: journal,
        preferredNoteId: journalNoteId,
      };
    }

    return {
      type: 'daily' as const,
      space: null,
      preferredNoteId: null,
    };
  }, [getJournalNoteIdForSpace, journalsById]);

  const selectedCaptureDestination = selectedBookmark
    ? resolveCaptureDestinationForBookmark(selectedBookmark)
    : null;

  const hasAnnotationChanges = Boolean(
    selectedBookmark &&
      (titleDraft !== (selectedBookmark.title ?? '') || noteDraft !== (selectedBookmark.note ?? ''))
  );

  const getBookmarkPayload = useCallback(
    async (bookmark: ConversationMessageBookmarkDto): Promise<BookmarkPayload> => {
      const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
      const cached = payloadCache.get(key);
      if (cached) {
        return cached;
      }

      const payload = await resolveBookmarkPayload(bookmark);
      setPayloadCache((current) => {
        const next = new Map(current);
        next.set(key, payload);
        return next;
      });
      return payload;
    },
    [payloadCache]
  );

  const openCapturedReference = (reference: CapturedChatReference | null) => {
    if (!reference) {
      navigate('/journals');
      return;
    }

    let resolvedJournalSpaceId: string | null = null;
    for (const journal of journalsById.values()) {
      if (getJournalNoteIdForSpace(journal.id) === reference.noteId) {
        resolvedJournalSpaceId = journal.id;
        break;
      }
    }

    const params = new URLSearchParams({
      noteId: reference.noteId,
      snapshotId: reference.snapshotId,
    });
    if (resolvedJournalSpaceId) {
      params.set('journalSpaceId', resolvedJournalSpaceId);
    }
    navigate(`/journals?${params.toString()}`);
  };

  const openBookmarkInChat = (bookmark: ConversationMessageBookmarkDto) => {
    const params = new URLSearchParams({
      conversationId: bookmark.conversationId,
      messageId: bookmark.messageId,
    });
    navigate(`/chat?${params.toString()}`);
  };

  const saveAnnotations = async () => {
    if (!selectedBookmark || isSaving || !hasAnnotationChanges) return;

    setIsSaving(true);
    const result = await VaultAPI.bookmarkConversationMessage({
      conversationId: selectedBookmark.conversationId,
      messageId: selectedBookmark.messageId,
      title: titleDraft.trim() ? titleDraft.trim() : null,
      note: noteDraft.trim() ? noteDraft.trim() : null,
    });
    setIsSaving(false);

    if (!result.ok) {
      toast.error('Failed to save annotation', { message: result.error });
      return;
    }

    setBookmarks((current) =>
      current.map((bookmark) =>
        bookmark.id === selectedBookmark.id
          ? {
              ...bookmark,
              title: titleDraft.trim() || null,
              note: noteDraft.trim() || null,
            }
          : bookmark
      )
    );
  };

  const captureBookmark = useCallback(
    async (
      bookmark: ConversationMessageBookmarkDto,
      options?: { referenceTitle?: string | null; referenceNote?: string | null }
    ): Promise<CapturedChatReference | null> => {
      const destination = resolveCaptureDestinationForBookmark(bookmark);
      const payload = await getBookmarkPayload(bookmark);
      const result = await captureChatReferenceToWorkspaceNote({
        conversationId: bookmark.conversationId,
        conversationTitle: bookmark.conversationTitle,
        messageId: bookmark.messageId,
        messageRole: bookmark.messageRole,
        messageContent: payload.content,
        referenceTitle: options?.referenceTitle ?? bookmark.title,
        referenceNote: options?.referenceNote ?? bookmark.note,
        sourceReferences: payload.sourceReferences,
        preferredNoteId: destination.preferredNoteId,
      });

      if (!result.snapshotId) {
        return null;
      }

      const capturedReference: CapturedChatReference = {
        conversationId: bookmark.conversationId,
        messageId: bookmark.messageId,
        snapshotId: result.snapshotId,
        capturedAt: new Date().toISOString(),
        noteId: result.noteId,
        noteTitle: result.noteTitle,
      };

      const key = chatReferenceKey(bookmark.conversationId, bookmark.messageId);
      setCapturedIndex((current) => {
        const next = new Map(current);
        next.set(key, capturedReference);
        return next;
      });

      return capturedReference;
    },
    [getBookmarkPayload, resolveCaptureDestinationForBookmark]
  );

  const captureSelected = async () => {
    if (!selectedBookmark || isCapturingOne) return;

    setIsCapturingOne(true);
    try {
      const capturedReference = await captureBookmark(selectedBookmark, {
        referenceTitle: titleDraft.trim() || selectedBookmark.title,
        referenceNote: noteDraft.trim() || selectedBookmark.note,
      });
      toast.success('Reference captured', {
        message: capturedReference
          ? `Saved to "${capturedReference.noteTitle}".`
          : 'Saved to Journals Inbox.',
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Unknown error';
      toast.error('Failed to capture reference', { message });
    } finally {
      setIsCapturingOne(false);
    }
  };

  const removeSelected = async () => {
    if (!selectedBookmark || isRemoving) return;

    setIsRemoving(true);
    const result = await VaultAPI.unbookmarkConversationMessage({
      conversationId: selectedBookmark.conversationId,
      messageId: selectedBookmark.messageId,
    });
    setIsRemoving(false);

    if (!result.ok) {
      toast.error('Failed to remove reference', { message: result.error });
      return;
    }

    const key = chatReferenceKey(selectedBookmark.conversationId, selectedBookmark.messageId);
    setBookmarks((current) => current.filter((bookmark) => bookmark.id !== selectedBookmark.id));
    setCapturedIndex((current) => {
      if (!current.has(key)) return current;
      const next = new Map(current);
      next.delete(key);
      return next;
    });
    setPayloadCache((current) => {
      if (!current.has(key)) return current;
      const next = new Map(current);
      next.delete(key);
      return next;
    });
    toast.success('Reference removed');
  };

  const capturePending = async () => {
    if (isCapturingPending) return;

    const pending = filteredBookmarks
      .filter(
        (bookmark) =>
          !capturedIndex.has(chatReferenceKey(bookmark.conversationId, bookmark.messageId))
      )
      .slice(0, 24);

    if (pending.length === 0) {
      toast.success('No pending references');
      return;
    }

    setIsCapturingPending(true);
    let captured = 0;
    let failed = 0;
    let latest: CapturedChatReference | null = null;

    for (const bookmark of pending) {
      try {
        const reference = await captureBookmark(bookmark);
        if (reference) {
          latest = reference;
        }
        captured += 1;
      } catch {
        failed += 1;
      }
    }

    setIsCapturingPending(false);
    if (captured > 0) {
      toast.success('Pending references captured', {
        message: failed > 0 ? `${captured} captured, ${failed} failed.` : `${captured} captured.`,
        action: latest
          ? {
              label: 'Open Capture',
              onClick: () => openCapturedReference(latest),
            }
          : undefined,
      });
      return;
    }

    toast.error('Failed to capture pending references');
  };

  const copySelected = async () => {
    if (!selectedBookmark) return;
    try {
      const payload = await getBookmarkPayload(selectedBookmark);
      await navigator.clipboard.writeText(payload.content);
      setIsCopied(true);
      window.setTimeout(() => setIsCopied(false), 1300);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Unknown error';
      toast.error('Failed to copy full reference', { message });
    }
  };

  const selectedPreview = useMemo(() => {
    if (!selectedBookmark) return '';
    const key = chatReferenceKey(selectedBookmark.conversationId, selectedBookmark.messageId);
    return payloadCache.get(key)?.content ?? selectedBookmark.messagePreview;
  }, [payloadCache, selectedBookmark]);

  useEffect(() => {
    if (!selectedBookmark) return;
    const key = chatReferenceKey(selectedBookmark.conversationId, selectedBookmark.messageId);
    if (payloadCache.has(key)) return;

    let cancelled = false;
    setIsResolvingPreview(true);
    void resolveBookmarkPayload(selectedBookmark)
      .then((payload) => {
        if (cancelled) return;
        setPayloadCache((current) => {
          const next = new Map(current);
          next.set(key, payload);
          return next;
        });
      })
      .catch(() => {
        // Keep using bookmark preview text in the panel if full payload resolution fails.
      })
      .finally(() => {
        if (!cancelled) {
          setIsResolvingPreview(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [payloadCache, selectedBookmark]);

  return (
    <div className="h-full overflow-hidden bg-[radial-gradient(circle_at_top_right,rgba(16,185,129,0.14),transparent_42%),radial-gradient(circle_at_bottom_left,rgba(34,211,238,0.12),transparent_46%),var(--bg-primary)] text-[var(--text-primary)]">
      <div className="h-full flex flex-col">
        <header className="border-b border-white/10 bg-black/20 px-4 py-3 backdrop-blur-sm md:px-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p className="text-[11px] uppercase tracking-[0.16em] text-emerald-100/70">
                Integrated Retrieval
              </p>
              <h1 className="text-lg font-semibold text-white/90">Reference Inbox</h1>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => void loadInbox(true)}
                disabled={isRefreshing || isLoading}
                className="inline-flex items-center gap-1.5 rounded-md border border-white/15 bg-white/5 px-2.5 py-1.5 text-xs text-white/75 transition-colors hover:border-white/30 hover:text-white disabled:opacity-50"
              >
                <RefreshCw className={`h-3.5 w-3.5 ${isRefreshing ? 'animate-spin' : ''}`} />
                Refresh
              </button>
              <button
                onClick={capturePending}
                disabled={isCapturingPending || stats.pending === 0}
                className="inline-flex items-center gap-1.5 rounded-md border border-emerald-300/40 bg-emerald-500/15 px-2.5 py-1.5 text-xs text-emerald-100 transition-colors hover:border-emerald-200/65 disabled:opacity-45"
              >
                {isCapturingPending ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Sparkles className="h-3.5 w-3.5" />
                )}
                Capture Pending
              </button>
            </div>
          </div>

          <div className="mt-3 grid grid-cols-1 gap-2 md:grid-cols-[minmax(0,1fr)_auto_auto]">
            <div className="relative">
              <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-white/40" />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search by reference title, note, or conversation..."
                className="w-full rounded-lg border border-white/10 bg-white/[0.04] py-2.5 pl-9 pr-3 text-sm text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-emerald-300/60"
              />
            </div>
            <div className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.03] p-1">
              {ROLE_FILTERS.map((filter) => (
                <button
                  key={filter.value}
                  onClick={() => setRoleFilter(filter.value)}
                  className={`rounded-md px-2.5 py-1 text-xs transition-colors ${
                    roleFilter === filter.value
                      ? 'bg-cyan-500/20 text-cyan-100'
                      : 'text-white/60 hover:text-white/85'
                  }`}
                >
                  {filter.label}
                </button>
              ))}
            </div>
            <div className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.03] p-1">
              {STATUS_FILTERS.map((filter) => (
                <button
                  key={filter.value}
                  onClick={() => setStatusFilter(filter.value)}
                  className={`rounded-md px-2.5 py-1 text-xs transition-colors ${
                    statusFilter === filter.value
                      ? 'bg-emerald-500/20 text-emerald-100'
                      : 'text-white/60 hover:text-white/85'
                  }`}
                >
                  {filter.label}
                </button>
              ))}
            </div>
          </div>

          <div className="mt-2 text-xs text-white/55">
            {stats.total} total · {stats.pending} pending · {stats.captured} captured
          </div>
        </header>

        <div className="flex-1 min-h-0 p-4 md:p-5">
          <div className="grid h-full grid-cols-1 gap-4 xl:grid-cols-[22rem_minmax(0,1fr)]">
            <section className="min-h-0 overflow-hidden rounded-xl border border-white/10 bg-black/20">
              <div className="border-b border-white/10 px-3 py-2 text-xs uppercase tracking-wide text-white/55">
                References
              </div>
              <div className="h-full overflow-y-auto p-2">
                {isLoading ? (
                  <div className="flex h-32 items-center justify-center text-white/45">
                    <Loader2 className="h-5 w-5 animate-spin" />
                  </div>
                ) : filteredBookmarks.length === 0 ? (
                  <div className="rounded-md border border-white/10 bg-white/[0.02] px-3 py-4 text-xs text-white/55">
                    No references match the current filters.
                  </div>
                ) : (
                  filteredBookmarks.map((bookmark) => {
                    const isSelected = bookmark.id === selectedBookmarkId;
                    const capture = capturedIndex.get(
                      chatReferenceKey(bookmark.conversationId, bookmark.messageId)
                    );
                    return (
                      <button
                        key={bookmark.id}
                        onClick={() => setSelectedBookmarkId(bookmark.id)}
                        className={`mb-2 w-full rounded-lg border px-3 py-2 text-left transition-colors ${
                          isSelected
                            ? 'border-cyan-300/45 bg-cyan-500/10'
                            : 'border-white/10 bg-white/[0.02] hover:border-white/25'
                        }`}
                      >
                        <div className="mb-1 flex items-center justify-between gap-2">
                          <div className="flex items-center gap-1.5">
                            <span className="text-[10px] uppercase tracking-wide text-cyan-200/90">
                              {bookmark.messageRole}
                            </span>
                            {capture && (
                              <span className="rounded-full border border-emerald-300/40 bg-emerald-500/15 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-emerald-100">
                                Captured
                              </span>
                            )}
                          </div>
                          <span className="text-[10px] text-white/45">
                            {new Date(bookmark.createdAt).toLocaleDateString()}
                          </span>
                        </div>
                        <p className="line-clamp-1 text-xs text-white/90">
                          {bookmark.title || bookmark.conversationTitle}
                        </p>
                        <p className="mt-1 line-clamp-2 text-[11px] text-white/55">
                          {bookmark.messagePreview}
                        </p>
                      </button>
                    );
                  })
                )}
              </div>
            </section>

            <section className="min-h-0 overflow-hidden rounded-xl border border-white/10 bg-black/20">
              <div className="border-b border-white/10 px-3 py-2 text-xs uppercase tracking-wide text-white/55">
                Detail
              </div>
              <div className="h-full overflow-y-auto p-3">
                {!selectedBookmark ? (
                  <div className="rounded-lg border border-white/10 bg-white/[0.02] p-4 text-sm text-white/55">
                    Select a reference to inspect, annotate, capture, or reopen context.
                  </div>
                ) : (
                  <div className="space-y-3">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="inline-flex items-center gap-1 rounded-full border border-white/20 bg-white/5 px-2 py-1 text-[11px] text-white/70">
                        <MessageSquare className="h-3 w-3" />
                        {selectedBookmark.conversationTitle}
                      </span>
                      {selectedBookmarkSpace && (
                        <span className="inline-flex items-center gap-1 rounded-full border border-white/20 bg-white/5 px-2 py-1 text-[11px] text-white/70">
                          <NotebookPen className="h-3 w-3" />
                          {selectedBookmarkSpace.name}
                        </span>
                      )}
                      {selectedCapture ? (
                        <span className="inline-flex items-center gap-1 rounded-full border border-emerald-300/40 bg-emerald-500/14 px-2 py-1 text-[11px] text-emerald-100">
                          <Check className="h-3 w-3" />
                          Captured
                        </span>
                      ) : (
                        <span className="inline-flex items-center gap-1 rounded-full border border-amber-300/40 bg-amber-500/14 px-2 py-1 text-[11px] text-amber-100">
                          Pending
                        </span>
                      )}
                    </div>

                    {selectedCaptureDestination && (
                      <div className="rounded-md border border-white/10 bg-white/[0.03] px-2.5 py-2">
                        <p className="text-[11px] uppercase tracking-wide text-white/55">Capture Destination</p>
                        <p className="mt-1 text-xs text-white/80">
                          {selectedCaptureDestination.type === 'journal' && selectedCaptureDestination.space
                            ? `Journal notebook: ${selectedCaptureDestination.space.name}`
                            : 'Daily Research Inbox'}
                        </p>
                        {selectedCaptureDestination.type === 'journal' && selectedCaptureDestination.space && (
                          <div className="mt-1.5 flex items-center gap-2">
                            <button
                              onClick={() => navigate(`/journals?journalSpaceId=${encodeURIComponent(selectedCaptureDestination.space.id)}`)}
                              className="inline-flex items-center gap-1 rounded-md border border-emerald-300/40 bg-emerald-500/12 px-2 py-1 text-[11px] text-emerald-100 transition-colors hover:border-emerald-200/70"
                            >
                              <ArrowUpRight className="h-3 w-3" />
                              Open Journal Notebook
                            </button>
                            {!selectedCaptureDestination.preferredNoteId && (
                              <span className="text-[11px] text-amber-100/80">
                                Open once to initialize notebook mapping.
                              </span>
                            )}
                          </div>
                        )}
                      </div>
                    )}

                    <div className="space-y-2">
                      <input
                        value={titleDraft}
                        onChange={(event) => setTitleDraft(event.target.value)}
                        placeholder="Reference title"
                        className="w-full rounded-md border border-white/10 bg-white/[0.03] px-2.5 py-2 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-cyan-300/65"
                      />
                      <textarea
                        value={noteDraft}
                        onChange={(event) => setNoteDraft(event.target.value)}
                        rows={3}
                        placeholder="Add context, takeaway, or follow-up note"
                        className="w-full rounded-md border border-white/10 bg-white/[0.03] px-2.5 py-2 text-xs text-white/90 placeholder:text-white/40 focus:outline-none focus:ring-1 focus:ring-cyan-300/65"
                      />
                    </div>

                    <div className="grid grid-cols-2 gap-2">
                      <button
                        onClick={saveAnnotations}
                        disabled={!hasAnnotationChanges || isSaving}
                        className="inline-flex items-center justify-center gap-1 rounded-md border border-cyan-300/35 px-2 py-1.5 text-[11px] text-cyan-100 transition-colors hover:border-cyan-200/70 disabled:opacity-50"
                      >
                        {isSaving ? <Loader2 className="h-3 w-3 animate-spin" /> : <NotebookPen className="h-3 w-3" />}
                        Save
                      </button>
                      <button
                        onClick={captureSelected}
                        disabled={isCapturingOne}
                        className="inline-flex items-center justify-center gap-1 rounded-md border border-emerald-300/40 bg-emerald-500/15 px-2 py-1.5 text-[11px] text-emerald-100 transition-colors hover:border-emerald-200/70 disabled:opacity-50"
                      >
                        {isCapturingOne ? <Loader2 className="h-3 w-3 animate-spin" /> : <Sparkles className="h-3 w-3" />}
                        {selectedCaptureDestination?.type === 'journal'
                          ? (selectedCapture ? 'Re-capture to Journal' : 'Capture to Journal')
                          : (selectedCapture ? 'Re-capture' : 'Capture')}
                      </button>
                      <button
                        onClick={() => openBookmarkInChat(selectedBookmark)}
                        className="inline-flex items-center justify-center gap-1 rounded-md border border-white/20 px-2 py-1.5 text-[11px] text-white/75 transition-colors hover:border-white/35 hover:text-white"
                      >
                        <ArrowUpRight className="h-3 w-3" />
                        Open in Chat
                      </button>
                      <button
                        onClick={copySelected}
                        className="inline-flex items-center justify-center gap-1 rounded-md border border-white/20 px-2 py-1.5 text-[11px] text-white/75 transition-colors hover:border-white/35 hover:text-white"
                      >
                        {isCopied ? <Check className="h-3 w-3 text-emerald-300" /> : <Copy className="h-3 w-3" />}
                        Copy
                      </button>
                      <button
                        onClick={removeSelected}
                        disabled={isRemoving}
                        className="inline-flex items-center justify-center gap-1 rounded-md border border-rose-300/35 px-2 py-1.5 text-[11px] text-rose-200 transition-colors hover:border-rose-200/70 disabled:opacity-50"
                      >
                        {isRemoving ? <Loader2 className="h-3 w-3 animate-spin" /> : <Trash2 className="h-3 w-3" />}
                        Remove
                      </button>
                    </div>

                    {selectedCapture && (
                      <button
                        onClick={() => openCapturedReference(selectedCapture)}
                        className="w-full inline-flex items-center justify-center gap-1 rounded-md border border-emerald-300/35 bg-emerald-500/10 px-2 py-1.5 text-[11px] text-emerald-100 transition-colors hover:border-emerald-200/70"
                      >
                        <NotebookPen className="h-3 w-3" />
                        Open Captured Note
                      </button>
                    )}

                    <div className="rounded-lg border border-white/10 bg-black/25 p-3">
                      <div className="mb-1 flex items-center justify-between gap-2">
                        <p className="text-[11px] uppercase tracking-wide text-white/45">
                          Message Content
                        </p>
                        {isResolvingPreview && <Loader2 className="h-3.5 w-3.5 animate-spin text-white/45" />}
                      </div>
                      <div className="max-h-[28rem] overflow-y-auto rounded-md border border-white/10 bg-black/20 p-2.5">
                        <div className="max-w-none break-words [overflow-wrap:anywhere]">
                          <TiptapViewer content={selectedPreview} />
                        </div>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </section>
          </div>
        </div>
      </div>
    </div>
  );
}
