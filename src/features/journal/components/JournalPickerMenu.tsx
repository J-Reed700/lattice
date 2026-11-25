import { useState } from 'react';

import { BookOpen, MoreHorizontal, Pencil, Plus, Trash2 } from 'lucide-react';

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  IconButton,
  Input,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui';
import type { ConversationJournalDto } from '@/types/api/conversation';

interface JournalPickerMenuProps {
  journals: ConversationJournalDto[];
  currentJournal: ConversationJournalDto | null;
  onSwitch: (journalId: string) => void;
  /** One quiet line under the name, e.g. "4 pages · 3 entries". */
  meta?: string;
  onCreate: () => void;
  onRenameCurrent: (nextName: string) => Promise<void> | void;
  onDeleteCurrent: () => Promise<void> | void;
}

const NEW_JOURNAL_VALUE = '__new-journal__';

/**
 * Sidebar journal picker: Radix Select for switching + a More menu (Popover)
 * for rename/delete of the current journal, with a clean Radix Dialog
 * confirmation before deletion.
 */
export function JournalPickerMenu({
  journals,
  currentJournal,
  onSwitch,
  meta,
  onCreate,
  onRenameCurrent,
  onDeleteCurrent,
}: JournalPickerMenuProps) {
  const [isMoreOpen, setIsMoreOpen] = useState(false);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState('');
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

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

  const handleConfirmDelete = async () => {
    if (!currentJournal || isDeleting) return;
    setIsDeleting(true);
    try {
      await onDeleteCurrent();
      setIsDeleteDialogOpen(false);
      setIsMoreOpen(false);
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <div className="flex items-center gap-1">
      <div className="min-w-0 flex-1">
        <Select
          value={currentJournal?.id ?? ''}
          onValueChange={handleValueChange}
          disabled={journals.length === 0 && !currentJournal}
        >
          <SelectTrigger
            className="row-hover h-auto rounded-lg border-0 bg-transparent px-1.5 py-1.5 shadow-none hover:bg-transparent [&>span]:line-clamp-none"
            aria-label="Switch journal"
          >
            <SelectValue placeholder="Select journal">
              {currentJournal && (
                <span className="flex min-w-0 items-center gap-2.5 text-left">
                  <span
                    aria-hidden="true"
                    className="relative flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border-subtle bg-surface text-sm text-text-secondary shadow-sm"
                  >
                    {currentJournal.icon ? (
                      <span>{currentJournal.icon}</span>
                    ) : (
                      <BookOpen className="h-4 w-4 text-text-secondary" strokeWidth={1.6} />
                    )}
                    {currentJournal.accentColor ? (
                      <span
                        className="absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-surface"
                        style={{ backgroundColor: currentJournal.accentColor }}
                      />
                    ) : null}
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate font-sans text-[14px] font-semibold leading-tight tracking-[-0.015em] text-text-primary">
                      {currentJournal.name}
                    </span>
                    {meta ? <span className="mt-0.5 block truncate text-[11px] text-text-muted">{meta}</span> : null}
                  </span>
                </span>
              )}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {journals.map((journal) => (
              <SelectItem key={journal.id} value={journal.id}>
                <span className="flex items-center gap-2">
                  {journal.icon ? (
                    <span>{journal.icon}</span>
                  ) : (
                    <BookOpen className="h-3.5 w-3.5 text-text-tertiary" strokeWidth={1.6} aria-hidden="true" />
                  )}
                  <span>{journal.name}</span>
                </span>
              </SelectItem>
            ))}
            <SelectItem value={NEW_JOURNAL_VALUE}>
              <span className="flex items-center gap-2 text-accent">
                <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
                New journal…
              </span>
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      <Popover open={isMoreOpen} onOpenChange={(open) => { setIsMoreOpen(open); if (!open) setIsRenaming(false); }}>
        <PopoverTrigger asChild>
          <IconButton label="Journal actions" disabled={!currentJournal}>
            <MoreHorizontal />
          </IconButton>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-60 rounded-xl p-2">
          {isRenaming ? (
            <div className="space-y-2.5 p-1">
              <label className="block text-xxs uppercase tracking-[0.08em] text-text-muted">
                Rename journal
              </label>
              <Input
                autoFocus
                size="sm"
                aria-label="Journal name"
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
              />
              <div className="flex items-center justify-end gap-1.5">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setIsRenaming(false)}
                >
                  Cancel
                </Button>
                <Button
                  type="button"
                  size="sm"
                  onClick={() => void commitRename()}
                >
                  Save
                </Button>
              </div>
            </div>
          ) : (
            <div className="flex flex-col">
              <button
                type="button"
                onClick={startRename}
                disabled={!currentJournal}
                className="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-ui text-text-primary transition-colors duration-fast hover:bg-surface-sunken disabled:opacity-40"
              >
                <Pencil className="h-3.5 w-3.5 text-text-tertiary" strokeWidth={1.75} />
                Rename journal
              </button>
              <button
                type="button"
                onClick={() => {
                  onCreate();
                  setIsMoreOpen(false);
                }}
                className="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-ui text-text-primary transition-colors duration-fast hover:bg-surface-sunken"
              >
                <Plus className="h-3.5 w-3.5 text-text-tertiary" strokeWidth={1.75} />
                New journal…
              </button>
              <div className="my-1 h-px bg-border-subtle" />
              <button
                type="button"
                onClick={() => {
                  setIsMoreOpen(false);
                  setIsDeleteDialogOpen(true);
                }}
                disabled={!currentJournal}
                className="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-ui text-danger-fg transition-colors duration-fast hover:bg-danger-muted disabled:opacity-40"
              >
                <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
                Delete journal…
              </button>
            </div>
          )}
        </PopoverContent>
      </Popover>

      <Dialog open={isDeleteDialogOpen} onOpenChange={setIsDeleteDialogOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Delete &ldquo;{currentJournal?.name}&rdquo;?</DialogTitle>
            <DialogDescription>
              This permanently deletes the journal and cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setIsDeleteDialogOpen(false)}
              disabled={isDeleting}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={() => void handleConfirmDelete()}
              disabled={isDeleting}
            >
              {isDeleting ? 'Deleting…' : 'Delete journal'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export { BookOpen };
