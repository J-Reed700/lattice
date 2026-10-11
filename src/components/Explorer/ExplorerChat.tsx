import { useState } from 'react';

import { Check, ChevronDown, Loader2, MessageSquarePlus, MoreHorizontal, Trash2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { IconButton } from '@/components/ui/IconButton';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { ChatPanel } from '@/features/chat/components/ChatPanel';
import { cn } from '@/lib/utils';


import type { ExplorerThreads } from './useExplorerThread';

function relativeDay(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/** The right column: this folder's threads and the ordinary chat. */
export function ExplorerChat({ threads }: { threads: ExplorerThreads }) {
  const [open, setOpen] = useState(false);
  const [actionsOpen, setActionsOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; title: string } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const active = threads.threads.find((thread) => thread.id === threads.activeId);

  const deleteConversation = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    setDeleteError(null);
    try {
      if (await threads.remove(deleteTarget.id)) setDeleteTarget(null);
      else setDeleteError('The conversation could not be deleted. Try again.');
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-1 border-b border-border-subtle bg-chrome px-2">
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>
            <button
              type="button"
              disabled={threads.threads.length === 0}
              className="row-hover inline-flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md px-2 text-left text-[13px] text-text-primary disabled:opacity-60"
            >
              <span className="truncate">{active?.title ?? (threads.preparing ? 'Opening thread…' : 'No thread')}</span>
              {threads.threads.length > 1 && <span className="shrink-0 text-[11px] tabular-nums text-text-muted">{threads.threads.length}</span>}
              <ChevronDown className="ml-auto h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
            </button>
          </PopoverTrigger>
          <PopoverContent align="start" className="max-h-80 w-72 overflow-y-auto p-1">
            <p className="px-2.5 pb-1 pt-1.5 text-[11px] font-medium uppercase tracking-[.08em] text-text-muted">Threads in this folder</p>
            {threads.threads.map((thread) => (
              <button
                key={thread.id}
                type="button"
                onClick={() => {
                  setOpen(false);
                  threads.select(thread.id);
                }}
                className={cn(
                  'flex w-full items-center gap-2 rounded-sm px-2.5 py-1.5 text-left text-[13px] hover:bg-[hsl(var(--text-primary)/0.06)] focus-visible:bg-[hsl(var(--text-primary)/0.06)] focus-visible:outline-hidden',
                  thread.id === threads.activeId ? 'text-text-primary' : 'text-text-secondary',
                )}
              >
                <Check className={cn('h-3.5 w-3.5 shrink-0 text-accent', thread.id !== threads.activeId && 'invisible')} strokeWidth={2} aria-hidden="true" />
                <span className="min-w-0 flex-1 truncate">{thread.title}</span>
                <span className="shrink-0 text-[11px] text-text-muted">{relativeDay(thread.updatedAt)}</span>
              </button>
            ))}
          </PopoverContent>
        </Popover>
        <IconButton label="New thread" onClick={() => void threads.create()} disabled={threads.preparing}>
          <MessageSquarePlus />
        </IconButton>
        <Popover open={actionsOpen} onOpenChange={setActionsOpen}>
          <PopoverTrigger asChild>
            <IconButton label="Conversation actions" disabled={!active || deleting}>
              <MoreHorizontal />
            </IconButton>
          </PopoverTrigger>
          <PopoverContent align="end" className="w-56 p-1.5">
            <button
              type="button"
              className="flex w-full items-center gap-2.5 rounded-sm px-2.5 py-2 text-left text-sm text-danger-fg transition-colors hover:bg-danger-muted focus-visible:bg-danger-muted focus-visible:outline-hidden"
              onClick={() => {
                setActionsOpen(false);
                setDeleteError(null);
                if (active) setDeleteTarget({ id: active.id, title: active.title });
              }}
            >
              <Trash2 className="h-4 w-4" />
              Delete conversation…
            </button>
          </PopoverContent>
        </Popover>
      </div>
      {threads.activeId ? (
        <ChatPanel />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
          {threads.error ? (
            <>
              <p role="alert" className="max-w-xs text-sm text-text-secondary">{threads.error}</p>
              <button type="button" onClick={threads.retry} className="rounded-md border border-border-subtle px-3 py-1.5 text-[13px] text-text-primary hover:bg-[hsl(var(--text-primary)/0.05)]">Try again</button>
            </>
          ) : (
            <p className="text-sm text-text-muted">Opening this folder’s thread…</p>
          )}
        </div>
      )}
      {deleteTarget && (
        <Dialog open onOpenChange={(next) => !next && !deleting && setDeleteTarget(null)}>
          <DialogContent className="sm:max-w-md">
            <DialogHeader>
              <DialogTitle>Delete this conversation?</DialogTitle>
              <DialogDescription>
                &ldquo;{deleteTarget.title}&rdquo; and all of its messages will be permanently deleted. This can&apos;t be undone.
              </DialogDescription>
            </DialogHeader>
            {deleteError && <p role="alert" className="text-sm text-danger-fg">{deleteError}</p>}
            <DialogFooter>
              <Button type="button" variant="outline" disabled={deleting} onClick={() => setDeleteTarget(null)}>Cancel</Button>
              <Button type="button" variant="destructive" disabled={deleting} onClick={() => void deleteConversation()}>
                {deleting ? <><Loader2 className="h-4 w-4 animate-spin" />Deleting…</> : 'Delete conversation'}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
    </div>
  );
}
