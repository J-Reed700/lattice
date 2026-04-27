import { useState } from 'react';

import { confirm as tauriConfirm } from '@tauri-apps/plugin-dialog';
import { BookOpen, MoreHorizontal, Pencil, Plus, Trash2 } from 'lucide-react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { ConversationJournalDto } from '@/types/api/conversation';

interface JournalPickerMenuProps {
  journals: ConversationJournalDto[];
  currentJournal: ConversationJournalDto | null;
  onSwitch: (journalId: string) => void;
  onCreate: () => void;
  onRenameCurrent: (nextName: string) => Promise<void> | void;
  onDeleteCurrent: () => Promise<void> | void;
}

const NEW_JOURNAL_VALUE = '__new-journal__';

/**
 * Sidebar journal picker: shadcn Select for switching + a More menu (Popover)
 * for rename/delete of the current journal. Replaces the bare native select
 * + new-journal button.
 * Spec §4.1.2.
 */
export function JournalPickerMenu({
  journals,
  currentJournal,
  onSwitch,
  onCreate,
  onRenameCurrent,
  onDeleteCurrent,
}: JournalPickerMenuProps) {
  const [isMoreOpen, setIsMoreOpen] = useState(false);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState('');

  const handleValueChange = (value: string) => {
    if (value === NEW_JOURNAL_VALUE) {
      onCreate();
      return;
    }
    onSwitch(value);
  };

  const startRename = () => {
    if (!currentJournal) return;
    setRenameDraft(currentJournal.name);
    setIsRenaming(true);
  };

  const commitRename = async () => {
    const next = renameDraft.trim();
    if (!currentJournal || !next || next === currentJournal.name) {
      setIsRenaming(false);
      return;
    }
    await onRenameCurrent(next);
    setIsRenaming(false);
    setIsMoreOpen(false);
  };

  const confirmDelete = async () => {
    if (!currentJournal) return;
    const confirmed = await tauriConfirm(
      'This permanently deletes the journal and cannot be undone.',
      { title: `Delete "${currentJournal.name}"?`, kind: 'warning' },
    );
    if (!confirmed) return;
    await onDeleteCurrent();
    setIsMoreOpen(false);
  };

  return (
    <div className="flex items-center gap-1.5">
      <div className="min-w-0 flex-1">
        <Select
          value={currentJournal?.id ?? ''}
          onValueChange={handleValueChange}
          disabled={journals.length === 0 && !currentJournal}
        >
          <SelectTrigger className="h-9 text-sm" aria-label="Switch journal">
            <SelectValue placeholder="Select journal">
              {currentJournal && (
                <span className="flex items-center gap-2 truncate">
                  <span className="shrink-0">{currentJournal.icon || '📓'}</span>
                  <span className="truncate">{currentJournal.name}</span>
                </span>
              )}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {journals.map((journal) => (
              <SelectItem key={journal.id} value={journal.id}>
                <span className="flex items-center gap-2">
                  <span>{journal.icon || '📓'}</span>
                  <span>{journal.name}</span>
                </span>
              </SelectItem>
            ))}
            <SelectItem value={NEW_JOURNAL_VALUE}>
              <span className="flex items-center gap-2 text-[hsl(var(--accent))]">
                <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
                New journal…
              </span>
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      <Popover open={isMoreOpen} onOpenChange={(open) => { setIsMoreOpen(open); if (!open) setIsRenaming(false); }}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className="shrink-0 rounded-sm p-1.5 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast disabled:opacity-40"
            aria-label="Journal actions"
            title="Journal actions"
            disabled={!currentJournal}
          >
            <MoreHorizontal className="h-4 w-4" strokeWidth={1.75} />
          </button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-56 p-2">
          {isRenaming ? (
            <div className="space-y-2">
              <label className="block text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
                Rename journal
              </label>
              <input
                autoFocus
                value={renameDraft}
                onChange={(e) => setRenameDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    void commitRename();
                  } else if (e.key === 'Escape') {
                    e.preventDefault();
                    setIsRenaming(false);
                  }
                }}
                className="w-full rounded-sm border border-[hsl(var(--border-default))] bg-[hsl(var(--surface))] px-2 py-1.5 text-sm text-[hsl(var(--text-primary))] outline-none focus:border-[hsl(var(--accent))]"
                maxLength={120}
              />
              <div className="flex items-center justify-end gap-1.5">
                <button
                  type="button"
                  onClick={() => setIsRenaming(false)}
                  className="rounded-sm px-2 py-1 text-xs text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={() => void commitRename()}
                  className="rounded-sm bg-[hsl(var(--accent))] px-2 py-1 text-xs font-medium text-[hsl(var(--accent-fg))] hover:bg-[hsl(var(--accent-hover))] transition-colors duration-fast"
                >
                  Save
                </button>
              </div>
            </div>
          ) : (
            <div className="flex flex-col">
              <button
                type="button"
                onClick={startRename}
                disabled={!currentJournal}
                className="flex items-center gap-2 rounded-sm px-2 py-1.5 text-sm text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--surface))] transition-colors duration-fast disabled:opacity-40"
              >
                <Pencil className="h-3.5 w-3.5" strokeWidth={1.75} />
                Rename journal
              </button>
              <button
                type="button"
                onClick={() => {
                  onCreate();
                  setIsMoreOpen(false);
                }}
                className="flex items-center gap-2 rounded-sm px-2 py-1.5 text-sm text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--surface))] transition-colors duration-fast"
              >
                <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
                New journal…
              </button>
              <div className="my-1 h-px bg-[hsl(var(--border-subtle))]" />
              <button
                type="button"
                onClick={() => void confirmDelete()}
                disabled={!currentJournal}
                className="flex items-center gap-2 rounded-sm px-2 py-1.5 text-sm text-[hsl(var(--danger-fg))] hover:bg-[hsl(var(--danger-muted))] transition-colors duration-fast disabled:opacity-40"
              >
                <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
                Delete journal…
              </button>
            </div>
          )}
        </PopoverContent>
      </Popover>
    </div>
  );
}

export { BookOpen };
