import { useCallback, useMemo, useState } from 'react';

import { ClipboardCopy, NotebookPen } from 'lucide-react';
import { useNavigate } from 'react-router';

import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';
import { VaultAPI } from '@/lib/api';
import { useConversationsStore } from '@/stores/conversationsStore';
import type { PaletteCommand } from '@/stores/paletteCommandsStore';
import { toast } from '@/stores/toastStore';
import { conversationToMarkdown } from '@/utils/conversationExport';

import { useJournalsQuery } from './workspaceQueries';

/**
 * The two ways a conversation leaves Lattice: onto the clipboard, or into the
 * journal.
 *
 * Both are offered from the palette and from the sidebar row's menu, because a
 * researcher wanting to quote a thread is rarely in that thread at the time —
 * the row menu acts on the conversation under the pointer, not the open one.
 *
 * Neither reads the conversations store's per-message maps: those hold only the
 * open conversation, and the menu can be opened on any row. The messages are
 * fetched and their persisted metadata is read instead, which is the same data
 * the store derives its maps from.
 */

/** What the journal page is called when the backend hands one back unnamed. */
const UNTITLED_PAGE = 'your journal';

type ExportKind = 'copy' | 'journal';

interface RunningExport {
  id: string;
  kind: ExportKind;
}

export function useConversationExport() {
  const navigate = useNavigate();
  const conversations = useConversationsStore((state) => state.conversations);
  const spaces = useConversationsStore((state) => state.spaces);
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);
  const { journals } = useJournalsQuery();
  const [running, setRunning] = useState<RunningExport | null>(null);

  const spaceLabelById = useMemo(() => {
    const map = new Map<string, string>();
    for (const space of spaces) map.set(space.id, space.name);
    // A journal is a space too, and a thread filed in one should say so rather
    // than looking like it searched an ordinary library.
    for (const journal of journals) map.set(journal.id, `${journal.name} · Journal`);
    return map;
  }, [journals, spaces]);

  /** The whole thread as markdown, or null once the failure has been reported. */
  const buildMarkdown = useCallback(
    async (id: string, title: string): Promise<string | null> => {
      const conversation = conversations.find((item) => item.id === id) ?? { id, title, updatedAt: '' };
      const result = await VaultAPI.getConversationMessages(id);
      if (!result.ok) {
        toast.error("Couldn't read that conversation", { message: result.error });
        return null;
      }
      const messages = result.data.messages ?? [];
      if (messages.length === 0) {
        toast.info('Nothing to export yet', { message: `"${title}" has no messages.` });
        return null;
      }
      const spaceId = 'spaceId' in conversation ? conversation.spaceId : null;
      return conversationToMarkdown({ ...conversation, title }, messages, {
        spaceName: spaceId ? spaceLabelById.get(spaceId) ?? null : null,
      });
    },
    [conversations, spaceLabelById],
  );

  const copyConversationAsMarkdown = useCallback(
    async (id: string, title: string) => {
      setRunning({ id, kind: 'copy' });
      try {
        const markdown = await buildMarkdown(id, title);
        if (markdown === null) return;
        await navigator.clipboard.writeText(markdown);
        // Say how much went, so a silent clipboard is not mistaken for a no-op.
        const lines = markdown.split('\n').length;
        toast.success('Copied as Markdown', {
          message: `"${title}" — ${lines} lines, with every source and verification line.`,
        });
      } catch (error) {
        toast.error("Couldn't copy that conversation", {
          message: error instanceof Error ? error.message : String(error),
        });
      } finally {
        setRunning(null);
      }
    },
    [buildMarkdown],
  );

  const saveConversationToJournal = useCallback(
    async (id: string, title: string) => {
      setRunning({ id, kind: 'journal' });
      try {
        const markdown = await buildMarkdown(id, title);
        if (markdown === null) return;
        const captured = await VaultAPI.quickCapture(markdown);
        if (!captured.ok) {
          toast.error('Could not write to the journal', { message: captured.error });
          return;
        }
        const { noteId, noteTitle } = captured.data;
        toast.success(`Saved to "${noteTitle || UNTITLED_PAGE}"`);
        navigate(`/journals?${new URLSearchParams({ noteId }).toString()}`);
      } catch (error) {
        toast.error('Could not write to the journal', {
          message: error instanceof Error ? error.message : String(error),
        });
      } finally {
        setRunning(null);
      }
    },
    [buildMarkdown, navigate],
  );

  const exportPaletteCommands = useMemo<PaletteCommand[]>(() => {
    const active = conversations.find((item) => item.id === activeConversationId);
    const title = active?.title ?? 'Conversation';
    return [
      {
        id: 'chat.copyConversationAsMarkdown',
        label: 'Copy conversation as Markdown',
        group: 'Chat',
        icon: ClipboardCopy,
        description: 'Every turn, its numbered sources and what the check found',
        keywords: ['export', 'markdown', 'clipboard'],
        enabled: Boolean(activeConversationId),
        run: () => {
          if (!activeConversationId) return;
          void copyConversationAsMarkdown(activeConversationId, title);
        },
      },
      {
        id: 'chat.saveConversationToJournal',
        label: 'Save conversation to Journal',
        group: 'Journal',
        icon: NotebookPen,
        description: 'Writes the thread, with its sources, to your journal',
        keywords: ['export', 'capture', 'journal'],
        enabled: Boolean(activeConversationId),
        run: () => {
          if (!activeConversationId) return;
          void saveConversationToJournal(activeConversationId, title);
        },
      },
    ];
  }, [activeConversationId, conversations, copyConversationAsMarkdown, saveConversationToJournal]);
  useRegisterPaletteCommands(exportPaletteCommands);

  return {
    copyConversationAsMarkdown,
    saveConversationToJournal,
    /** The row that is exporting right now, so it can show a spinner. */
    copyingConversationId: running?.kind === 'copy' ? running.id : null,
    savingConversationId: running?.kind === 'journal' ? running.id : null,
  };
}

export type ConversationExportActions = ReturnType<typeof useConversationExport>;
