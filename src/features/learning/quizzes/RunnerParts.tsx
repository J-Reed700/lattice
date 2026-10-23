import type { KeyboardEvent, ReactNode } from 'react';

import { ArrowLeft, Check, Clock3, CloudOff, LoaderCircle } from 'lucide-react';

import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { isAnswered, type FormItem, type ItemDraft } from '@/features/learning/quizzes/items/itemResponse';
import { moveRoving } from '@/features/learning/quizzes/items/roving';
import type { SaveState } from '@/features/learning/quizzes/useQuizAutosave';

export const parchmentBand = 'border-b border-[#d7c9b8] bg-[#eee8df] dark:border-white/10 dark:bg-[#28251f]';
export const eyebrow = 'text-[10px] font-semibold uppercase tracking-[.15em] text-accent';
export const primaryPill = 'inline-flex min-h-11 items-center justify-center gap-2 rounded-full bg-accent px-5 text-sm font-semibold text-accent-fg shadow-sm transition hover:brightness-95 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-45';
export const quietPill = 'inline-flex min-h-11 items-center justify-center gap-2 rounded-full border border-border px-4 text-sm font-medium text-text-secondary transition hover:bg-background hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-45';

export function BackLink({ label, onClick, disabled }: { label: string; onClick: () => void; disabled?: boolean }) {
  return <button type="button" onClick={onClick} disabled={disabled} className="inline-flex min-h-11 items-center gap-2 rounded-full pr-3 text-sm font-medium text-text-secondary hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50">
    <ArrowLeft size={16} aria-hidden="true" /> {label}
  </button>;
}

const saveWords: Record<SaveState, { label: string; icon: ReactNode }> = {
  saved: { label: 'Autosaved as you go', icon: <Check size={13} aria-hidden="true" /> },
  unsaved: { label: 'Saving shortly…', icon: <Clock3 size={13} aria-hidden="true" /> },
  saving: { label: 'Saving…', icon: <LoaderCircle size={13} aria-hidden="true" className="animate-spin motion-reduce:animate-none" /> },
  error: { label: 'Not saved yet', icon: <CloudOff size={13} aria-hidden="true" /> },
};

/** Autosave status, announced politely; never alarming while it works. */
export function SaveStatus({ state }: { state: SaveState }) {
  const words = saveWords[state];
  return <p role="status" aria-live="polite" className={`inline-flex items-center gap-1.5 text-xs ${state === 'error' ? 'font-semibold text-rose-700 dark:text-rose-300' : 'text-text-muted'}`}>{words.icon}{words.label}</p>;
}

/** The question rail: arrows move along it, Enter opens a question. Each button says its state in words. */
export function QuestionRail({ items, drafts, current, onSelect }: { items: FormItem[]; drafts: Record<string, ItemDraft>; current: number; onSelect: (index: number) => void }) {
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => { moveRoving(event, 'button', 'horizontal'); };
  return <nav aria-label="Questions">
    <ol data-roving className="flex flex-wrap gap-2">{items.map((item, index) => {
      const answered = isAnswered(item, drafts[item.id]);
      const here = index === current;
      const label = `Question ${index + 1}, ${answered ? 'answered' : 'not answered yet'}${item.previouslyExposed ? ', seen before' : ''}`;
      return <li key={item.id}>
        <button type="button" aria-label={label} aria-current={here ? 'step' : undefined} tabIndex={here ? 0 : -1} onClick={() => onSelect(index)} onKeyDown={onKeyDown}
          className={`grid h-11 w-11 place-items-center rounded-full text-sm font-semibold tabular-nums transition focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 ${here ? 'bg-accent text-accent-fg shadow-sm' : answered ? 'bg-accent/12 text-accent hover:bg-accent/20' : 'border border-border bg-surface text-text-muted hover:border-accent/40 hover:text-text-primary'}`}>
          {answered && !here ? <Check size={16} strokeWidth={2.5} aria-hidden="true" /> : index + 1}
        </button>
      </li>;
    })}</ol>
  </nav>;
}

/** The existing alert-dialog pattern, for submit and pause. */
export function ConfirmDialog({ open, title, body, confirmLabel, pending, onConfirm, onCancel }: {
  open: boolean; title: string; body: string; confirmLabel: string; pending: boolean; onConfirm: () => void; onCancel: () => void;
}) {
  return <Dialog open={open} onOpenChange={(next) => { if (!next) onCancel(); }}>
    {open && <DialogContent role="alertdialog" hideClose overlayClassName="bg-black/45" className="block max-w-md rounded-2xl border border-border bg-surface p-6 shadow-2xl" onInteractOutside={(event) => event.preventDefault()}>
      <DialogTitle asChild><h3 className="font-serif text-xl text-text-primary">{title}</h3></DialogTitle>
      <DialogDescription asChild><p className="mt-2 text-sm leading-6 text-text-secondary">{body}</p></DialogDescription>
      <div className="mt-5 flex justify-end gap-2">
        <button type="button" autoFocus onClick={onCancel} className={quietPill}>Keep working</button>
        <button type="button" disabled={pending} onClick={onConfirm} className={primaryPill}>{pending ? 'Saving…' : confirmLabel}</button>
      </div>
    </DialogContent>}
  </Dialog>;
}
