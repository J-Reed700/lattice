import { useEffect, useMemo } from 'react';

import { Combine, FilePlus2, Plus } from 'lucide-react';

import type { SynthesisScope } from '@/features/journal/components/SynthesizePopover';
import type { JournalEntrySummary } from '@/features/journal/hooks/useJournalEntries';
import { useRegisterPaletteCommands } from '@/features/palette/hooks/useRegisterPaletteCommands';
import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import type { ConversationJournalDto } from '@/types/api/conversation';

/** The journal's entries in the command palette. */
export function useJournalPaletteCommands(options: {
  weekCandidateTotal: number | undefined;
  isSynthesizing: boolean;
  pinnedIds: Set<string>;
  journalSpace: ConversationJournalDto | null;
  handleSynthesize: (scope: SynthesisScope) => Promise<boolean>;
  handleNewEntry: (spaceId?: string) => Promise<void>;
  handleNewPage: () => Promise<void>;
}) {
  const {
    weekCandidateTotal,
    isSynthesizing,
    pinnedIds,
    journalSpace,
    handleSynthesize,
    handleNewEntry,
    handleNewPage,
  } = options;

  const journalPaletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'journal.synthesize.week',
        label: 'Synthesize the past week',
        group: 'Journal',
        icon: Combine,
        enabled: (weekCandidateTotal ?? 0) > 0 && !isSynthesizing,
        run: () => {
          void handleSynthesize('week');
        },
      },
      {
        id: 'journal.synthesize.pinned',
        label: 'Synthesize pinned entries',
        group: 'Journal',
        icon: Combine,
        enabled: pinnedIds.size > 0 && !isSynthesizing,
        run: () => {
          void handleSynthesize('pinned');
        },
      },
      {
        id: 'journal.newEntry',
        label: 'Ask in Chat, filed under this journal',
        group: 'Journal',
        icon: Plus,
        enabled: Boolean(journalSpace),
        run: () => {
          void handleNewEntry();
        },
      },
      {
        id: 'journal.newPage',
        label: 'New journal page',
        group: 'Journal',
        icon: FilePlus2,
        enabled: Boolean(journalSpace),
        run: () => {
          void handleNewPage();
        },
      },
    ],
    [
      handleNewEntry,
      handleNewPage,
      handleSynthesize,
      journalSpace,
      isSynthesizing,
      pinnedIds,
      weekCandidateTotal,
    ],
  );
  useRegisterPaletteCommands(journalPaletteCommands);
}

/** The journal workspace's keyboard shortcuts. */
export function useJournalShortcuts(options: {
  entries: JournalEntrySummary[];
  selectedId: string | null;
  setSelectedId: (id: string | null) => void;
  saveNow: () => Promise<boolean>;
  handleNewPage: () => Promise<void>;
  setSidebarCollapsed: (update: (collapsed: boolean) => boolean) => void;
}) {
  const { entries, selectedId, setSelectedId, saveNow, handleNewPage, setSidebarCollapsed } = options;

  // Keyboard shortcuts: J/K navigate, T today, ⌘N new entry, ⌘S save now,
  // ⌘⇧H highlight selection (delegates to highlights strip via selection),
  // ⌘\ toggle sidebar.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const isEditable =
        target instanceof HTMLElement &&
        (target.tagName === 'INPUT' ||
          target.tagName === 'TEXTAREA' ||
          target.isContentEditable);

      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === 's') {
        e.preventDefault();
        void saveNow();
        return;
      }
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === '\\') {
        e.preventDefault();
        setSidebarCollapsed((v) => !v);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === 'n') {
        if (isEditable) return;
        e.preventDefault();
        void handleNewPage();
        return;
      }
      if (!isEditable) {
        if (e.key === 'j' || e.key === 'ArrowDown') {
          if (entries.length === 0) return;
          const idx = entries.findIndex((en) => en.id === selectedId);
          const nextIdx = idx < 0 ? 0 : Math.min(entries.length - 1, idx + 1);
          setSelectedId(entries[nextIdx].id);
          e.preventDefault();
          return;
        }
        if (e.key === 'k' || e.key === 'ArrowUp') {
          if (entries.length === 0) return;
          const idx = entries.findIndex((en) => en.id === selectedId);
          const nextIdx = idx <= 0 ? 0 : idx - 1;
          setSelectedId(entries[nextIdx].id);
          e.preventDefault();
          return;
        }
        if (e.key === 't' || e.key === 'T') {
          const today = entries.find((en) => {
            const d = new Date(en.updatedAt);
            return !Number.isNaN(d.getTime()) &&
              d.toDateString() === new Date().toDateString();
          });
          if (today) {
            setSelectedId(today.id);
            e.preventDefault();
          }
          return;
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  }, [entries, handleNewPage, saveNow, selectedId, setSelectedId, setSidebarCollapsed]);
}
