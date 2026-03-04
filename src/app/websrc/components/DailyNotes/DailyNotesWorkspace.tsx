import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  BookOpen,
  ChevronLeft,
  ChevronRight,
  FileText,
  Highlighter,
  Link2,
  MessageSquare,
  NotebookPen,
  Pin,
  Plus,
  Search,
  Save,
  Sparkles,
  StickyNote,
  Trash2,
} from 'lucide-react';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import ReactMarkdown from 'react-markdown';
import { useNavigate, useSearchParams } from 'react-router-dom';
import remarkGfm from 'remark-gfm';

import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { DocumentMetadata } from '@/types';
import type { ConversationMessage as ChatConversationMessage } from '@/types/conversation';
import { getSourceExternalUrl } from '@/utils/sourcePreview';
import type {
  ConversationDto,
  MessageDto,
  ConversationMessageBookmarkDto,
  ConversationJournalDto,
} from '@/types/api/conversation';
import type {
  WorkspaceNote,
  SnapshotMessage,
  ConversationSnapshot,
} from '@/types/api/dailyNotes';

const DOCUMENT_LIMIT = 500;
const CONVERSATION_LIMIT = 100;
const PREVIEW_CHAR_LIMIT = 4000;
const HIGHLIGHT_CHAR_LIMIT = 8000;
const SYNTHESIS_ENTRY_LIMIT = 12;
const JOURNAL_SOURCE_SCAN_LIMIT = 24;
const LAST_JOURNAL_SPACE_KEY = 'journal.lastSpaceId';
const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

type MessageRole = 'user' | 'assistant' | 'system';
type StickyColor = 'amber' | 'sky' | 'rose' | 'mint';
type PanelTab = 'editor' | 'annotations' | 'documents' | 'chats' | 'snapshots';
type ActionTone = 'info' | 'success' | 'error';
type AnnotationView = 'highlights' | 'stickies';
type ResourceView = 'list' | 'preview';
type EditorView = 'edit' | 'preview' | 'split';
type JournalPanelQuery = 'entries' | 'pages' | 'highlights' | 'sources' | 'timeline';
type JournalSynthesisScope = 'current' | 'deck' | 'pinned';

interface ConversationSummary {
  id: string;
  title: string;
  updatedAt: string;
  messageCount: number;
}

interface DocumentPreviewState {
  loading: boolean;
  content: string;
  error: string | null;
}

interface JournalMessageSource {
  documentId: string | null;
  fileName: string;
  filePath: string;
  category: string;
  mimeType: string;
  excerpt: string;
  score: number;
}

interface JournalSourceSummary {
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

const stickyClasses: Record<StickyColor, string> = {
  amber: 'border-amber-300/70 bg-amber-200/85 text-amber-950',
  sky: 'border-sky-300/70 bg-sky-200/85 text-sky-950',
  rose: 'border-rose-300/70 bg-rose-200/85 text-rose-950',
  mint: 'border-emerald-300/70 bg-emerald-200/85 text-emerald-950',
};

const stickyColorOptions: Array<{ value: StickyColor; label: string }> = [
  { value: 'amber', label: 'Amber' },
  { value: 'sky', label: 'Sky' },
  { value: 'rose', label: 'Rose' },
  { value: 'mint', label: 'Mint' },
];

function makeId(prefix: string): string {
  return `${prefix}_${crypto.randomUUID()}`;
}

function asString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

function asNumber(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function asRole(value: unknown): MessageRole {
  if (value === 'user' || value === 'assistant' || value === 'system') {
    return value;
  }
  return 'assistant';
}

function unique(values: string[]): string[] {
  return [...new Set(values.filter(Boolean))];
}

function nowIso(): string {
  return new Date().toISOString();
}

function formatWhen(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

function defaultDailyTitle(): string {
  return `Journal · ${new Date().toLocaleDateString(undefined, {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
  })}`;
}

function defaultJournalTitle(spaceName: string): string {
  return `Journal · ${spaceName}`;
}

function normalizeStickyColor(color: string): StickyColor {
  return stickyColorOptions.some((option) => option.value === color)
    ? (color as StickyColor)
    : 'amber';
}

function normalizeDocument(raw: DocumentMetadata | Record<string, unknown>): DocumentMetadata {
  const source = raw as Partial<DocumentMetadata> & Record<string, unknown>;
  return {
    id: asString(source.id, makeId('doc')),
    fileName: asString(source.fileName ?? source.file_name, 'Untitled file'),
    filePath: asString(source.filePath ?? source.file_path),
    fileType: asString(source.fileType ?? source.file_type, 'unknown'),
    category: asString(source.category, 'uncategorized'),
    language: asString(source.language, 'unknown'),
    modifiedAt: asString(source.modifiedAt ?? source.modified_at, nowIso()),
    indexedAt: asString(source.indexedAt ?? source.indexed_at, nowIso()),
    wordCount: asNumber(source.wordCount ?? source.word_count, 0),
  };
}

function normalizeConversation(
  raw: ConversationDto | Record<string, unknown>,
): ConversationSummary {
  const source = raw as Partial<ConversationDto> & Record<string, unknown>;
  const updatedAt = asString(
    source.updatedAt ?? source.updated_at ?? source.createdAt ?? source.created_at,
    nowIso(),
  );
  return {
    id: asString(source.id, makeId('conversation')),
    title: asString(source.title, 'Untitled chat'),
    updatedAt,
    messageCount: asNumber(source.messageCount ?? source.message_count, 0),
  };
}

function normalizeMessage(
  raw: MessageDto | ChatConversationMessage | Record<string, unknown>
): SnapshotMessage {
  const source = raw as Partial<MessageDto> & Partial<ChatConversationMessage> & Record<string, unknown>;
  return {
    id: asString(source.id, makeId('message')),
    role: asRole(source.role),
    content: asString(source.content),
    createdAt: asString(source.createdAt ?? source.created_at, nowIso()),
    metadata: source.metadata ?? null,
  };
}

function parseMessageSources(metadata: string | null | undefined): JournalMessageSource[] {
  if (!metadata) return [];

  try {
    const parsed = JSON.parse(metadata) as Record<string, unknown>;
    const rawSources = Array.isArray(parsed.sources)
      ? parsed.sources
      : (Array.isArray(parsed.sourceReferences)
        ? parsed.sourceReferences
        : (Array.isArray(parsed.source_references) ? parsed.source_references : []));
    if (!Array.isArray(rawSources)) return [];

    return rawSources
      .filter((value): value is Record<string, unknown> => Boolean(value) && typeof value === 'object')
      .map((source) => {
        const filePath = asString(
          source.filePath ?? source.file_path ?? source.path ?? source.url ?? source.uri
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

function collectJournalEntrySources(messages: SnapshotMessage[]): JournalMessageSource[] {
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

      if (source.score > existing.score) {
        existing.score = source.score;
      }
      if (!existing.excerpt && source.excerpt) {
        existing.excerpt = source.excerpt;
      }
      if (!existing.mimeType && source.mimeType) {
        existing.mimeType = source.mimeType;
      }
      if (!existing.category && source.category) {
        existing.category = source.category;
      }
    }
  }

  return [...byKey.values()].sort((a, b) => {
    if (a.score !== b.score) {
      return b.score - a.score;
    }
    return a.fileName.localeCompare(b.fileName);
  });
}

function getJournalMessageSourceExternalUrl(source: JournalMessageSource): string | null {
  return getSourceExternalUrl({
    filePath: source.filePath,
    path: source.documentId?.startsWith('web:') ? source.documentId.slice(4) : undefined,
    documentId: source.documentId,
    category: source.category,
    mimeType: source.mimeType,
  });
}

function getJournalSourceExternalUrl(source: JournalSourceSummary): string | null {
  return getSourceExternalUrl({
    filePath: source.filePath,
    path: source.documentId?.startsWith('web:') ? source.documentId.slice(4) : undefined,
    documentId: source.documentId,
    category: source.category,
    mimeType: source.mimeType,
  });
}

function parseStoredIdSet(raw: string | null): Set<string> {
  if (!raw) return new Set();
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((value): value is string => typeof value === 'string' && value.trim().length > 0));
  } catch {
    return new Set();
  }
}

function readStoredIdSet(storageKey: string | null): Set<string> {
  if (!storageKey) return new Set();
  try {
    return parseStoredIdSet(localStorage.getItem(storageKey));
  } catch {
    return new Set();
  }
}

function persistStoredIdSet(storageKey: string, ids: Set<string>): void {
  try {
    localStorage.setItem(storageKey, JSON.stringify([...ids]));
  } catch {
    // Ignore localStorage failures in constrained environments.
  }
}

function mapJournalPanelToTab(panel: JournalPanelQuery | null): PanelTab | null {
  if (!panel) return null;
  switch (panel) {
    case 'entries':
      return 'chats';
    case 'pages':
      return 'editor';
    case 'highlights':
      return 'annotations';
    case 'sources':
      return 'documents';
    case 'timeline':
      return 'snapshots';
    default:
      return null;
  }
}

function createSynthesisBlock(scope: JournalSynthesisScope, entryCount: number, synthesis: string): string {
  const scopeLabel =
    scope === 'current'
      ? 'Current Entry'
      : scope === 'pinned'
        ? 'Pinned Entries'
        : 'Entry Deck';
  const generatedAt = new Date().toLocaleString();

  return [
    `## Journal Synthesis · ${scopeLabel}`,
    `_Generated ${generatedAt} from ${entryCount} entr${entryCount === 1 ? 'y' : 'ies'}._`,
    '',
    synthesis.trim(),
    '',
  ].join('\n');
}

export function DailyNotesWorkspace() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const requestedJournalSpaceId = searchParams.get('journalSpaceId');
  const isJournalMode = Boolean(requestedJournalSpaceId);
  const requestedJournalPanel = searchParams.get('panel') as JournalPanelQuery | null;
  const requestedEntryId = searchParams.get('entryId');
  const [notes, setNotes] = useState<WorkspaceNote[]>([]);
  const [journalSpace, setJournalSpace] = useState<ConversationJournalDto | null>(null);
  const [journalBookmarks, setJournalBookmarks] = useState<ConversationMessageBookmarkDto[]>([]);
  const [activeNoteId, setActiveNoteId] = useState<string | null>(null);
  const [isLoadingNotes, setIsLoadingNotes] = useState(true);
  const [notesError, setNotesError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  const [documents, setDocuments] = useState<DocumentMetadata[]>([]);
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [conversationMessages, setConversationMessages] = useState<Record<string, SnapshotMessage[]>>({});
  const [conversationLoading, setConversationLoading] = useState<Record<string, boolean>>({});
  const [documentPreviews, setDocumentPreviews] = useState<Record<string, DocumentPreviewState>>({});

  const [docSearch, setDocSearch] = useState('');
  const [conversationSearch, setConversationSearch] = useState('');
  const [selectedDocumentPreviewId, setSelectedDocumentPreviewId] = useState<string | null>(null);
  const [selectedJournalSourceKey, setSelectedJournalSourceKey] = useState<string | null>(null);
  const [selectedConversationPreviewId, setSelectedConversationPreviewId] = useState<string | null>(null);
  const [newStickyText, setNewStickyText] = useState('');
  const [newStickyColor, setNewStickyColor] = useState<StickyColor>('amber');
  const [activePanel, setActivePanel] = useState<PanelTab>('editor');
  const [annotationView, setAnnotationView] = useState<AnnotationView>('highlights');
  const [documentsView, setDocumentsView] = useState<ResourceView>('list');
  const [chatsView, setChatsView] = useState<ResourceView>('list');
  const [editorView, setEditorView] = useState<EditorView>('split');
  const [actionNotice, setActionNotice] = useState<{ tone: ActionTone; message: string } | null>(null);
  const [isSavingNow, setIsSavingNow] = useState(false);
  const [hasPendingChanges, setHasPendingChanges] = useState(false);
  const [isSynthesizingEntries, setIsSynthesizingEntries] = useState(false);
  const [synthesisProgress, setSynthesisProgress] = useState<string | null>(null);
  const [isDeletingJournal, setIsDeletingJournal] = useState(false);

  const [isLoadingContext, setIsLoadingContext] = useState(true);
  const [contextError, setContextError] = useState<string | null>(null);
  const [pinnedBookmarkIds, setPinnedBookmarkIds] = useState<Set<string>>(new Set());
  const [pinnedNoteHighlightIds, setPinnedNoteHighlightIds] = useState<Set<string>>(new Set());
  const [pinnedEntryIds, setPinnedEntryIds] = useState<Set<string>>(new Set());

  const editorRef = useRef<HTMLTextAreaElement | null>(null);
  const persistTimersRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
  const dirtyNoteIdsRef = useRef<Set<string>>(new Set());
  const notesRef = useRef<WorkspaceNote[]>([]);
  const appliedRequestedEntryKeyRef = useRef<string | null>(null);
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);
  const createConversation = useConversationsStore((state) => state.createConversation);
  const addConversationWebSource = useConversationsStore(
    (state) => state.addConversationWebSource
  );
  const requestedNoteId = searchParams.get('noteId');
  const requestedSnapshotId = searchParams.get('snapshotId');
  const pinnedBookmarkStorageKey = requestedJournalSpaceId
    ? `journal.pinnedBookmarks.${requestedJournalSpaceId}`
    : null;
  const pinnedNoteHighlightStorageKey = requestedJournalSpaceId
    ? `journal.pinnedNoteHighlights.${requestedJournalSpaceId}`
    : null;
  const pinnedEntryStorageKey = requestedJournalSpaceId
    ? `journal.pinnedEntries.${requestedJournalSpaceId}`
    : null;

  const activeNote = useMemo(
    () => notes.find((note) => note.id === activeNoteId) ?? notes[0] ?? null,
    [notes, activeNoteId],
  );

  useEffect(() => {
    let cancelled = false;

    const routeToJournalNotebook = async () => {
      const journalsResult = await VaultAPI.listJournals();
      if (cancelled || !journalsResult.ok) {
        return;
      }

      const activeJournals = journalsResult.data.filter((journal) => !journal.isArchived);

      if (requestedJournalSpaceId) {
        const requestedIsActive = activeJournals.some((journal) => journal.id === requestedJournalSpaceId);
        if (requestedIsActive) {
          try {
            localStorage.setItem(LAST_JOURNAL_SPACE_KEY, requestedJournalSpaceId);
          } catch {
            // Ignore localStorage failures in constrained environments.
          }
          return;
        }

        const params = new URLSearchParams(searchParams);
        params.delete('entryId');

        if (activeJournals.length === 0) {
          params.delete('journalSpaceId');
          const next = params.toString();
          navigate(next ? `/journals?${next}` : '/journals', { replace: true });
          return;
        }

        params.set('journalSpaceId', activeJournals[0].id);
        if (!params.get('panel')) {
          params.set('panel', 'entries');
        }
        navigate(`/journals?${params.toString()}`, { replace: true });
        return;
      }

      if (activeJournals.length === 0) {
        return;
      }

      let preferredJournalId: string | null = null;
      try {
        preferredJournalId = localStorage.getItem(LAST_JOURNAL_SPACE_KEY);
      } catch {
        preferredJournalId = null;
      }

      const targetSpace =
        activeJournals.find((journal) => journal.id === preferredJournalId)
        ?? activeJournals[0];
      const params = new URLSearchParams(searchParams);
      params.set('journalSpaceId', targetSpace.id);
      if (!params.get('panel')) {
        params.set('panel', 'entries');
      }
      navigate(`/journals?${params.toString()}`, { replace: true });
    };

    void routeToJournalNotebook();

    return () => {
      cancelled = true;
    };
  }, [navigate, requestedJournalSpaceId, searchParams]);

  const clearPersistTimers = useCallback(() => {
    Object.values(persistTimersRef.current).forEach(clearTimeout);
    persistTimersRef.current = {};
  }, []);

  const persistNoteNow = useCallback(async (note: WorkspaceNote): Promise<boolean> => {
    const result = await VaultAPI.updateWorkspaceNote(note);
    if (!result.ok) {
      setSaveError(result.error);
      return false;
    }

    setSaveError(null);
    dirtyNoteIdsRef.current.delete(note.id);
    setHasPendingChanges(dirtyNoteIdsRef.current.size > 0);
    setNotes((current) =>
      current.map((item) => (item.id === result.data.id ? result.data : item)));
    return true;
  }, []);

  const scheduleNotePersist = useCallback((note: WorkspaceNote) => {
    const existing = persistTimersRef.current[note.id];
    if (existing) {
      clearTimeout(existing);
    }

    dirtyNoteIdsRef.current.add(note.id);
    setHasPendingChanges(true);

    persistTimersRef.current[note.id] = setTimeout(async () => {
      delete persistTimersRef.current[note.id];
      const latest = notesRef.current.find((item) => item.id === note.id) ?? note;
      await persistNoteNow(latest);
    }, 450);
  }, [persistNoteNow]);

  const saveAllNow = useCallback(async (showNotice = true): Promise<boolean> => {
    const dirtyIds = [...dirtyNoteIdsRef.current];
    if (dirtyIds.length === 0) {
      if (showNotice) {
        setActionNotice({ tone: 'info', message: 'No unsaved changes.' });
      }
      return true;
    }

    setIsSavingNow(true);
    clearPersistTimers();

    let failed = 0;
    for (const noteId of dirtyIds) {
      const latest = notesRef.current.find((item) => item.id === noteId);
      if (!latest) {
        dirtyNoteIdsRef.current.delete(noteId);
        continue;
      }
      const ok = await persistNoteNow(latest);
      if (!ok) {
        failed += 1;
      }
    }

    setIsSavingNow(false);
    setHasPendingChanges(dirtyNoteIdsRef.current.size > 0);

    if (showNotice) {
      if (failed > 0) {
        setActionNotice({ tone: 'error', message: `Save failed for ${failed} note${failed > 1 ? 's' : ''}.` });
      } else {
        setActionNotice({ tone: 'success', message: 'All changes saved.' });
      }
    }

    return failed === 0;
  }, [clearPersistTimers, persistNoteNow]);

  useEffect(() => {
    let cancelled = false;

    const loadNotes = async () => {
      setIsLoadingNotes(true);
      setNotesError(null);

      const result = await VaultAPI.listWorkspaceNotes();
      if (cancelled) {
        return;
      }

      if (!result.ok) {
        setNotesError(result.error);
        setIsLoadingNotes(false);
        return;
      }

      if (isJournalMode && requestedJournalSpaceId) {
        let matchedJournalSpace: ConversationJournalDto | null = null;
        let journalName = 'Journal';

        const journalsResult = await VaultAPI.listJournals();
        if (cancelled) {
          return;
        }
        if (journalsResult.ok) {
          matchedJournalSpace = journalsResult.data.find(
            (journal) => journal.id === requestedJournalSpaceId
          ) ?? null;
          if (matchedJournalSpace) {
            journalName = matchedJournalSpace.name;
          }
        }

        setJournalSpace(matchedJournalSpace);

        const storageKey = `journal.noteBySpace.${requestedJournalSpaceId}`;
        const storedNoteId = localStorage.getItem(storageKey);
        let targetNote = storedNoteId
          ? (result.data.notes.find((note) => note.id === storedNoteId) ?? null)
          : null;

        if (!targetNote && matchedJournalSpace) {
          const expectedTitle = defaultJournalTitle(matchedJournalSpace.name);
          targetNote =
            result.data.notes.find((note) => note.title.trim() === expectedTitle) ?? null;
        }

        if (!targetNote) {
          const created = await VaultAPI.createWorkspaceNote(defaultJournalTitle(journalName));
          if (cancelled) {
            return;
          }
          if (!created.ok) {
            setNotesError(created.error);
            setIsLoadingNotes(false);
            return;
          }
          targetNote = created.data;
        }

        localStorage.setItem(storageKey, targetNote.id);
        setNotes([targetNote]);
        setActiveNoteId(targetNote.id);
        setIsLoadingNotes(false);
        return;
      }

      setJournalSpace(null);
      if (result.data.notes.length > 0) {
        setNotes(result.data.notes);
        setActiveNoteId(result.data.notes[0].id);
        setIsLoadingNotes(false);
        return;
      }

      const created = await VaultAPI.createWorkspaceNote(defaultDailyTitle());
      if (cancelled) {
        return;
      }

      if (!created.ok) {
        setNotesError(created.error);
        setIsLoadingNotes(false);
        return;
      }

      setNotes([created.data]);
      setActiveNoteId(created.data.id);
      setIsLoadingNotes(false);
    };

    void loadNotes();

    return () => {
      cancelled = true;
    };
  }, [isJournalMode, requestedJournalSpaceId]);

  useEffect(() => {
    notesRef.current = notes;
  }, [notes]);

  useEffect(() => {
    if (!isJournalMode || !requestedJournalSpaceId) {
      setPinnedBookmarkIds(new Set());
      setPinnedNoteHighlightIds(new Set());
      setPinnedEntryIds(new Set());
      return;
    }

    setPinnedBookmarkIds(readStoredIdSet(pinnedBookmarkStorageKey));
    setPinnedNoteHighlightIds(readStoredIdSet(pinnedNoteHighlightStorageKey));
    setPinnedEntryIds(readStoredIdSet(pinnedEntryStorageKey));
  }, [
    isJournalMode,
    pinnedBookmarkStorageKey,
    pinnedEntryStorageKey,
    pinnedNoteHighlightStorageKey,
    requestedJournalSpaceId,
  ]);

  useEffect(() => {
    if (!isJournalMode || !pinnedBookmarkStorageKey) return;
    persistStoredIdSet(pinnedBookmarkStorageKey, pinnedBookmarkIds);
  }, [isJournalMode, pinnedBookmarkIds, pinnedBookmarkStorageKey]);

  useEffect(() => {
    if (!isJournalMode || !pinnedNoteHighlightStorageKey) return;
    persistStoredIdSet(pinnedNoteHighlightStorageKey, pinnedNoteHighlightIds);
  }, [isJournalMode, pinnedNoteHighlightIds, pinnedNoteHighlightStorageKey]);

  useEffect(() => {
    if (!isJournalMode || !pinnedEntryStorageKey) return;
    persistStoredIdSet(pinnedEntryStorageKey, pinnedEntryIds);
  }, [isJournalMode, pinnedEntryIds, pinnedEntryStorageKey]);

  useEffect(() => () => void saveAllNow(false), [saveAllNow]);

  useEffect(() => {
    const flushPendingSaves = () => {
      if (dirtyNoteIdsRef.current.size > 0) {
        void saveAllNow(false);
      }
    };

    const handleVisibilityChange = () => {
      if (document.visibilityState === 'hidden') {
        flushPendingSaves();
      }
    };

    window.addEventListener('beforeunload', flushPendingSaves);
    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      window.removeEventListener('beforeunload', flushPendingSaves);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, [saveAllNow]);

  useEffect(() => {
    if (!activeNote && notes.length > 0) {
      setActiveNoteId(notes[0].id);
    }
  }, [activeNote, notes]);

  useEffect(() => {
    if (!requestedNoteId || notes.length === 0) {
      return;
    }
    if (!notes.some((note) => note.id === requestedNoteId)) {
      return;
    }
    setActiveNoteId(requestedNoteId);
  }, [notes, requestedNoteId]);

  useEffect(() => {
    if (!requestedSnapshotId || !activeNote) {
      return;
    }
    const hasSnapshot = activeNote.conversationSnapshots.some(
      (snapshot) => snapshot.id === requestedSnapshotId
    );
    if (hasSnapshot) {
      setActivePanel('snapshots');
    }
  }, [activeNote, requestedSnapshotId]);

  useEffect(() => {
    if (!requestedSnapshotId || activePanel !== 'snapshots') {
      return;
    }
    const id = window.setTimeout(() => {
      const selector = `[data-snapshot-id="${requestedSnapshotId}"]`;
      const target = document.querySelector(selector);
      if (target instanceof HTMLElement) {
        target.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    }, 80);

    return () => window.clearTimeout(id);
  }, [
    activePanel,
    activeNote?.id,
    activeNote?.conversationSnapshots.length,
    requestedSnapshotId,
  ]);

  useEffect(() => {
    let cancelled = false;

    const loadContext = async () => {
      setIsLoadingContext(true);
      setContextError(null);

      const conversationsRequest =
        isJournalMode && requestedJournalSpaceId
          ? VaultAPI.listJournalConversations({
            journalSpaceId: requestedJournalSpaceId,
            includeArchived: true,
            limit: CONVERSATION_LIMIT,
            offset: 0,
          })
          : VaultAPI.listConversations(CONVERSATION_LIMIT, 0);

      const [docsResult, conversationsResult, bookmarksResult] = await Promise.all([
        VaultAPI.listAllDocuments(DOCUMENT_LIMIT),
        conversationsRequest,
        isJournalMode
          ? VaultAPI.listMessageBookmarks({ limit: 500, offset: 0 })
          : Promise.resolve(null),
      ]);

      if (cancelled) {
        return;
      }

      const errors: string[] = [];

      if (docsResult.ok) {
        setDocuments(docsResult.data.map((document) => normalizeDocument(document)));
      } else {
        errors.push(`documents: ${docsResult.error}`);
      }

      const parseRawConversations = (data: unknown): Array<ConversationDto | Record<string, unknown>> =>
        Array.isArray(data)
          ? data as Array<ConversationDto | Record<string, unknown>>
          : (Array.isArray((data as { conversations?: unknown })?.conversations)
            ? ((data as { conversations: Array<ConversationDto | Record<string, unknown>> }).conversations)
            : []);

      let rawConversations: Array<ConversationDto | Record<string, unknown>> = [];
      if (conversationsResult.ok) {
        rawConversations = parseRawConversations(conversationsResult.data);
      } else {
        errors.push(`conversations: ${conversationsResult.error}`);
      }

      if (isJournalMode && requestedJournalSpaceId && !conversationsResult.ok) {
        const fallbackResult = await VaultAPI.listConversationsExplorer({
          spaceId: requestedJournalSpaceId,
          includeArchived: true,
          limit: CONVERSATION_LIMIT,
          offset: 0,
        });
        if (fallbackResult.ok) {
          rawConversations = parseRawConversations(fallbackResult.data);
        } else {
          errors.push(`fallback conversations: ${fallbackResult.error}`);
        }
      }

      if (isJournalMode && requestedJournalSpaceId && !conversationsResult.ok) {
        setActionNotice({
          tone: 'info',
          message: 'Loaded entries via fallback conversation source.',
        });
      }
      const normalizedConversations = rawConversations.map((conversation) => normalizeConversation(conversation));
      setConversations(normalizedConversations);

      if (isJournalMode && requestedJournalSpaceId) {
        if (bookmarksResult?.ok) {
          const entryConversationIds = new Set(normalizedConversations.map((conversation) => conversation.id));
          setJournalBookmarks(
            bookmarksResult.data.bookmarks.filter(
              (bookmark) => entryConversationIds.has(bookmark.conversationId)
            )
          );
        } else if (bookmarksResult && !bookmarksResult.ok) {
          errors.push(`bookmarks: ${bookmarksResult.error}`);
          setJournalBookmarks([]);
        } else {
          setJournalBookmarks([]);
        }
      } else {
        setJournalBookmarks([]);
      }

      setContextError(errors.length > 0 ? `Failed loading ${errors.join(' · ')}` : null);
      setIsLoadingContext(false);
    };

    void loadContext();

    return () => {
      cancelled = true;
    };
  }, [isJournalMode, requestedJournalSpaceId]);

  const filteredDocuments = useMemo(() => {
    const query = docSearch.trim().toLowerCase();
    if (!query) {
      return documents.slice(0, 120);
    }
    return documents
      .filter((document) =>
        document.fileName.toLowerCase().includes(query)
        || document.filePath.toLowerCase().includes(query)
        || document.fileType.toLowerCase().includes(query))
      .slice(0, 120);
  }, [documents, docSearch]);

  const filteredConversations = useMemo(() => {
    const query = conversationSearch.trim().toLowerCase();
    if (!query) {
      return conversations.slice(0, 80);
    }
    return conversations
      .filter((conversation) => conversation.title.toLowerCase().includes(query))
      .slice(0, 80);
  }, [conversations, conversationSearch]);

  const journalEntryConversations = useMemo(() => {
    if (!isJournalMode) {
      return filteredConversations;
    }

    return filteredConversations
      .map((conversation, index) => ({
        conversation,
        index,
        pinned: pinnedEntryIds.has(conversation.id),
      }))
      .sort((a, b) => {
        if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
        const aTime = new Date(a.conversation.updatedAt).getTime();
        const bTime = new Date(b.conversation.updatedAt).getTime();
        if (!Number.isNaN(aTime) && !Number.isNaN(bTime) && aTime !== bTime) {
          return bTime - aTime;
        }
        return a.index - b.index;
      })
      .map((item) => item.conversation);
  }, [filteredConversations, isJournalMode, pinnedEntryIds]);

  const journalSourceConversations = useMemo(() => {
    if (!isJournalMode) {
      return [];
    }

    return conversations
      .map((conversation, index) => ({
        conversation,
        index,
        pinned: pinnedEntryIds.has(conversation.id),
      }))
      .sort((a, b) => {
        if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
        const aTime = new Date(a.conversation.updatedAt).getTime();
        const bTime = new Date(b.conversation.updatedAt).getTime();
        if (!Number.isNaN(aTime) && !Number.isNaN(bTime) && aTime !== bTime) {
          return bTime - aTime;
        }
        return a.index - b.index;
      })
      .map((item) => item.conversation)
      .slice(0, JOURNAL_SOURCE_SCAN_LIMIT);
  }, [conversations, isJournalMode, pinnedEntryIds]);

  const journalSourceSummaries = useMemo(() => {
    if (!isJournalMode || journalSourceConversations.length === 0) {
      return [];
    }

    const byKey = new Map<string, JournalSourceSummary>();
    for (const conversation of journalSourceConversations) {
      const messages = conversationMessages[conversation.id];
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
          if (source.score > existing.score) {
            existing.score = source.score;
          }
          if (!existing.excerpt && source.excerpt) {
            existing.excerpt = source.excerpt;
          }
          if (!existing.documentId && source.documentId) {
            existing.documentId = source.documentId;
          }
          if (!existing.mimeType && source.mimeType) {
            existing.mimeType = source.mimeType;
          }
          if (!existing.category && source.category) {
            existing.category = source.category;
          }
          if (!existing.conversationIds.includes(conversation.id)) {
            existing.conversationIds.push(conversation.id);
          }
          if (!existing.conversationTitles.includes(conversation.title)) {
            existing.conversationTitles.push(conversation.title);
          }
        }
      }
    }

    return [...byKey.values()].sort((a, b) => {
      if (a.referenceCount !== b.referenceCount) {
        return b.referenceCount - a.referenceCount;
      }
      if (a.score !== b.score) {
        return b.score - a.score;
      }
      return a.fileName.localeCompare(b.fileName);
    });
  }, [conversationMessages, isJournalMode, journalSourceConversations]);

  const filteredJournalSources = useMemo(() => {
    const query = docSearch.trim().toLowerCase();
    if (!query) {
      return journalSourceSummaries;
    }

    return journalSourceSummaries.filter((source) =>
      source.fileName.toLowerCase().includes(query)
      || source.filePath.toLowerCase().includes(query)
      || source.category.toLowerCase().includes(query)
      || source.mimeType.toLowerCase().includes(query)
      || source.excerpt.toLowerCase().includes(query)
      || source.conversationTitles.some((title) => title.toLowerCase().includes(query)));
  }, [docSearch, journalSourceSummaries]);

  const selectedJournalSource = useMemo(
    () =>
      filteredJournalSources.find((source) => source.key === selectedJournalSourceKey)
      ?? filteredJournalSources[0]
      ?? null,
    [filteredJournalSources, selectedJournalSourceKey],
  );

  const journalSourcesLoading = useMemo(() => {
    if (!isJournalMode || activePanel !== 'documents' || journalSourceConversations.length === 0) {
      return false;
    }
    return journalSourceConversations.some(
      (conversation) =>
        conversationLoading[conversation.id] || conversationMessages[conversation.id] === undefined,
    );
  }, [activePanel, conversationLoading, conversationMessages, isJournalMode, journalSourceConversations]);

  const selectedDocument = useMemo(
    () => documents.find((document) => document.id === selectedDocumentPreviewId) ?? null,
    [documents, selectedDocumentPreviewId],
  );

  const selectedDocumentPreview = selectedDocumentPreviewId
    ? documentPreviews[selectedDocumentPreviewId]
    : undefined;

  const selectedConversationMessages = selectedConversationPreviewId
    ? conversationMessages[selectedConversationPreviewId] ?? []
    : [];
  const selectedJournalConversation = selectedConversationPreviewId
    ? journalEntryConversations.find((conversation) => conversation.id === selectedConversationPreviewId) ?? null
    : null;

  useEffect(() => {
    if (selectedDocumentPreviewId) {
      setDocumentsView('preview');
    }
  }, [selectedDocumentPreviewId]);

  useEffect(() => {
    if (selectedConversationPreviewId) {
      setChatsView('preview');
    }
  }, [selectedConversationPreviewId]);

  useEffect(() => {
    if (!isJournalMode) return;
    setChatsView('preview');
    if (editorView !== 'preview') {
      setEditorView('preview');
    }
    const requestedTab = mapJournalPanelToTab(requestedJournalPanel);
    setActivePanel(requestedTab ?? 'chats');
  }, [editorView, isJournalMode, requestedJournalPanel]);

  useEffect(() => {
    if (!isJournalMode) return;

    if (journalEntryConversations.length === 0) {
      setSelectedConversationPreviewId(null);
      return;
    }

    const requestedEntryKey = requestedEntryId && requestedJournalSpaceId
      ? `${requestedJournalSpaceId}:${requestedEntryId}`
      : null;

    // Apply URL-requested entry once on initial open. After that, allow manual paging/selection.
    if (requestedEntryId && requestedEntryKey && appliedRequestedEntryKeyRef.current !== requestedEntryKey) {
      const requested = journalEntryConversations.find((conversation) => conversation.id === requestedEntryId);
      if (requested) {
        appliedRequestedEntryKeyRef.current = requestedEntryKey;
        if (selectedConversationPreviewId !== requested.id) {
          setSelectedConversationPreviewId(requested.id);
        }
        return;
      }
    }

    const hasSelected = selectedConversationPreviewId
      ? journalEntryConversations.some((conversation) => conversation.id === selectedConversationPreviewId)
      : false;

    if (!hasSelected) {
      setSelectedConversationPreviewId(journalEntryConversations[0].id);
    }
  }, [
    isJournalMode,
    journalEntryConversations,
    requestedEntryId,
    requestedJournalSpaceId,
    selectedConversationPreviewId,
  ]);

  useEffect(() => {
    if (!isJournalMode) {
      if (selectedJournalSourceKey !== null) {
        setSelectedJournalSourceKey(null);
      }
      return;
    }

    if (filteredJournalSources.length === 0) {
      if (selectedJournalSourceKey !== null) {
        setSelectedJournalSourceKey(null);
      }
      return;
    }

    const hasSelected = selectedJournalSourceKey
      ? filteredJournalSources.some((source) => source.key === selectedJournalSourceKey)
      : false;

    if (!hasSelected) {
      setSelectedJournalSourceKey(filteredJournalSources[0].key);
    }
  }, [filteredJournalSources, isJournalMode, selectedJournalSourceKey]);

  const updateNote = useCallback((noteId: string, updater: (note: WorkspaceNote) => WorkspaceNote) => {
    const existing = notesRef.current.find((note) => note.id === noteId);
    if (!existing) {
      return;
    }

    const updatedNote = { ...updater(existing), updatedAt: nowIso() };
    setNotes((current) => {
      const next = current.map((note) => (note.id === noteId ? updatedNote : note));
      notesRef.current = next;
      return next;
    });

    scheduleNotePersist(updatedNote);
  }, [scheduleNotePersist]);

  const updateActiveNote = useCallback(
    (updater: (note: WorkspaceNote) => WorkspaceNote) => {
      if (!activeNote) {
        return;
      }
      updateNote(activeNote.id, updater);
    },
    [activeNote, updateNote],
  );

  const ensureConversationMessages = useCallback(
    async (conversationId: string): Promise<SnapshotMessage[]> => {
      if (conversationMessages[conversationId]) {
        return conversationMessages[conversationId];
      }

      setConversationLoading((current) => ({ ...current, [conversationId]: true }));
      const result = await VaultAPI.getConversationMessages(conversationId);
      if (!result.ok) {
        setSaveError(result.error);
      }

      const messages = result.ok
        ? result.data.messages.map((message) => normalizeMessage(message))
        : [];

      setConversationMessages((current) => ({ ...current, [conversationId]: messages }));
      setConversationLoading((current) => ({ ...current, [conversationId]: false }));

      return messages;
    },
    [conversationMessages],
  );

  useEffect(() => {
    if (!isJournalMode || !selectedConversationPreviewId) return;
    void ensureConversationMessages(selectedConversationPreviewId);
  }, [ensureConversationMessages, isJournalMode, selectedConversationPreviewId]);

  useEffect(() => {
    if (!isJournalMode || activePanel !== 'documents' || journalSourceConversations.length === 0) {
      return;
    }

    const conversationsToLoad = journalSourceConversations.filter(
      (conversation) =>
        conversationMessages[conversation.id] === undefined && !conversationLoading[conversation.id],
    );
    if (conversationsToLoad.length === 0) {
      return;
    }

    void Promise.all(conversationsToLoad.map((conversation) => ensureConversationMessages(conversation.id)));
  }, [
    activePanel,
    conversationLoading,
    conversationMessages,
    ensureConversationMessages,
    isJournalMode,
    journalSourceConversations,
  ]);

  const loadDocumentPreview = useCallback(async (documentId: string) => {
    const document = documents.find((item) => item.id === documentId);
    if (!document || documentPreviews[documentId]) {
      return;
    }

    setDocumentPreviews((current) => ({
      ...current,
      [documentId]: { loading: true, content: '', error: null },
    }));

    const result = await VaultAPI.readFileContent(document.filePath);
    if (result.ok) {
      setDocumentPreviews((current) => ({
        ...current,
        [documentId]: {
          loading: false,
          content: result.data.slice(0, PREVIEW_CHAR_LIMIT),
          error: null,
        },
      }));
      return;
    }

    setDocumentPreviews((current) => ({
      ...current,
      [documentId]: {
        loading: false,
        content: '',
        error: result.error,
      },
    }));
  }, [documents, documentPreviews]);

  const createNewNote = async () => {
    const result = await VaultAPI.createWorkspaceNote(`Note ${notes.length + 1}`);
    if (!result.ok) {
      setSaveError(result.error);
      return;
    }

    setSaveError(null);
    setNotes((current) => [result.data, ...current]);
    setActiveNoteId(result.data.id);
  };

  const deleteActiveNote = async () => {
    if (!activeNote) {
      return;
    }

    const deletingId = activeNote.id;
    const deleteResult = await VaultAPI.deleteWorkspaceNote(deletingId);
    if (!deleteResult.ok) {
      setSaveError(deleteResult.error);
      return;
    }

    setSaveError(null);
    const remaining = notes.filter((note) => note.id !== activeNote.id);
    if (remaining.length === 0) {
      const createResult = await VaultAPI.createWorkspaceNote(defaultDailyTitle());
      if (!createResult.ok) {
        setSaveError(createResult.error);
        setNotes([]);
        setActiveNoteId(null);
        return;
      }
      setNotes([createResult.data]);
      setActiveNoteId(createResult.data.id);
      return;
    }

    setNotes(remaining);
    setActiveNoteId(remaining[0].id);

    const pendingTimer = persistTimersRef.current[deletingId];
    if (pendingTimer) {
      clearTimeout(pendingTimer);
      delete persistTimersRef.current[deletingId];
    }
    dirtyNoteIdsRef.current.delete(deletingId);
    setHasPendingChanges(dirtyNoteIdsRef.current.size > 0);
  };

  const deleteCurrentJournal = async () => {
    if (!requestedJournalSpaceId || !journalSpace || isDeletingJournal) {
      return;
    }

    const confirmed = window.confirm(
      `Delete journal "${journalSpace.name}"?\n\nThis permanently deletes the journal and cannot be undone.`
    );
    if (!confirmed) {
      return;
    }

    setIsDeletingJournal(true);
    try {
      const result = await VaultAPI.deleteJournal({
        journalId: requestedJournalSpaceId,
      });
      if (!result.ok) {
        setActionNotice({ tone: 'error', message: result.error });
        return;
      }

      try {
        localStorage.removeItem(`journal.noteBySpace.${requestedJournalSpaceId}`);
        localStorage.removeItem(`journal.pinnedBookmarks.${requestedJournalSpaceId}`);
        localStorage.removeItem(`journal.pinnedNoteHighlights.${requestedJournalSpaceId}`);
        localStorage.removeItem(`journal.pinnedEntries.${requestedJournalSpaceId}`);
        const lastJournalSpaceId = localStorage.getItem(LAST_JOURNAL_SPACE_KEY);
        if (lastJournalSpaceId === requestedJournalSpaceId) {
          localStorage.removeItem(LAST_JOURNAL_SPACE_KEY);
        }
      } catch {
        // Ignore localStorage failures in constrained environments.
      }

      const journalsResult = await VaultAPI.listJournals();
      if (!journalsResult.ok) {
        navigate('/journals', { replace: true });
        return;
      }

      const remainingActiveJournals = journalsResult.data.filter(
        (journal) => !journal.isArchived
      );
      const params = new URLSearchParams(searchParams);
      params.delete('entryId');

      if (remainingActiveJournals.length === 0) {
        params.delete('journalSpaceId');
      } else {
        params.set('journalSpaceId', remainingActiveJournals[0].id);
        if (!params.get('panel')) {
          params.set('panel', 'entries');
        }
      }

      const next = params.toString();
      navigate(next ? `/journals?${next}` : '/journals', { replace: true });
      setActionNotice({
        tone: 'success',
        message: `Deleted journal "${journalSpace.name}".`,
      });
    } finally {
      setIsDeletingJournal(false);
    }
  };

  const toggleLinkedDocument = (documentId: string) => {
    updateActiveNote((note) => {
      const exists = note.linkedDocumentIds.includes(documentId);
      return {
        ...note,
        linkedDocumentIds: exists
          ? note.linkedDocumentIds.filter((id) => id !== documentId)
          : [...note.linkedDocumentIds, documentId],
      };
    });
  };

  const toggleLinkedConversation = (conversationId: string) => {
    updateActiveNote((note) => {
      const exists = note.linkedConversationIds.includes(conversationId);
      return {
        ...note,
        linkedConversationIds: exists
          ? note.linkedConversationIds.filter((id) => id !== conversationId)
          : [...note.linkedConversationIds, conversationId],
      };
    });
  };

  const captureConversation = async (conversationId: string) => {
    if (!activeNote) {
      setActionNotice({ tone: 'error', message: 'Select a note before capturing chat history.' });
      return;
    }

    const messages = await ensureConversationMessages(conversationId);
    const conversation = conversations.find((item) => item.id === conversationId);
    const snapshot: ConversationSnapshot = {
      id: makeId('snapshot'),
      conversationId,
      conversationTitle: conversation?.title ?? 'Conversation Snapshot',
      capturedAt: nowIso(),
      messageCount: messages.length,
      messages: messages.map((message) => ({ ...message })),
    };

    updateActiveNote((note) => ({
      ...note,
      linkedConversationIds: unique([...note.linkedConversationIds, conversationId]),
      conversationSnapshots: [snapshot, ...note.conversationSnapshots],
    }));
    setActionNotice({
      tone: 'success',
      message: `Captured ${snapshot.messageCount} messages from "${snapshot.conversationTitle}".`,
    });
  };

  const captureActiveConversation = async () => {
    const targetConversationId =
      activeConversationId ?? selectedConversationPreviewId ?? conversations[0]?.id ?? null;
    if (!targetConversationId) {
      setActionNotice({ tone: 'error', message: 'No conversation available to capture yet.' });
      return;
    }
    await captureConversation(targetConversationId);
  };

  const focusJournalEntry = useCallback(async (conversationId: string) => {
    setActivePanel('chats');
    setSelectedConversationPreviewId(conversationId);
    await ensureConversationMessages(conversationId);
  }, [ensureConversationMessages]);

  const insertSnapshotIntoNote = (snapshot: ConversationSnapshot) => {
    const transcript = snapshot.messages
      .map(
        (message) =>
          `### ${message.role.toUpperCase()} · ${formatWhen(message.createdAt)}\n\n${message.content}`,
      )
      .join('\n\n');

    const block = [
      '',
      `## Snapshot: ${snapshot.conversationTitle}`,
      `Captured: ${formatWhen(snapshot.capturedAt)}`,
      '',
      transcript || '_No messages captured_',
      '',
    ].join('\n');

    updateActiveNote((note) => ({
      ...note,
      content: note.content ? `${note.content}\n${block}` : block.trim(),
    }));
  };

  const addHighlightFromSelection = () => {
    if (!activeNote || !editorRef.current) {
      if (!activeNote) {
        setActionNotice({ tone: 'error', message: 'Select a note first.' });
      } else {
        setEditorView('edit');
        setActivePanel('editor');
        setActionNotice({ tone: 'info', message: 'Switched to edit mode. Select text, then click Highlight again.' });
      }
      return;
    }

    const textarea = editorRef.current;
    const { selectionStart, selectionEnd } = textarea;
    if (selectionEnd <= selectionStart) {
      setActionNotice({ tone: 'info', message: 'Select some text in the editor first.' });
      return;
    }

    const selectedText = activeNote.content.slice(selectionStart, selectionEnd).trim();
    if (!selectedText) {
      setActionNotice({ tone: 'info', message: 'Select non-empty text in the editor first.' });
      return;
    }

    const wrapped = `${activeNote.content.slice(0, selectionStart)}==${activeNote.content.slice(selectionStart, selectionEnd)}==${activeNote.content.slice(selectionEnd)}`;
    const caret = selectionEnd + 4;

    updateActiveNote((note) => ({
      ...note,
      content: wrapped,
      highlights: [
        {
          id: makeId('highlight'),
          text: selectedText.slice(0, HIGHLIGHT_CHAR_LIMIT),
          createdAt: nowIso(),
        },
        ...note.highlights,
      ],
    }));

    requestAnimationFrame(() => {
      textarea.focus();
      textarea.setSelectionRange(caret, caret);
    });
    setActionNotice({ tone: 'success', message: 'Highlight added.' });
  };

  const removeHighlight = (highlightId: string) => {
    updateActiveNote((note) => ({
      ...note,
      highlights: note.highlights.filter((highlight) => highlight.id !== highlightId),
    }));
  };

  const addStickyNote = () => {
    const text = newStickyText.trim();
    if (!text) {
      return;
    }
    updateActiveNote((note) => ({
      ...note,
      stickyNotes: [
        {
          id: makeId('sticky'),
          text,
          color: newStickyColor,
          createdAt: nowIso(),
        },
        ...note.stickyNotes,
      ],
    }));
    setNewStickyText('');
  };

  const updateStickyText = (stickyId: string, text: string) => {
    updateActiveNote((note) => ({
      ...note,
      stickyNotes: note.stickyNotes.map((sticky) =>
        sticky.id === stickyId ? { ...sticky, text } : sticky),
    }));
  };

  const removeSticky = (stickyId: string) => {
    updateActiveNote((note) => ({
      ...note,
      stickyNotes: note.stickyNotes.filter((sticky) => sticky.id !== stickyId),
    }));
  };

  const selectDocumentForPreview = async (documentId: string) => {
    setSelectedDocumentPreviewId(documentId);
    setDocumentsView('preview');
    await loadDocumentPreview(documentId);
  };

  const selectConversationForPreview = async (conversationId: string) => {
    setSelectedConversationPreviewId(conversationId);
    setChatsView('preview');
    await ensureConversationMessages(conversationId);
  };

  const openBookmarkInEntry = async (bookmark: ConversationMessageBookmarkDto) => {
    await focusJournalEntry(bookmark.conversationId);
  };

  useEffect(() => {
    if (!isJournalMode) return;
    const validIds = new Set(journalBookmarks.map((bookmark) => bookmark.id));
    setPinnedBookmarkIds((current) => {
      const next = new Set([...current].filter((id) => validIds.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [isJournalMode, journalBookmarks]);

  useEffect(() => {
    if (!isJournalMode || !activeNote) return;
    const validIds = new Set(activeNote.highlights.map((highlight) => highlight.id));
    setPinnedNoteHighlightIds((current) => {
      const next = new Set([...current].filter((id) => validIds.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [activeNote, isJournalMode]);

  useEffect(() => {
    if (!isJournalMode) return;
    const validIds = new Set(journalEntryConversations.map((conversation) => conversation.id));
    setPinnedEntryIds((current) => {
      const next = new Set([...current].filter((id) => validIds.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [isJournalMode, journalEntryConversations]);

  const togglePinnedBookmark = (bookmarkId: string) => {
    setPinnedBookmarkIds((current) => {
      const next = new Set(current);
      if (next.has(bookmarkId)) {
        next.delete(bookmarkId);
      } else {
        next.add(bookmarkId);
      }
      return next;
    });
  };

  const togglePinnedNoteHighlight = (highlightId: string) => {
    setPinnedNoteHighlightIds((current) => {
      const next = new Set(current);
      if (next.has(highlightId)) {
        next.delete(highlightId);
      } else {
        next.add(highlightId);
      }
      return next;
    });
  };

  const togglePinnedEntry = (conversationId: string) => {
    setPinnedEntryIds((current) => {
      const next = new Set(current);
      if (next.has(conversationId)) {
        next.delete(conversationId);
      } else {
        next.add(conversationId);
      }
      return next;
    });
  };

  const openJournalSource = useCallback(async (source: JournalSourceSummary) => {
    const rawPath = source.filePath.trim();
    const sourceUrl = getJournalSourceExternalUrl(source);
    const pathFromFileUri = rawPath.startsWith('file://')
      ? decodeURIComponent(rawPath.replace(/^file:\/\//, ''))
      : rawPath;
    const isWebUrl = Boolean(sourceUrl);

    if (isWebUrl) {
      try {
        await openExternal(sourceUrl!);
      } catch {
        const opened = window.open(sourceUrl!, '_blank', 'noopener,noreferrer');
        if (!opened) {
          setActionNotice({ tone: 'error', message: 'Unable to open source URL in browser.' });
        }
      }
      return;
    }

    if (source.documentId && UUID_PATTERN.test(source.documentId)) {
      const byId = await VaultAPI.openFileById(source.documentId);
      if (byId.ok) {
        if (byId.data.action === 'render_internal' && byId.data.contentPath) {
          const internalOpen = await VaultAPI.openFile(byId.data.contentPath);
          if (!internalOpen.ok) {
            setActionNotice({ tone: 'error', message: `Unable to open source: ${internalOpen.error}` });
          }
        }
        return;
      }

      const metadataResult = await VaultAPI.getDocument(source.documentId);
      if (metadataResult.ok && metadataResult.data.filePath) {
        const byMetadataPath = await VaultAPI.openFile(metadataResult.data.filePath);
        if (byMetadataPath.ok) {
          return;
        }
      }
    }

    if (!pathFromFileUri || pathFromFileUri === 'unknown://source') {
      setActionNotice({ tone: 'error', message: 'Source path unavailable for this citation.' });
      return;
    }

    const byPath = await VaultAPI.openFile(pathFromFileUri);
    if (!byPath.ok) {
      setActionNotice({ tone: 'error', message: `Unable to open source: ${byPath.error}` });
    }
  }, []);

  const startNewChatFromEntrySources = useCallback(async (conversation: ConversationSummary) => {
    const entryMessages = await ensureConversationMessages(conversation.id);
    const sources = collectJournalEntrySources(entryMessages).slice(0, 24);
    if (sources.length === 0) {
      setActionNotice({
        tone: 'error',
        message: 'No citation sources found for this journal entry yet.',
      });
      return;
    }

    try {
      const titleBase = conversation.title.trim() || 'Journal Entry';
      const conversationTitle = `Sources · ${titleBase}`.slice(0, 90);
      const newConversationId = await createConversation(conversationTitle);

      const webSources = sources
        .map((source) => ({
          source,
          url: getJournalMessageSourceExternalUrl(source),
        }))
        .filter((item): item is { source: JournalMessageSource; url: string } => Boolean(item.url));

      const linkedResults = await Promise.all(
        webSources.map(({ source, url }) =>
          addConversationWebSource(newConversationId, url, {
            title: source.fileName,
            excerpt: source.excerpt || undefined,
            relevanceScore: source.score,
          })
        )
      );
      const linkedCount = linkedResults.filter(Boolean).length;
      if (linkedCount === 0) {
        setActionNotice({
          tone: 'info',
          message: 'Opened new chat. No web links were attached from this journal entry.',
        });
      }

      navigate(`/chat?conversationId=${encodeURIComponent(newConversationId)}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to start new chat from sources.';
      setActionNotice({ tone: 'error', message });
    }
  }, [addConversationWebSource, createConversation, ensureConversationMessages, navigate]);

  const selectSynthesisTargets = useCallback((scope: JournalSynthesisScope): ConversationSummary[] => {
    if (scope === 'current') {
      const currentId = selectedConversationPreviewId ?? journalEntryConversations[0]?.id ?? null;
      if (!currentId) return [];
      return journalEntryConversations.filter((conversation) => conversation.id === currentId);
    }

    if (scope === 'pinned') {
      const pinned = journalEntryConversations.filter((conversation) => pinnedEntryIds.has(conversation.id));
      return pinned.slice(0, SYNTHESIS_ENTRY_LIMIT);
    }

    return journalEntryConversations.slice(0, SYNTHESIS_ENTRY_LIMIT);
  }, [journalEntryConversations, pinnedEntryIds, selectedConversationPreviewId]);

  const synthesizeJournalEntries = useCallback(async (scope: JournalSynthesisScope) => {
    if (!isJournalMode) {
      setActionNotice({ tone: 'error', message: 'Synthesis is available in journal mode only.' });
      return;
    }
    if (!activeNote) {
      setActionNotice({ tone: 'error', message: 'Select a notebook page before synthesizing.' });
      return;
    }

    const targets = selectSynthesisTargets(scope);
    if (targets.length === 0) {
      setActionNotice({ tone: 'error', message: 'No journal entries available for synthesis.' });
      return;
    }

    setIsSynthesizingEntries(true);
    setSynthesisProgress(`Synthesizing ${targets.length} entr${targets.length === 1 ? 'y' : 'ies'}...`);

    try {
      const result = await VaultAPI.synthesizeJournalEntries({
        conversationIds: targets.map((target) => target.id),
        scope,
        maxEntries: SYNTHESIS_ENTRY_LIMIT,
      });
      if (!result.ok) {
        throw new Error(result.error);
      }

      const synthesisBlock = createSynthesisBlock(scope, result.data.entryCount, result.data.synthesis);
      updateActiveNote((note) => ({
        ...note,
        content: note.content.trim()
          ? `${note.content.trim()}\n\n${synthesisBlock}`
          : synthesisBlock,
        linkedConversationIds: unique([
          ...note.linkedConversationIds,
          ...result.data.conversationIds,
        ]),
      }));

      setActivePanel('editor');
      setEditorView('preview');
      setActionNotice({
        tone: 'success',
        message: `Synthesis complete for ${result.data.entryCount} entr${result.data.entryCount === 1 ? 'y' : 'ies'}.`,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Synthesis failed.';
      setActionNotice({ tone: 'error', message });
    } finally {
      setIsSynthesizingEntries(false);
      setSynthesisProgress(null);
    }
  }, [
    activeNote,
    isJournalMode,
    selectSynthesisTargets,
    updateActiveNote,
  ]);

  const linkedDocuments = activeNote
    ? documents.filter((document) => activeNote.linkedDocumentIds.includes(document.id))
    : [];
  const pinnedJournalBookmarks = journalBookmarks.filter((bookmark) => pinnedBookmarkIds.has(bookmark.id));
  const unpinnedJournalBookmarks = journalBookmarks.filter((bookmark) => !pinnedBookmarkIds.has(bookmark.id));
  const noteHighlights = activeNote?.highlights ?? [];
  const pinnedNoteHighlights = noteHighlights.filter((highlight) => pinnedNoteHighlightIds.has(highlight.id));
  const unpinnedNoteHighlights = noteHighlights.filter((highlight) => !pinnedNoteHighlightIds.has(highlight.id));
  const journalSourcesCount = journalSourceSummaries.length;

  const notebookTitle = isJournalMode
    ? `${journalSpace?.name ?? 'Journal'} Notebook`
    : 'Journals';
  const notebookSubtitle = isJournalMode
    ? 'A notebook view for this journal: pages, entries, highlights, and sources.'
    : 'A journals workspace for pages, highlights, and references. Pages are listed on the left.';
  const noteTitlePlaceholder = isJournalMode ? 'Notebook title' : 'Page title';
  const tabLabels: Record<PanelTab, string> = isJournalMode
    ? {
      editor: 'Pages',
      annotations: 'Highlights',
      documents: 'Sources',
      chats: 'Entries',
      snapshots: 'Timeline',
    }
    : {
      editor: 'Page',
      annotations: 'Highlights',
      documents: 'Sources',
      chats: 'Chats',
      snapshots: 'Snapshots',
    };
  const previewConversations = isJournalMode ? journalEntryConversations : filteredConversations;
  const currentPreviewIndex = selectedConversationPreviewId
    ? previewConversations.findIndex((conversation) => conversation.id === selectedConversationPreviewId)
    : -1;
  const canGoPrevPreview = currentPreviewIndex > 0;
  const canGoNextPreview = currentPreviewIndex >= 0 && currentPreviewIndex < previewConversations.length - 1;

  const goToAdjacentPreviewConversation = async (direction: -1 | 1) => {
    if (currentPreviewIndex < 0) return;
    const nextIndex = currentPreviewIndex + direction;
    if (nextIndex < 0 || nextIndex >= previewConversations.length) return;
    await selectConversationForPreview(previewConversations[nextIndex].id);
  };

  const tabButtonClass = (tab: PanelTab): string =>
    `px-3.5 py-1.5 rounded-full text-xs border transition-all ${
      activePanel === tab
        ? 'border-cyan-300/65 bg-cyan-500/20 text-cyan-100 shadow-[0_0_0_1px_rgba(34,211,238,0.25)]'
        : 'border-white/15 bg-white/[0.04] text-white/70 hover:bg-white/10'
    }`;
  const subviewButtonClass = (active: boolean): string =>
    `px-2.5 py-1 rounded-full text-[11px] border transition-all ${
      active
        ? 'border-cyan-300/60 bg-cyan-500/20 text-cyan-100 shadow-[0_0_0_1px_rgba(34,211,238,0.25)]'
        : 'border-white/15 bg-white/[0.04] text-white/65 hover:bg-white/10'
    }`;

  return (
    <div className="h-full overflow-hidden bg-[radial-gradient(circle_at_5%_0%,rgba(16,185,129,0.16),transparent_35%),radial-gradient(circle_at_90%_8%,rgba(56,189,248,0.14),transparent_42%),linear-gradient(180deg,rgba(2,10,24,0.98),rgba(2,6,18,1))] text-[var(--text-primary)]">
      <div className="h-full grid grid-cols-1 xl:grid-cols-[20rem_minmax(0,1fr)]">
        <aside className="border-b xl:border-b-0 xl:border-r border-white/10 bg-black/25 backdrop-blur-md flex flex-col min-h-0">
          <div className="p-4 border-b border-white/10">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-cyan-400/40 to-amber-300/40 flex items-center justify-center">
                  <BookOpen className="w-4 h-4 text-cyan-100" />
                </div>
                <div>
                  <h1 className="text-sm font-semibold tracking-wide uppercase text-white/80">{notebookTitle}</h1>
                  <p className="text-xs text-white/50">{notebookSubtitle}</p>
                </div>
              </div>
              {!isJournalMode && (
                <button
                  onClick={createNewNote}
                  className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-md bg-cyan-500/20 hover:bg-cyan-500/30 border border-cyan-400/30 text-cyan-100 text-xs transition-colors"
                  title="Create page"
                >
                  <Plus className="w-3.5 h-3.5" />
                  New Page
                </button>
              )}
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-2 space-y-2">
            {isLoadingNotes && (
              <p className="px-2 py-1 text-xs text-white/45">Loading notes…</p>
            )}
            {notesError && (
              <p className="px-2 py-1 text-xs text-rose-300">{notesError}</p>
            )}
            {isJournalMode ? (
              <>
                {activeNote && (
                  <div className="rounded-lg border border-cyan-400/35 bg-cyan-500/12 p-3">
                    <p className="text-[11px] uppercase tracking-wide text-cyan-100/70">Notebook Page</p>
                    <p className="mt-1 text-sm font-medium text-white/90 line-clamp-2">{activeNote.title}</p>
                    <p className="mt-1 text-[11px] text-white/55">Updated {formatWhen(activeNote.updatedAt)}</p>
                  </div>
                )}

                <div className="rounded-lg border border-white/10 bg-white/5 p-2.5">
                  <p className="text-[11px] uppercase tracking-wide text-white/55">Notebook Sections</p>
                  <div className="mt-2 grid grid-cols-2 gap-1.5">
                    <button
                      onClick={() => setActivePanel('editor')}
                      className={tabButtonClass('editor')}
                    >
                      {tabLabels.editor}
                    </button>
                    <button
                      onClick={() => setActivePanel('chats')}
                      className={tabButtonClass('chats')}
                    >
                      {tabLabels.chats}
                    </button>
                    <button
                      onClick={() => setActivePanel('annotations')}
                      className={tabButtonClass('annotations')}
                    >
                      {tabLabels.annotations}
                    </button>
                    <button
                      onClick={() => setActivePanel('documents')}
                      className={tabButtonClass('documents')}
                    >
                      {tabLabels.documents}
                    </button>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-1.5">
                  <div className="rounded-md border border-white/10 bg-white/5 p-2">
                    <p className="text-[10px] uppercase tracking-wide text-white/45">Entries</p>
                    <p className="mt-1 text-sm font-semibold text-white/85">{journalEntryConversations.length}</p>
                  </div>
                  <div className="rounded-md border border-white/10 bg-white/5 p-2">
                    <p className="text-[10px] uppercase tracking-wide text-white/45">Pinned</p>
                    <p className="mt-1 text-sm font-semibold text-white/85">
                      {pinnedEntryIds.size + pinnedBookmarkIds.size + pinnedNoteHighlightIds.size}
                    </p>
                  </div>
                  <div className="rounded-md border border-white/10 bg-white/5 p-2">
                    <p className="text-[10px] uppercase tracking-wide text-white/45">Highlights</p>
                    <p className="mt-1 text-sm font-semibold text-white/85">
                      {journalBookmarks.length + noteHighlights.length}
                    </p>
                  </div>
                  <div className="rounded-md border border-white/10 bg-white/5 p-2">
                    <p className="text-[10px] uppercase tracking-wide text-white/45">Sources</p>
                    <p className="mt-1 text-sm font-semibold text-white/85">
                      {isJournalMode ? journalSourcesCount : linkedDocuments.length}
                    </p>
                  </div>
                </div>

                <button
                  onClick={() => navigate('/references')}
                  className="w-full inline-flex items-center justify-center gap-1.5 rounded-md border border-white/15 bg-white/5 px-2.5 py-2 text-[11px] text-white/75 transition-colors hover:border-white/30 hover:text-white"
                >
                  <Link2 className="w-3.5 h-3.5" />
                  Open Reference Inbox
                </button>
              </>
            ) : (
              <>
                <p className="px-2 py-1 text-[11px] uppercase tracking-wide text-white/45">Pages</p>
                {notes.map((note) => {
                  const isActive = activeNote?.id === note.id;
                  return (
                    <button
                      key={note.id}
                      onClick={() => setActiveNoteId(note.id)}
                      className={`w-full text-left p-3 rounded-lg border transition-all ${
                        isActive
                          ? 'border-cyan-400/60 bg-cyan-500/15 shadow-lg shadow-cyan-900/20'
                          : 'border-white/10 bg-white/5 hover:bg-white/10'
                      }`}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <span className="font-medium text-sm text-white/90 line-clamp-2">{note.title}</span>
                        <NotebookPen className="w-3.5 h-3.5 mt-0.5 text-white/40" />
                      </div>
                      <p className="text-xs text-white/50 mt-1">{formatWhen(note.updatedAt)}</p>
                    </button>
                  );
                })}
              </>
            )}
          </div>

          {!isJournalMode && (
            <div className="p-3 border-t border-white/10">
              <button
                onClick={deleteActiveNote}
                disabled={!activeNote}
                className="w-full inline-flex items-center justify-center gap-2 px-3 py-2 rounded-md bg-rose-500/15 hover:bg-rose-500/25 border border-rose-300/25 text-rose-100 text-sm disabled:opacity-40"
              >
                <Trash2 className="w-4 h-4" />
                Delete Page
              </button>
            </div>
          )}
        </aside>

        <main className="min-h-0 flex flex-col">
          <div className="p-4 md:p-5 border-b border-white/10 bg-black/15 backdrop-blur-sm">
            <div className="flex flex-col md:flex-row md:items-center gap-3 md:gap-4">
              <input
                type="text"
                value={activeNote?.title ?? ''}
                onChange={(event) => updateActiveNote((note) => ({ ...note, title: event.target.value }))}
                className="flex-1 bg-white/[0.04] border border-white/10 focus:border-cyan-400/70 focus:shadow-[0_0_0_1px_rgba(34,211,238,0.25)] rounded-xl px-3.5 py-2.5 text-lg font-semibold outline-none"
                placeholder={noteTitlePlaceholder}
                maxLength={120}
                disabled={!activeNote}
              />
              <div className="flex items-center gap-2 flex-wrap">
                {isJournalMode && (
                  <>
                    <button
                      onClick={() => void deleteCurrentJournal()}
                      disabled={!journalSpace || isDeletingJournal}
                      className="inline-flex items-center gap-2 px-3 py-2 rounded-md border border-rose-300/30 bg-rose-500/15 hover:bg-rose-500/25 text-rose-100 text-sm disabled:opacity-40"
                    >
                      <Trash2 className="w-4 h-4" />
                      {isDeletingJournal ? 'Deleting…' : 'Delete Journal'}
                    </button>
                    <button
                      onClick={() => void synthesizeJournalEntries('current')}
                      disabled={!activeNote || isSynthesizingEntries || journalEntryConversations.length === 0}
                      className="inline-flex items-center gap-2 px-3 py-2 rounded-md border border-emerald-300/30 bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-100 text-sm disabled:opacity-40"
                    >
                      <Sparkles className="w-4 h-4" />
                      {isSynthesizingEntries ? 'Synthesizing…' : 'Synthesize Page'}
                    </button>
                    <button
                      onClick={() => void synthesizeJournalEntries(pinnedEntryIds.size > 0 ? 'pinned' : 'deck')}
                      disabled={!activeNote || isSynthesizingEntries || journalEntryConversations.length === 0}
                      className="inline-flex items-center gap-2 px-3 py-2 rounded-md border border-emerald-300/30 bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-100 text-sm disabled:opacity-40"
                    >
                      <Sparkles className="w-4 h-4" />
                      {isSynthesizingEntries
                        ? 'Synthesizing…'
                        : pinnedEntryIds.size > 0
                          ? `Synthesize Pinned (${Math.min(pinnedEntryIds.size, SYNTHESIS_ENTRY_LIMIT)})`
                          : `Synthesize Deck (${Math.min(journalEntryConversations.length, SYNTHESIS_ENTRY_LIMIT)})`}
                    </button>
                  </>
                )}
                <button
                  onClick={addHighlightFromSelection}
                  disabled={!activeNote}
                  className="inline-flex items-center gap-2 px-3 py-2 rounded-md border border-amber-300/30 bg-amber-500/15 hover:bg-amber-500/25 text-amber-100 text-sm disabled:opacity-40"
                >
                  <Highlighter className="w-4 h-4" />
                  Highlight Selection
                </button>
                <button
                  onClick={() => void saveAllNow(true)}
                  disabled={!activeNote || isSavingNow || !hasPendingChanges}
                  className="inline-flex items-center gap-2 px-3 py-2 rounded-md border border-cyan-300/30 bg-cyan-500/15 hover:bg-cyan-500/25 text-cyan-100 text-sm disabled:opacity-40"
                >
                  <Save className="w-4 h-4" />
                  {isSavingNow ? 'Saving…' : 'Save Now'}
                </button>
              </div>
            </div>
            <p className="text-xs text-white/50 mt-2">
              {isJournalMode
                ? 'Notebook saves automatically. Capture chat entries, highlight insights, and keep references in one place.'
                : 'Pages auto-save to SQLite. Select a page from the left, then edit or preview it here.'}
            </p>
            <p className={`text-xs mt-1 ${hasPendingChanges ? 'text-amber-200' : 'text-emerald-300'}`}>
              {hasPendingChanges ? 'Unsaved changes pending' : 'All changes saved'}
            </p>
            {isSynthesizingEntries && synthesisProgress && (
              <p className="text-xs mt-1 text-cyan-200">{synthesisProgress}</p>
            )}
            {saveError && <p className="text-xs text-rose-300 mt-1">{saveError}</p>}
            {actionNotice && (
              <p
                className={`text-xs mt-1 ${
                  actionNotice.tone === 'error'
                    ? 'text-rose-300'
                    : actionNotice.tone === 'success'
                      ? 'text-emerald-300'
                      : 'text-cyan-200'
                }`}
              >
                {actionNotice.message}
              </p>
            )}
          </div>

          <div className="px-4 md:px-5 pt-3 border-b border-white/10 bg-black/15">
            <div className="flex items-center gap-2 overflow-x-auto pb-3">
              <button onClick={() => setActivePanel('editor')} className={tabButtonClass('editor')}>{tabLabels.editor}</button>
              <button onClick={() => setActivePanel('annotations')} className={tabButtonClass('annotations')}>{tabLabels.annotations}</button>
              <button onClick={() => setActivePanel('documents')} className={tabButtonClass('documents')}>{tabLabels.documents}</button>
              <button onClick={() => setActivePanel('chats')} className={tabButtonClass('chats')}>{tabLabels.chats}</button>
              <button onClick={() => setActivePanel('snapshots')} className={tabButtonClass('snapshots')}>{tabLabels.snapshots}</button>
            </div>
          </div>

          <div className="flex-1 min-h-0 overflow-hidden p-4 md:p-5">
            {activePanel === 'editor' && (
              <div className="h-full overflow-hidden rounded-xl border border-white/10 bg-black/15">
                <div className="flex items-center justify-between border-b border-white/10 px-3 py-2">
                  <p className="text-[11px] uppercase tracking-wide text-white/50">
                    {isJournalMode ? 'Notebook Page' : 'Page'}
                  </p>
                  <div className="flex items-center gap-1.5">
                    <button
                      onClick={() => setEditorView('edit')}
                      className={subviewButtonClass(editorView === 'edit')}
                    >
                      Edit
                    </button>
                    <button
                      onClick={() => setEditorView('preview')}
                      className={subviewButtonClass(editorView === 'preview')}
                    >
                      Preview
                    </button>
                    <button
                      onClick={() => setEditorView('split')}
                      className={subviewButtonClass(editorView === 'split')}
                    >
                      Split
                    </button>
                  </div>
                </div>
                <div
                  className={`h-[calc(100%-41px)] min-h-0 ${
                    editorView === 'split'
                      ? 'grid grid-cols-1 gap-3 p-3 xl:grid-cols-2'
                      : 'p-3'
                  }`}
                >
                  {(editorView === 'edit' || editorView === 'split') && (
                    <textarea
                      ref={editorRef}
                      value={activeNote?.content ?? ''}
                      onChange={(event) => updateActiveNote((note) => ({ ...note, content: event.target.value }))}
                      placeholder="Write your page content here. Use markdown for structure."
                      className="h-full min-h-[260px] w-full rounded-lg border border-white/10 bg-black/20 p-4 text-sm leading-6 text-white/90 outline-none focus:border-cyan-400/60 resize-none"
                      disabled={!activeNote}
                    />
                  )}

                  {(editorView === 'preview' || editorView === 'split') && (
                    <div className="h-full min-h-[260px] w-full overflow-auto rounded-lg border border-white/10 bg-black/20 p-4">
                      {activeNote?.content?.trim() ? (
                        <div className="prose prose-invert prose-sm max-w-none break-words [overflow-wrap:anywhere]">
                          <ReactMarkdown
                            remarkPlugins={[remarkGfm]}
                            components={{
                              p({ children }) {
                                return (
                                  <p className="text-white/80 leading-relaxed mb-3 last:mb-0 break-words whitespace-pre-wrap">
                                    {children}
                                  </p>
                                );
                              },
                              ul({ children }) {
                                return (
                                  <ul className="list-disc list-inside space-y-1 text-white/80 break-words">
                                    {children}
                                  </ul>
                                );
                              },
                              ol({ children }) {
                                return (
                                  <ol className="list-decimal list-inside space-y-1 text-white/80 break-words">
                                    {children}
                                  </ol>
                                );
                              },
                              li({ children }) {
                                return <li className="text-white/80 break-words">{children}</li>;
                              },
                              blockquote({ children }) {
                                return (
                                  <blockquote className="border-l-4 border-cyan-500/45 pl-4 italic text-white/65 my-3 break-words">
                                    {children}
                                  </blockquote>
                                );
                              },
                              a({ children, href }) {
                                return (
                                  <a
                                    href={href}
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    className="text-cyan-300 hover:text-cyan-200 underline transition-colors"
                                  >
                                    {children}
                                  </a>
                                );
                              },
                              table({ children }) {
                                return (
                                  <div className="overflow-x-auto my-3">
                                    <table className="min-w-full border border-white/10 rounded-lg">
                                      {children}
                                    </table>
                                  </div>
                                );
                              },
                              th({ children }) {
                                return (
                                  <th className="px-3 py-2 bg-white/5 border-b border-white/10 text-left text-white/90 font-semibold">
                                    {children}
                                  </th>
                                );
                              },
                              td({ children }) {
                                return (
                                  <td className="px-3 py-2 border-b border-white/5 text-white/80 align-top">
                                    {children}
                                  </td>
                                );
                              },
                            }}
                          >
                            {activeNote.content}
                          </ReactMarkdown>
                        </div>
                      ) : (
                        <div className="space-y-2">
                          <p className="text-sm text-white/45">
                            {isJournalMode
                              ? 'This notebook page is empty.'
                              : 'This page is empty. Start writing in the editor.'}
                          </p>
                          {isJournalMode && (
                            <button
                              onClick={() => setActivePanel('chats')}
                              className="inline-flex items-center gap-1.5 rounded-md border border-emerald-300/35 bg-emerald-500/12 px-2.5 py-1.5 text-xs text-emerald-100 hover:border-emerald-200/70"
                            >
                              <MessageSquare className="w-3.5 h-3.5" />
                              Open Entry Deck
                            </button>
                          )}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              </div>
            )}

            {activePanel === 'annotations' && (
              <div className="h-full overflow-y-auto rounded-xl border border-white/10 bg-black/15 p-4 space-y-3">
                <div className="flex items-center justify-between gap-2">
                  <h2 className="text-xs tracking-wide uppercase text-white/60">
                    {isJournalMode ? 'Highlights & Notes' : 'Annotations'}
                  </h2>
                  <div className="flex items-center gap-1.5">
                    <button
                      onClick={() => setAnnotationView('highlights')}
                      className={subviewButtonClass(annotationView === 'highlights')}
                    >
                      Highlights
                    </button>
                    <button
                      onClick={() => setAnnotationView('stickies')}
                      className={subviewButtonClass(annotationView === 'stickies')}
                    >
                      Stickies
                    </button>
                  </div>
                </div>

                {annotationView === 'highlights' && (
                  <section className="space-y-3">
                    <h3 className="text-xs tracking-wide uppercase text-white/60 flex items-center gap-2">
                      <Highlighter className="w-3.5 h-3.5" />
                      {isJournalMode ? 'Journal Highlights' : 'Highlights'}
                    </h3>
                    {isJournalMode && (
                      <div className="rounded-md border border-white/10 bg-white/[0.03] p-2.5">
                        <p className="text-[11px] text-white/65">
                          Highlights now stay organized inside this notebook.
                          Pin items to keep key findings at the top.
                        </p>
                      </div>
                    )}

                    {isJournalMode && (
                      <div className="space-y-2">
                        <h4 className="text-[11px] uppercase tracking-wide text-white/55">
                          Pinned Highlights
                        </h4>
                        {pinnedJournalBookmarks.length === 0 && pinnedNoteHighlights.length === 0 ? (
                          <p className="text-xs text-white/45">No pinned highlights yet.</p>
                        ) : (
                          <>
                            {pinnedJournalBookmarks.map((bookmark) => (
                              <div key={bookmark.id} className="p-2.5 rounded-md bg-cyan-400/8 border border-cyan-300/20">
                                <div className="flex items-center justify-between gap-2">
                                  <p className="text-xs text-cyan-100/90 line-clamp-1">
                                    {bookmark.title || bookmark.conversationTitle}
                                  </p>
                                  <div className="flex items-center gap-1">
                                    <button
                                      onClick={() => togglePinnedBookmark(bookmark.id)}
                                      className="rounded border border-amber-300/35 bg-amber-500/15 p-1 text-amber-100 hover:bg-amber-500/25"
                                      title="Unpin highlight"
                                    >
                                      <Pin className="h-3 w-3" />
                                    </button>
                                  </div>
                                </div>
                                <p className="mt-1 text-[11px] text-white/65 line-clamp-2">{bookmark.messagePreview}</p>
                              </div>
                            ))}
                            {pinnedNoteHighlights.map((highlight) => (
                              <div key={highlight.id} className="p-2.5 rounded-md bg-amber-300/10 border border-amber-200/25">
                                <p className="text-xs text-amber-100/90 line-clamp-4">{highlight.text}</p>
                                <div className="mt-2 flex items-center justify-between gap-2">
                                  <span className="text-[11px] text-amber-200/60">{formatWhen(highlight.createdAt)}</span>
                                  <div className="flex items-center gap-1">
                                    <button
                                      onClick={() => togglePinnedNoteHighlight(highlight.id)}
                                      className="rounded border border-amber-300/35 bg-amber-500/15 p-1 text-amber-100 hover:bg-amber-500/25"
                                      title="Unpin highlight"
                                    >
                                      <Pin className="h-3 w-3" />
                                    </button>
                                    <button
                                      onClick={() => removeHighlight(highlight.id)}
                                      className="text-[11px] text-amber-100/70 hover:text-amber-100"
                                    >
                                      Remove
                                    </button>
                                  </div>
                                </div>
                              </div>
                            ))}
                          </>
                        )}
                      </div>
                    )}

                    {isJournalMode && (
                      <div className="space-y-2">
                        <div className="flex items-center justify-between gap-2">
                          <h4 className="text-[11px] uppercase tracking-wide text-white/55">
                            Captured References
                          </h4>
                          <button
                            onClick={() => navigate('/references')}
                            className="inline-flex items-center gap-1.5 rounded-md border border-white/20 bg-white/5 px-2 py-1 text-[11px] text-white/75 hover:text-white hover:border-white/35"
                          >
                            <Link2 className="w-3.5 h-3.5" />
                            Reference Inbox
                          </button>
                        </div>
                        {unpinnedJournalBookmarks.length === 0 ? (
                          <p className="text-xs text-white/45">No captured chat highlights in this journal yet.</p>
                        ) : (
                          unpinnedJournalBookmarks.slice(0, 30).map((bookmark) => (
                            <div key={bookmark.id} className="p-2.5 rounded-md bg-cyan-400/8 border border-cyan-300/20">
                              <div className="flex items-center justify-between gap-2">
                                <p className="text-xs text-cyan-100/90 line-clamp-1">
                                  {bookmark.title || bookmark.conversationTitle}
                                </p>
                                <div className="flex items-center gap-1">
                                  <button
                                    onClick={() => void openBookmarkInEntry(bookmark)}
                                    className="text-[11px] rounded border border-cyan-300/30 bg-cyan-500/15 px-2 py-0.5 text-cyan-100 hover:bg-cyan-500/25"
                                  >
                                    Open Entry
                                  </button>
                                  <button
                                    onClick={() => togglePinnedBookmark(bookmark.id)}
                                    className="rounded border border-white/20 bg-white/5 p-1 text-white/70 hover:text-white"
                                    title="Pin highlight"
                                  >
                                    <Pin className="h-3 w-3" />
                                  </button>
                                </div>
                              </div>
                              <p className="mt-1 text-[11px] text-white/65 line-clamp-2">{bookmark.messagePreview}</p>
                            </div>
                          ))
                        )}
                      </div>
                    )}

                    <div className="space-y-2">
                      {isJournalMode && (
                        <h4 className="text-[11px] uppercase tracking-wide text-white/55">
                          Notebook Text Highlights
                        </h4>
                      )}
                      {(isJournalMode ? unpinnedNoteHighlights : noteHighlights).length === 0 ? (
                        <p className="text-xs text-white/45">
                          {isJournalMode ? 'No notebook text highlights yet.' : 'No highlights yet.'}
                        </p>
                      ) : (
                        (isJournalMode ? unpinnedNoteHighlights : noteHighlights).map((highlight) => (
                          <div key={highlight.id} className="p-2.5 rounded-md bg-amber-300/10 border border-amber-200/25">
                            <p className="text-xs text-amber-100/90 line-clamp-4">{highlight.text}</p>
                            <div className="mt-2 flex items-center justify-between">
                              <span className="text-[11px] text-amber-200/60">{formatWhen(highlight.createdAt)}</span>
                              <div className="flex items-center gap-1">
                                {isJournalMode && (
                                  <button
                                    onClick={() => togglePinnedNoteHighlight(highlight.id)}
                                    className="rounded border border-white/20 bg-white/5 p-1 text-white/70 hover:text-white"
                                    title="Pin highlight"
                                  >
                                    <Pin className="h-3 w-3" />
                                  </button>
                                )}
                                <button
                                  onClick={() => removeHighlight(highlight.id)}
                                  className="text-[11px] text-amber-100/70 hover:text-amber-100"
                                >
                                  Remove
                                </button>
                              </div>
                            </div>
                          </div>
                        ))
                      )}
                    </div>
                  </section>
                )}

                {annotationView === 'stickies' && (
                  <section className="space-y-3">
                    <h3 className="text-xs tracking-wide uppercase text-white/60 flex items-center gap-2">
                      <StickyNote className="w-3.5 h-3.5" />
                      Sticky Notes
                    </h3>
                    <div className="space-y-2">
                      <textarea
                        value={newStickyText}
                        onChange={(event) => setNewStickyText(event.target.value)}
                        className="w-full min-h-[68px] rounded-md border border-white/10 bg-black/20 p-2 text-xs outline-none focus:border-cyan-400/60"
                        placeholder="Quick sticky thought..."
                      />
                      <div className="flex items-center gap-2">
                        <select
                          value={newStickyColor}
                          onChange={(event) => setNewStickyColor(event.target.value as StickyColor)}
                          className="flex-1 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs outline-none"
                        >
                          {stickyColorOptions.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                        <button
                          onClick={addStickyNote}
                          className="px-2.5 py-1.5 text-xs rounded-md border border-cyan-300/30 bg-cyan-500/15 hover:bg-cyan-500/25"
                        >
                          Add
                        </button>
                      </div>
                    </div>
                    {(activeNote?.stickyNotes ?? []).length === 0 ? (
                      <p className="text-xs text-white/45">No stickies yet.</p>
                    ) : (
                      activeNote?.stickyNotes.map((sticky) => (
                        <div key={sticky.id} className={`rounded-md border p-2 ${stickyClasses[normalizeStickyColor(sticky.color)]}`}>
                          <textarea
                            value={sticky.text}
                            onChange={(event) => updateStickyText(sticky.id, event.target.value)}
                            className="w-full min-h-[58px] bg-transparent resize-none text-xs leading-5 outline-none"
                          />
                          <div className="mt-1 flex items-center justify-between">
                            <span className="text-[11px] opacity-70">{formatWhen(sticky.createdAt)}</span>
                            <button
                              onClick={() => removeSticky(sticky.id)}
                              className="text-[11px] underline decoration-dotted"
                            >
                              Remove
                            </button>
                          </div>
                        </div>
                      ))
                    )}
                  </section>
                )}
              </div>
            )}

            {activePanel === 'documents' && (
              <>
                {isJournalMode ? (
                  <div className="h-full grid grid-cols-1 gap-4 xl:grid-cols-[20rem_minmax(0,1fr)]">
                    <section className="min-h-0 overflow-hidden rounded-xl border border-white/10 bg-black/15 p-3 space-y-2">
                      <div>
                        <h3 className="text-xs tracking-wide uppercase text-white/60 flex items-center gap-2">
                          <FileText className="w-3.5 h-3.5" />
                          Sources from Journal Entries
                        </h3>
                        <p className="mt-1 text-[11px] text-white/45">
                          References pulled from assistant citations in your entry chats.
                        </p>
                      </div>

                      {contextError && <p className="text-xs text-rose-300">{contextError}</p>}

                      <div className="relative">
                        <Search className="w-3.5 h-3.5 absolute left-2.5 top-2.5 text-white/35" />
                        <input
                          value={docSearch}
                          onChange={(event) => setDocSearch(event.target.value)}
                          className="w-full rounded-md border border-white/10 bg-black/20 pl-8 pr-2 py-2 text-xs outline-none focus:border-cyan-400/60"
                          placeholder="Search chat sources"
                        />
                      </div>

                      <div className="max-h-[calc(100%-126px)] overflow-y-auto space-y-1.5 pr-1">
                        {isLoadingContext ? (
                          <p className="p-2 text-xs text-white/45">Loading journal entries…</p>
                        ) : journalSourcesLoading ? (
                          <p className="p-2 text-xs text-white/45">Loading sources from journal entries…</p>
                        ) : filteredJournalSources.length === 0 ? (
                          <p className="p-2 text-xs text-white/45">No chat sources found in these entries yet.</p>
                        ) : (
                          filteredJournalSources.map((source) => {
                            const isSelected = selectedJournalSource?.key === source.key;
                            return (
                              <button
                                key={source.key}
                                onClick={() => setSelectedJournalSourceKey(source.key)}
                                className={`w-full rounded-md border p-2 text-left transition-colors ${
                                  isSelected
                                    ? 'border-cyan-300/45 bg-cyan-500/12'
                                    : 'border-white/10 bg-white/[0.03] hover:border-white/25'
                                }`}
                              >
                                <div className="flex items-start justify-between gap-2">
                                  <p className="text-xs text-white/90 line-clamp-2">{source.fileName}</p>
                                  <span className="rounded border border-cyan-300/30 bg-cyan-500/15 px-1.5 py-0.5 text-[10px] text-cyan-100">
                                    {source.referenceCount}x
                                  </span>
                                </div>
                                <p className="mt-1 text-[11px] text-white/45 line-clamp-1">{source.filePath}</p>
                                <p className="mt-1 text-[11px] text-white/55">
                                  {source.conversationIds.length} entr{source.conversationIds.length === 1 ? 'y' : 'ies'}
                                </p>
                              </button>
                            );
                          })
                        )}
                      </div>
                    </section>

                    <section className="min-h-0 overflow-hidden rounded-xl border border-white/10 bg-black/15 p-4 space-y-3">
                      <div className="flex items-center justify-between gap-2">
                        <h3 className="text-xs tracking-wide uppercase text-white/60">Source Detail</h3>
                        <p className="text-[11px] text-white/45">
                          Scanning {journalSourceConversations.length} recent entr{journalSourceConversations.length === 1 ? 'y' : 'ies'}
                        </p>
                      </div>

                      {!selectedJournalSource ? (
                        <p className="text-xs text-white/45">Select a source to view details.</p>
                      ) : (
                        <div className="h-full min-h-0 overflow-y-auto space-y-3 pr-1">
                          <div className="rounded-lg border border-white/10 bg-white/[0.03] p-3">
                            <p className="text-sm font-medium text-white/90 break-words">{selectedJournalSource.fileName}</p>
                            <p className="mt-1 text-[11px] text-white/45 break-all">{selectedJournalSource.filePath}</p>
                            <div className="mt-2 flex flex-wrap items-center gap-1.5">
                              <span className="rounded border border-white/20 bg-white/5 px-2 py-0.5 text-[10px] text-white/70">
                                {selectedJournalSource.category}
                              </span>
                              {selectedJournalSource.mimeType && (
                                <span className="rounded border border-white/20 bg-white/5 px-2 py-0.5 text-[10px] text-white/70">
                                  {selectedJournalSource.mimeType}
                                </span>
                              )}
                              {selectedJournalSource.score > 0 && (
                                <span className="rounded border border-cyan-300/30 bg-cyan-500/15 px-2 py-0.5 text-[10px] text-cyan-100">
                                  Score {selectedJournalSource.score.toFixed(2)}
                                </span>
                              )}
                            </div>
                            <div className="mt-3 flex items-center gap-1.5">
                              <button
                                onClick={() => void openJournalSource(selectedJournalSource)}
                                className="inline-flex items-center gap-1 rounded-md border border-cyan-300/30 bg-cyan-500/15 px-2 py-1 text-[11px] text-cyan-100 hover:bg-cyan-500/25"
                              >
                                <FileText className="h-3 w-3" />
                                Open Source
                              </button>
                            </div>
                          </div>

                          <div className="rounded-lg border border-white/10 bg-black/25 p-3">
                            <p className="text-[11px] uppercase tracking-wide text-white/55">Excerpt</p>
                            <p className="mt-1 text-xs text-white/75 whitespace-pre-wrap">
                              {selectedJournalSource.excerpt || 'No excerpt available for this source.'}
                            </p>
                          </div>

                          <div className="rounded-lg border border-white/10 bg-black/25 p-3 space-y-2">
                            <p className="text-[11px] uppercase tracking-wide text-white/55">
                              Referenced in ({selectedJournalSource.conversationIds.length})
                            </p>
                            {selectedJournalSource.conversationIds.length === 0 ? (
                              <p className="text-xs text-white/45">No linked entries found.</p>
                            ) : (
                              selectedJournalSource.conversationIds.map((conversationId, index) => (
                                <button
                                  key={`${selectedJournalSource.key}:${conversationId}`}
                                  onClick={() => void focusJournalEntry(conversationId)}
                                  className="w-full rounded-md border border-white/10 bg-white/[0.03] px-2.5 py-1.5 text-left text-xs text-white/80 transition-colors hover:border-white/25"
                                >
                                  {selectedJournalSource.conversationTitles[index] ?? `Entry ${index + 1}`}
                                </button>
                              ))
                            )}
                          </div>
                        </div>
                      )}
                    </section>
                  </div>
                ) : (
                  <div className="h-full overflow-y-auto space-y-4">
                    <section className="rounded-xl border border-white/10 bg-black/15 p-4 space-y-3">
                      <div className="flex items-center justify-between gap-2">
                        <h3 className="text-xs tracking-wide uppercase text-white/60 flex items-center gap-2">
                          <FileText className="w-3.5 h-3.5" />
                          Documents
                        </h3>
                        <div className="flex items-center gap-1.5">
                          <button
                            onClick={() => setDocumentsView('list')}
                            className={subviewButtonClass(documentsView === 'list')}
                          >
                            List
                          </button>
                          <button
                            onClick={() => setDocumentsView('preview')}
                            disabled={!selectedDocument}
                            className={subviewButtonClass(documentsView === 'preview')}
                          >
                            Preview
                          </button>
                        </div>
                      </div>
                      {contextError && <p className="text-xs text-rose-300">{contextError}</p>}
                      <div className="relative">
                        <Search className="w-3.5 h-3.5 absolute left-2.5 top-2.5 text-white/35" />
                        <input
                          value={docSearch}
                          onChange={(event) => setDocSearch(event.target.value)}
                          className="w-full rounded-md border border-white/10 bg-black/20 pl-8 pr-2 py-2 text-xs outline-none focus:border-cyan-400/60"
                          placeholder="Search files"
                        />
                      </div>
                      <div className="max-h-96 overflow-y-auto rounded-md border border-white/10 bg-black/10 divide-y divide-white/5">
                        {isLoadingContext ? (
                          <p className="p-3 text-xs text-white/45">Loading documents…</p>
                        ) : filteredDocuments.length === 0 ? (
                          <p className="p-3 text-xs text-white/45">No matching files.</p>
                        ) : (
                          filteredDocuments.map((document) => {
                            const linked = activeNote?.linkedDocumentIds.includes(document.id) ?? false;
                            return (
                              <div key={document.id} className="p-2.5 flex items-start gap-2">
                                <input
                                  type="checkbox"
                                  checked={linked}
                                  onChange={() => toggleLinkedDocument(document.id)}
                                  className="mt-1"
                                />
                                <button
                                  onClick={() => selectDocumentForPreview(document.id)}
                                  className="text-left flex-1"
                                >
                                  <p className="text-xs text-white/90 line-clamp-1">{document.fileName}</p>
                                  <p className="text-[11px] text-white/45 line-clamp-1">{document.filePath}</p>
                                </button>
                              </div>
                            );
                          })
                        )}
                      </div>
                      {linkedDocuments.length > 0 && (
                        <p className="text-[11px] text-cyan-100/70 flex items-center gap-1">
                          <Link2 className="w-3 h-3" />
                          {linkedDocuments.length} linked file{linkedDocuments.length > 1 ? 's' : ''}
                        </p>
                      )}
                    </section>
                    {documentsView === 'preview' && (
                      <section className="rounded-xl border border-white/10 bg-black/15 p-4 space-y-2">
                        <h3 className="text-xs tracking-wide uppercase text-white/60">File Preview</h3>
                        {!selectedDocument && <p className="text-xs text-white/45">Select a file to preview.</p>}
                        {selectedDocument && (
                          <div className="space-y-2">
                            <p className="text-xs font-medium text-white/90">{selectedDocument.fileName}</p>
                            <p className="text-[11px] text-white/45">{selectedDocument.filePath}</p>
                            <p className="text-[11px] text-white/45">
                              {selectedDocument.fileType} · {selectedDocument.wordCount} words
                            </p>
                            {selectedDocumentPreview?.loading && <p className="text-[11px] text-white/45">Loading preview…</p>}
                            {selectedDocumentPreview?.error && <p className="text-[11px] text-rose-300">{selectedDocumentPreview.error}</p>}
                            {selectedDocumentPreview?.content && (
                              <pre className="text-[11px] whitespace-pre-wrap max-h-[26rem] overflow-y-auto rounded bg-black/30 p-2 border border-white/10 text-white/70">
                                {selectedDocumentPreview.content}
                              </pre>
                            )}
                          </div>
                        )}
                      </section>
                    )}
                  </div>
                )}
              </>
            )}

            {activePanel === 'chats' && (
              <>
                {isJournalMode ? (
                  <div className="h-full grid grid-cols-1 gap-4 xl:grid-cols-[18rem_minmax(0,1fr)]">
                    <section className="min-h-0 overflow-hidden rounded-xl border border-white/10 bg-black/15 p-3 space-y-2">
                      <div>
                        <h3 className="text-xs tracking-wide uppercase text-white/60 flex items-center gap-2">
                          <MessageSquare className="w-3.5 h-3.5" />
                          Entry Deck
                        </h3>
                        <p className="mt-1 text-[11px] text-white/45">
                          Flip through chats like pages in your notebook.
                        </p>
                      </div>

                      <div className="relative">
                        <Search className="w-3.5 h-3.5 absolute left-2.5 top-2.5 text-white/35" />
                        <input
                          value={conversationSearch}
                          onChange={(event) => setConversationSearch(event.target.value)}
                          className="w-full rounded-md border border-white/10 bg-black/20 pl-8 pr-2 py-2 text-xs outline-none focus:border-cyan-400/60"
                          placeholder="Search journal entries"
                        />
                      </div>

                      <div className="max-h-[calc(100%-96px)] overflow-y-auto space-y-1.5 pr-1">
                        {isLoadingContext ? (
                          <p className="p-2 text-xs text-white/45">Loading entries…</p>
                        ) : journalEntryConversations.length === 0 ? (
                          <p className="p-2 text-xs text-white/45">No matching entries.</p>
                        ) : (
                          journalEntryConversations.map((conversation) => {
                            const isSelected = selectedConversationPreviewId === conversation.id;
                            const isPinned = pinnedEntryIds.has(conversation.id);
                            return (
                              <button
                                key={conversation.id}
                                onClick={() => setSelectedConversationPreviewId(conversation.id)}
                                className={`w-full rounded-md border p-2 text-left transition-colors ${
                                  isSelected
                                    ? 'border-cyan-300/45 bg-cyan-500/12'
                                    : 'border-white/10 bg-white/[0.03] hover:border-white/25'
                                }`}
                              >
                                <div className="flex items-start justify-between gap-2">
                                  <p className="text-xs text-white/90 line-clamp-2">{conversation.title}</p>
                                  <button
                                    onClick={(event) => {
                                      event.stopPropagation();
                                      togglePinnedEntry(conversation.id);
                                    }}
                                    className={`rounded border p-1 transition-colors ${
                                      isPinned
                                        ? 'border-amber-300/35 bg-amber-500/15 text-amber-100'
                                        : 'border-white/20 bg-white/5 text-white/60 hover:text-white'
                                    }`}
                                    title={isPinned ? 'Unpin entry' : 'Pin entry'}
                                  >
                                    <Pin className="h-3 w-3" />
                                  </button>
                                </div>
                                <p className="mt-1 text-[11px] text-white/45">
                                  {conversation.messageCount} messages · {formatWhen(conversation.updatedAt)}
                                </p>
                              </button>
                            );
                          })
                        )}
                      </div>
                    </section>

                    <section className="min-h-0 overflow-hidden rounded-xl border border-white/10 bg-black/15 p-4 space-y-3">
                      <div className="flex items-center justify-between">
                        <div>
                          <h3 className="text-xs tracking-wide uppercase text-white/60">Notebook Page View</h3>
                          <p className="text-[11px] text-white/45">
                            Page {currentPreviewIndex >= 0 ? currentPreviewIndex + 1 : 0} of {previewConversations.length}
                          </p>
                        </div>
                        <div className="flex items-center gap-1.5">
                          <button
                            onClick={() => void goToAdjacentPreviewConversation(-1)}
                            disabled={!canGoPrevPreview}
                            className="inline-flex items-center justify-center rounded border border-white/15 bg-white/5 p-1 text-white/70 transition-colors hover:border-white/30 hover:text-white disabled:opacity-40"
                            title="Previous page"
                          >
                            <ChevronLeft className="h-3.5 w-3.5" />
                          </button>
                          <button
                            onClick={() => void goToAdjacentPreviewConversation(1)}
                            disabled={!canGoNextPreview}
                            className="inline-flex items-center justify-center rounded border border-white/15 bg-white/5 p-1 text-white/70 transition-colors hover:border-white/30 hover:text-white disabled:opacity-40"
                            title="Next page"
                          >
                            <ChevronRight className="h-3.5 w-3.5" />
                          </button>
                        </div>
                      </div>

                      {!selectedJournalConversation && (
                        <p className="text-xs text-white/45">Select an entry to open its notebook page.</p>
                      )}

                      {selectedJournalConversation && (
                        <>
                          <div className="rounded-lg border border-white/10 bg-white/[0.03] p-3">
                            <p className="text-sm font-medium text-white/90">{selectedJournalConversation.title}</p>
                            <p className="mt-1 text-[11px] text-white/50">
                              {selectedJournalConversation.messageCount} messages · {formatWhen(selectedJournalConversation.updatedAt)}
                            </p>
                            <div className="mt-2 flex items-center gap-1.5 flex-wrap">
                              <button
                                onClick={() => togglePinnedEntry(selectedJournalConversation.id)}
                                className={`inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[11px] transition-colors ${
                                  pinnedEntryIds.has(selectedJournalConversation.id)
                                    ? 'border-amber-300/35 bg-amber-500/15 text-amber-100'
                                    : 'border-white/20 bg-white/5 text-white/70 hover:text-white'
                                }`}
                              >
                                <Pin className="h-3 w-3" />
                                {pinnedEntryIds.has(selectedJournalConversation.id) ? 'Unpin Entry' : 'Pin Entry'}
                              </button>
                              <button
                                onClick={() => void captureConversation(selectedJournalConversation.id)}
                                className="inline-flex items-center gap-1 rounded-md border border-emerald-300/30 bg-emerald-500/15 px-2 py-1 text-[11px] text-emerald-100 hover:bg-emerald-500/25"
                              >
                                <Sparkles className="h-3 w-3" />
                                Add to Notebook
                              </button>
                              <button
                                onClick={() => void startNewChatFromEntrySources(selectedJournalConversation)}
                                className="inline-flex items-center gap-1 rounded-md border border-cyan-300/30 bg-cyan-500/15 px-2 py-1 text-[11px] text-cyan-100 hover:bg-cyan-500/25"
                              >
                                <MessageSquare className="h-3 w-3" />
                                New Chat from Sources
                              </button>
                            </div>
                          </div>

                          {selectedConversationPreviewId && conversationLoading[selectedConversationPreviewId] && (
                            <p className="text-[11px] text-white/45">Loading entry…</p>
                          )}

                          <div className="max-h-[calc(100%-178px)] overflow-y-auto space-y-2 pr-1">
                            {selectedConversationMessages.length === 0 ? (
                              <p className="text-[11px] text-white/45">No messages loaded.</p>
                            ) : (
                              selectedConversationMessages.map((message, idx) => (
                                <div key={message.id} className="rounded-md border border-white/10 bg-black/25 p-3">
                                  <div className="mb-1.5 flex items-center justify-between gap-2">
                                    <p className="text-[10px] uppercase tracking-wide text-cyan-200/80">
                                      {message.role}
                                    </p>
                                    <p className="text-[10px] text-white/45">Block {idx + 1}</p>
                                  </div>
                                  <p className="text-xs text-white/80 whitespace-pre-wrap">
                                    {message.content}
                                  </p>
                                </div>
                              ))
                            )}
                          </div>
                        </>
                      )}
                    </section>
                  </div>
                ) : (
                  <div className="h-full overflow-y-auto space-y-4">
                    <section className="rounded-xl border border-white/10 bg-black/15 p-4 space-y-3">
                      <div className="flex items-center justify-between gap-2">
                        <h3 className="text-xs tracking-wide uppercase text-white/60 flex items-center gap-2">
                          <MessageSquare className="w-3.5 h-3.5" />
                          Chats
                        </h3>
                        <div className="flex items-center gap-1.5">
                          <button
                            onClick={() => setChatsView('list')}
                            className={subviewButtonClass(chatsView === 'list')}
                          >
                            List
                          </button>
                          <button
                            onClick={() => setChatsView('preview')}
                            disabled={!selectedConversationPreviewId}
                            className={subviewButtonClass(chatsView === 'preview')}
                          >
                            Preview
                          </button>
                        </div>
                      </div>
                      <div className="relative">
                        <Search className="w-3.5 h-3.5 absolute left-2.5 top-2.5 text-white/35" />
                        <input
                          value={conversationSearch}
                          onChange={(event) => setConversationSearch(event.target.value)}
                          className="w-full rounded-md border border-white/10 bg-black/20 pl-8 pr-2 py-2 text-xs outline-none focus:border-cyan-400/60"
                          placeholder="Search chats"
                        />
                      </div>
                      <div className="max-h-96 overflow-y-auto rounded-md border border-white/10 bg-black/10 divide-y divide-white/5">
                        {isLoadingContext ? (
                          <p className="p-3 text-xs text-white/45">Loading chats…</p>
                        ) : filteredConversations.length === 0 ? (
                          <p className="p-3 text-xs text-white/45">No matching chats.</p>
                        ) : (
                          filteredConversations.map((conversation) => {
                            const linked = activeNote?.linkedConversationIds.includes(conversation.id) ?? false;
                            const isActive = activeConversationId === conversation.id;
                            return (
                              <div key={conversation.id} className="p-2.5 flex items-start gap-2">
                                <input
                                  type="checkbox"
                                  checked={linked}
                                  onChange={() => toggleLinkedConversation(conversation.id)}
                                  className="mt-1"
                                />
                                <button
                                  onClick={() => selectConversationForPreview(conversation.id)}
                                  className="text-left flex-1"
                                >
                                  <p className="text-xs text-white/90 line-clamp-1">{conversation.title}</p>
                                  <p className="text-[11px] text-white/45">
                                    {conversation.messageCount} messages · {formatWhen(conversation.updatedAt)}
                                    {isActive ? ' · active' : ''}
                                  </p>
                                </button>
                                <button
                                  onClick={() => captureConversation(conversation.id)}
                                  className="px-2 py-1 rounded border border-emerald-300/30 bg-emerald-500/15 text-[11px] text-emerald-100 hover:bg-emerald-500/25"
                                  title="Capture full chat into this note"
                                >
                                  Save
                                </button>
                              </div>
                            );
                          })
                        )}
                      </div>
                    </section>
                    {chatsView === 'preview' && (
                      <section className="rounded-xl border border-white/10 bg-black/15 p-4 space-y-2">
                        <div className="flex items-center justify-between">
                          <h3 className="text-xs tracking-wide uppercase text-white/60">Chat Preview</h3>
                          {selectedConversationPreviewId && conversationLoading[selectedConversationPreviewId] && (
                            <span className="text-[11px] text-white/45">Loading…</span>
                          )}
                        </div>
                        {!selectedConversationPreviewId && <p className="text-xs text-white/45">Select a chat to preview.</p>}
                        {selectedConversationPreviewId && (
                          <div className="max-h-[30rem] overflow-y-auto space-y-2">
                            {selectedConversationMessages.length === 0 ? (
                              <p className="text-[11px] text-white/45">No messages loaded.</p>
                            ) : (
                              selectedConversationMessages.map((message) => (
                                <div key={message.id} className="rounded border border-white/10 p-2 bg-black/25">
                                  <p className="text-[10px] uppercase tracking-wide text-cyan-200/80">
                                    {message.role}
                                  </p>
                                  <p className="text-[11px] text-white/75 whitespace-pre-wrap line-clamp-6">
                                    {message.content}
                                  </p>
                                </div>
                              ))
                            )}
                          </div>
                        )}
                      </section>
                    )}
                  </div>
                )}
              </>
            )}

            {activePanel === 'snapshots' && (
              <div className="h-full overflow-y-auto rounded-xl border border-white/10 bg-black/15 p-4 space-y-3">
                <div className="flex items-center justify-between gap-2">
                  <h3 className="text-xs tracking-wide uppercase text-white/60 flex items-center gap-2">
                    <Sparkles className="w-3.5 h-3.5" />
                    {isJournalMode ? 'Captured Entries Timeline' : 'Captured Snapshots'}
                  </h3>
                  <button
                    onClick={captureActiveConversation}
                    disabled={!activeNote}
                    className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border border-emerald-300/30 bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-100 text-[11px] disabled:opacity-40"
                  >
                    <Sparkles className="w-3 h-3" />
                    {isJournalMode ? 'Add Active Entry' : 'Capture Active Chat'}
                  </button>
                </div>
                {(activeNote?.conversationSnapshots ?? []).length === 0 ? (
                  <p className="text-xs text-white/45">
                    {isJournalMode ? 'No entries captured yet.' : 'No snapshots captured yet.'}
                  </p>
                ) : (
                  activeNote?.conversationSnapshots.map((snapshot) => {
                    const isLinkedSnapshot = requestedSnapshotId === snapshot.id;
                    return (
                    <div
                      key={snapshot.id}
                      data-snapshot-id={snapshot.id}
                      className={`rounded-md border p-3 ${
                        isLinkedSnapshot
                          ? 'border-cyan-300/60 bg-cyan-500/12'
                          : 'border-white/10 bg-black/20'
                      }`}
                    >
                      <p className="text-sm text-white/90 flex items-center gap-2">
                        <span>{snapshot.conversationTitle}</span>
                        {isLinkedSnapshot && (
                          <span className="rounded-full border border-cyan-300/45 bg-cyan-500/20 px-2 py-0.5 text-[10px] uppercase tracking-wide text-cyan-100">
                            From chat capture
                          </span>
                        )}
                      </p>
                      <p className="text-[11px] text-white/45">
                        {snapshot.messageCount} messages · {formatWhen(snapshot.capturedAt)}
                      </p>
                      <div className="mt-2 flex items-center gap-1.5">
                        <button
                          onClick={() => insertSnapshotIntoNote(snapshot)}
                          className="px-2.5 py-1.5 text-[11px] rounded border border-cyan-300/30 bg-cyan-500/15 text-cyan-100 hover:bg-cyan-500/25"
                        >
                          Insert into note
                        </button>
                        <button
                          onClick={() => void focusJournalEntry(snapshot.conversationId)}
                          className="px-2.5 py-1.5 text-[11px] rounded border border-emerald-300/35 bg-emerald-500/15 text-emerald-100 hover:bg-emerald-500/25"
                        >
                          Open Entry
                        </button>
                      </div>
                    </div>
                    );
                  })
                )}
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
