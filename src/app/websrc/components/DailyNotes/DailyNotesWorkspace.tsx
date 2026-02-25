import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  CalendarDays,
  FileText,
  Highlighter,
  Link2,
  MessageSquare,
  NotebookPen,
  Plus,
  Search,
  Save,
  Sparkles,
  StickyNote,
  Trash2,
} from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import { useNavigate, useSearchParams } from 'react-router-dom';
import remarkGfm from 'remark-gfm';

import VaultAPI from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { DocumentMetadata } from '@/types';
import type { ConversationDto, MessageDto } from '@/types/api/conversation';
import type {
  WorkspaceNote,
  SnapshotMessage,
  ConversationSnapshot,
} from '@/types/api/dailyNotes';

const DOCUMENT_LIMIT = 500;
const CONVERSATION_LIMIT = 100;
const PREVIEW_CHAR_LIMIT = 4000;
const HIGHLIGHT_CHAR_LIMIT = 8000;

type MessageRole = 'user' | 'assistant' | 'system';
type StickyColor = 'amber' | 'sky' | 'rose' | 'mint';
type PanelTab = 'editor' | 'annotations' | 'documents' | 'chats' | 'snapshots';
type ActionTone = 'info' | 'success' | 'error';
type AnnotationView = 'highlights' | 'stickies';
type ResourceView = 'list' | 'preview';
type EditorView = 'edit' | 'preview' | 'split';

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
  return `Daily Notes · ${new Date().toLocaleDateString(undefined, {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
  })}`;
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

function normalizeMessage(raw: MessageDto | Record<string, unknown>): SnapshotMessage {
  const source = raw as Partial<MessageDto> & Record<string, unknown>;
  return {
    id: asString(source.id, makeId('message')),
    role: asRole(source.role),
    content: asString(source.content),
    createdAt: asString(source.createdAt ?? source.created_at, nowIso()),
  };
}

export function DailyNotesWorkspace() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const [notes, setNotes] = useState<WorkspaceNote[]>([]);
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

  const [isLoadingContext, setIsLoadingContext] = useState(true);
  const [contextError, setContextError] = useState<string | null>(null);

  const editorRef = useRef<HTMLTextAreaElement | null>(null);
  const persistTimersRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
  const dirtyNoteIdsRef = useRef<Set<string>>(new Set());
  const notesRef = useRef<WorkspaceNote[]>([]);
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);
  const requestedNoteId = searchParams.get('noteId');
  const requestedSnapshotId = searchParams.get('snapshotId');

  const activeNote = useMemo(
    () => notes.find((note) => note.id === activeNoteId) ?? notes[0] ?? null,
    [notes, activeNoteId],
  );

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

    loadNotes();

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    notesRef.current = notes;
  }, [notes]);

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

      const [docsResult, conversationsResult] = await Promise.all([
        VaultAPI.listAllDocuments(DOCUMENT_LIMIT),
        VaultAPI.listConversations(CONVERSATION_LIMIT, 0),
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

      if (conversationsResult.ok) {
        const data = conversationsResult.data;
        const rawConversations = Array.isArray(data)
          ? data
          : Array.isArray(data.conversations)
            ? data.conversations
            : [];
        setConversations(rawConversations.map((conversation) => normalizeConversation(conversation)));
      } else {
        errors.push(`conversations: ${conversationsResult.error}`);
      }

      setContextError(errors.length > 0 ? `Failed loading ${errors.join(' · ')}` : null);
      setIsLoadingContext(false);
    };

    loadContext();

    return () => {
      cancelled = true;
    };
  }, []);

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

  const openSnapshotInChat = (snapshot: ConversationSnapshot) => {
    const params = new URLSearchParams({ conversationId: snapshot.conversationId });
    const anchorMessageId = snapshot.messages[snapshot.messages.length - 1]?.id;
    if (anchorMessageId) {
      params.set('messageId', anchorMessageId);
    }
    navigate(`/chat?${params.toString()}`);
  };

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

  const linkedDocuments = activeNote
    ? documents.filter((document) => activeNote.linkedDocumentIds.includes(document.id))
    : [];

  const tabButtonClass = (tab: PanelTab): string =>
    `px-3 py-1.5 rounded-md text-xs border transition-colors ${
      activePanel === tab
        ? 'border-cyan-400/60 bg-cyan-500/20 text-cyan-100'
        : 'border-white/15 bg-white/5 text-white/70 hover:bg-white/10'
    }`;
  const subviewButtonClass = (active: boolean): string =>
    `px-2.5 py-1 rounded-md text-[11px] border transition-colors ${
      active
        ? 'border-cyan-300/60 bg-cyan-500/20 text-cyan-100'
        : 'border-white/15 bg-white/5 text-white/65 hover:bg-white/10'
    }`;

  return (
    <div className="h-full overflow-hidden bg-[radial-gradient(circle_at_top_right,rgba(56,189,248,0.12),transparent_45%),radial-gradient(circle_at_bottom_left,rgba(251,191,36,0.12),transparent_50%),var(--bg-primary)] text-[var(--text-primary)]">
      <div className="h-full grid grid-cols-1 xl:grid-cols-[18rem_minmax(0,1fr)]">
        <aside className="border-b xl:border-b-0 xl:border-r border-white/10 bg-black/20 backdrop-blur-sm flex flex-col min-h-0">
          <div className="p-4 border-b border-white/10">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-cyan-400/40 to-amber-300/40 flex items-center justify-center">
                  <CalendarDays className="w-4 h-4 text-cyan-100" />
                </div>
                <div>
                  <h1 className="text-sm font-semibold tracking-wide uppercase text-white/80">Daily Notes</h1>
                  <p className="text-xs text-white/50">SQLite-backed notes + chat synthesis</p>
                </div>
              </div>
              <button
                onClick={createNewNote}
                className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-md bg-cyan-500/20 hover:bg-cyan-500/30 border border-cyan-400/30 text-cyan-100 text-xs transition-colors"
                title="Create note"
              >
                <Plus className="w-3.5 h-3.5" />
                New
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-2 space-y-2">
            {isLoadingNotes && (
              <p className="px-2 py-1 text-xs text-white/45">Loading notes…</p>
            )}
            {notesError && (
              <p className="px-2 py-1 text-xs text-rose-300">{notesError}</p>
            )}
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
          </div>

          <div className="p-3 border-t border-white/10">
            <button
              onClick={deleteActiveNote}
              disabled={!activeNote}
              className="w-full inline-flex items-center justify-center gap-2 px-3 py-2 rounded-md bg-rose-500/15 hover:bg-rose-500/25 border border-rose-300/25 text-rose-100 text-sm disabled:opacity-40"
            >
              <Trash2 className="w-4 h-4" />
              Delete Note
            </button>
          </div>
        </aside>

        <main className="min-h-0 flex flex-col">
          <div className="p-4 md:p-5 border-b border-white/10 bg-black/15 backdrop-blur-sm">
            <div className="flex flex-col md:flex-row md:items-center gap-3 md:gap-4">
              <input
                type="text"
                value={activeNote?.title ?? ''}
                onChange={(event) => updateActiveNote((note) => ({ ...note, title: event.target.value }))}
                className="flex-1 bg-white/5 border border-white/10 focus:border-cyan-400/60 rounded-lg px-3 py-2 text-lg font-semibold outline-none"
                placeholder="Note title"
                maxLength={120}
                disabled={!activeNote}
              />
              <div className="flex items-center gap-2 flex-wrap">
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
              Notes auto-save to SQLite. Use chat snapshots to merge full conversation history into any note.
            </p>
            <p className={`text-xs mt-1 ${hasPendingChanges ? 'text-amber-200' : 'text-emerald-300'}`}>
              {hasPendingChanges ? 'Unsaved changes pending' : 'All changes saved'}
            </p>
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

          <div className="px-4 md:px-5 pt-3 border-b border-white/10 bg-black/10">
            <div className="flex items-center gap-2 overflow-x-auto pb-3">
              <button onClick={() => setActivePanel('editor')} className={tabButtonClass('editor')}>Editor</button>
              <button onClick={() => setActivePanel('annotations')} className={tabButtonClass('annotations')}>Annotations</button>
              <button onClick={() => setActivePanel('documents')} className={tabButtonClass('documents')}>Documents</button>
              <button onClick={() => setActivePanel('chats')} className={tabButtonClass('chats')}>Chats</button>
              <button onClick={() => setActivePanel('snapshots')} className={tabButtonClass('snapshots')}>Snapshots</button>
            </div>
          </div>

          <div className="flex-1 min-h-0 overflow-hidden p-4 md:p-5">
            {activePanel === 'editor' && (
              <div className="h-full overflow-hidden rounded-xl border border-white/10 bg-black/15">
                <div className="flex items-center justify-between border-b border-white/10 px-3 py-2">
                  <p className="text-[11px] uppercase tracking-wide text-white/50">Markdown Note</p>
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
                      placeholder="Write, synthesize, and connect ideas. Use this space for your main note body."
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
                        <p className="text-sm text-white/45">
                          Nothing to preview yet. Start writing markdown in the editor.
                        </p>
                      )}
                    </div>
                  )}
                </div>
              </div>
            )}

            {activePanel === 'annotations' && (
              <div className="h-full overflow-y-auto rounded-xl border border-white/10 bg-black/15 p-4 space-y-3">
                <div className="flex items-center justify-between gap-2">
                  <h2 className="text-xs tracking-wide uppercase text-white/60">Annotations</h2>
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
                      Highlights
                    </h3>
                    {(activeNote?.highlights ?? []).length === 0 ? (
                      <p className="text-xs text-white/45">No highlights yet.</p>
                    ) : (
                      activeNote?.highlights.map((highlight) => (
                        <div key={highlight.id} className="p-2.5 rounded-md bg-amber-300/10 border border-amber-200/25">
                          <p className="text-xs text-amber-100/90 line-clamp-4">{highlight.text}</p>
                          <div className="mt-2 flex items-center justify-between">
                            <span className="text-[11px] text-amber-200/60">{formatWhen(highlight.createdAt)}</span>
                            <button
                              onClick={() => removeHighlight(highlight.id)}
                              className="text-[11px] text-amber-100/70 hover:text-amber-100"
                            >
                              Remove
                            </button>
                          </div>
                        </div>
                      ))
                    )}
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

            {activePanel === 'chats' && (
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

            {activePanel === 'snapshots' && (
              <div className="h-full overflow-y-auto rounded-xl border border-white/10 bg-black/15 p-4 space-y-3">
                <div className="flex items-center justify-between gap-2">
                  <h3 className="text-xs tracking-wide uppercase text-white/60 flex items-center gap-2">
                    <Sparkles className="w-3.5 h-3.5" />
                    Captured Snapshots
                  </h3>
                  <button
                    onClick={captureActiveConversation}
                    disabled={!activeNote}
                    className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border border-emerald-300/30 bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-100 text-[11px] disabled:opacity-40"
                  >
                    <Sparkles className="w-3 h-3" />
                    Capture Active Chat
                  </button>
                </div>
                {(activeNote?.conversationSnapshots ?? []).length === 0 ? (
                  <p className="text-xs text-white/45">No snapshots captured yet.</p>
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
                          onClick={() => openSnapshotInChat(snapshot)}
                          className="px-2.5 py-1.5 text-[11px] rounded border border-emerald-300/35 bg-emerald-500/15 text-emerald-100 hover:bg-emerald-500/25"
                        >
                          Open in Chat
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
