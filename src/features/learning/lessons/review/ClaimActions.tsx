import { useEffect, useId, useState } from 'react';

import { ChevronDown, LoaderCircle } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { ClaimHistory } from '@/features/learning/lessons/review/ClaimHistory';
import type { AnnouncedAction, FixAction } from '@/features/learning/lessons/review/claimReviewCopy';
import { checkerColumns, confirmNeedsNote, type CheckerSide } from '@/features/learning/lessons/review/claimReviewPresentation';
import type { ClaimActionRequestDto, ClaimReviewItemDto } from '@/lib/bindings';

/** The forms the action row opens in place; the claim stays in view. */
export type ActionForm = 'note' | 'edit' | 'hint' | 'history';

export type ClaimActionRunner = (request: ClaimActionRequestDto, kind: AnnouncedAction) => Promise<boolean>;

function CheckColumn({ title, side }: { title: string; side: CheckerSide | null }) {
  return (
    <div className="min-w-0 rounded-lg border border-border bg-background p-3 text-xs leading-5">
      <p className="font-semibold text-text-primary">{title}</p>
      {!side ? <p className="mt-1 text-text-muted">No check recorded.</p> : <>
        <p className="mt-1 text-text-primary">{side.verdict}</p>
        {side.reason && <p className="mt-1 text-text-secondary">{side.reason}</p>}
        {side.supportingQuote && <blockquote className="mt-1 border-l-2 border-accent pl-2 text-text-primary">{side.supportingQuote}</blockquote>}
      </>}
    </div>
  );
}

/**
 * The claim's actions (§3.5): five buttons in one row, the suggested fix
 * first in weight, then More (common knowledge, a search hint, history).
 * Accept on a contradicted claim asks for a note first.
 */
export function ClaimActions({ item, sharing, suggested, busy, error, form, onFormChange, onAct, onAcceptSource, onAddSource, closest = false }: {
  item: ClaimReviewItemDto;
  /** Waiting claims that rest on the same source as this one, this one included. */
  sharing: number;
  suggested: FixAction;
  busy: boolean;
  error: string | null;
  form: ActionForm | null;
  onFormChange: (form: ActionForm | null) => void;
  onAct: ClaimActionRunner;
  onAcceptSource: () => void;
  onAddSource: () => void;
  /** What the user added does not say this: Edit comes first, then another source. */
  closest?: boolean;
}) {
  const id = useId();
  const [note, setNote] = useState('');
  const [after, setAfter] = useState(item.presentation.sentence);
  const [hint, setHint] = useState('');
  const [formError, setFormError] = useState('');
  const [moreOpen, setMoreOpen] = useState(false);
  useEffect(() => { setNote(''); setAfter(item.presentation.sentence); setHint(''); setFormError(''); setMoreOpen(false); }, [item.claimId, item.presentation.sentence]);

  const available = new Set(item.availableActions);
  const supporting = item.support?.role === 'supports' ? item.support : null;
  const needsNote = confirmNeedsNote(item);
  const columns = checkerColumns(item);
  const confirmed = item.state.state === 'confirmed';
  const primary: FixAction = closest ? 'edit' : suggested;
  const variant = (action: FixAction) => (action === primary ? 'default' : 'outline');

  const run = async (request: ClaimActionRequestDto, kind: AnnouncedAction) => {
    setFormError('');
    if (await onAct(request, kind)) onFormChange(null);
  };
  const accept = () => {
    if (needsNote && form !== 'note') { onFormChange('note'); return; }
    if (needsNote && !note.trim()) { setFormError('Write a note saying why the claim is right despite the source.'); return; }
    void run({ action: 'confirm', note: note.trim() || null, sourceVersionIds: supporting ? [supporting.source.sourceVersionId] : [] }, 'confirm');
  };
  const saveEdit = () => {
    if (!after.trim()) { setFormError('To delete the sentence, choose Remove it.'); return; }
    if (after.trim() === item.presentation.sentence.trim()) { setFormError('Change the sentence first.'); return; }
    void run({ action: 'edit', before: item.presentation.sentence, after: after.trim() }, 'edit');
  };
  const search = () => {
    if (!hint.trim()) { setFormError('Write what to search for.'); return; }
    void run({ action: 'research_with_hint', hint: hint.trim() }, 'research_with_hint');
  };
  const toggle = (next: ActionForm) => { setFormError(''); setMoreOpen(false); onFormChange(form === next ? null : next); };

  return (
    <div className="space-y-3">
      <div role="toolbar" aria-label="Claim actions" className="flex flex-wrap gap-2">
        {available.has('confirm') && <Button type="button" size="lg" variant={variant('accept')} disabled={busy} onClick={accept} aria-keyshortcuts="A">
          {needsNote ? 'Accept with a note' : 'Accept'}
        </Button>}
        {supporting && <Button type="button" size="lg" variant={variant('accept_source')} disabled={busy} onClick={onAcceptSource} aria-keyshortcuts="Shift+A">
          {sharing >= 2 ? `Accept this source for all ${sharing}` : 'Accept this source for the course'}
        </Button>}
        {available.has('edit') && <Button type="button" size="lg" variant={variant('edit')} disabled={busy} aria-expanded={form === 'edit'} onClick={() => toggle('edit')} aria-keyshortcuts="E">Edit the sentence</Button>}
        {available.has('remove') && <Button type="button" size="lg" variant={primary === 'remove' ? 'destructive' : 'outline'} disabled={busy} onClick={() => void run({ action: 'remove' }, 'remove')} aria-keyshortcuts="R"
          className={primary === 'remove' ? undefined : 'text-danger-fg'}>Remove it</Button>}
        {available.has('attach_evidence') && <Button type="button" size="lg" variant={variant('add_source')} disabled={busy} onClick={onAddSource} aria-keyshortcuts="S">
          {closest ? 'Add another source' : 'Add a source'}
        </Button>}
        <div className="relative">
          <Button type="button" size="lg" variant="ghost" aria-haspopup="menu" aria-expanded={moreOpen} aria-controls={`${id}-more`} onClick={() => setMoreOpen((open) => !open)}>
            More <ChevronDown aria-hidden="true" />
          </Button>
          {moreOpen && <div id={`${id}-more`} role="menu" aria-label="More actions" className="absolute left-0 top-full z-20 mt-1 w-56 rounded-xl border border-border bg-surface p-1.5 shadow-sheet">
            {available.has('general_knowledge') && <button type="button" role="menuitem" disabled={busy} onClick={() => { setMoreOpen(false); void run({ action: 'general_knowledge' }, 'general_knowledge'); }}
              className="flex min-h-10 w-full items-center rounded-lg px-3 text-left text-sm text-text-primary hover:bg-background disabled:opacity-50">General knowledge</button>}
            {available.has('research_with_hint') && <button type="button" role="menuitem" onClick={() => toggle('hint')}
              className="flex min-h-10 w-full items-center rounded-lg px-3 text-left text-sm text-text-primary hover:bg-background">Search with a hint</button>}
            {confirmed && available.has('unconfirm') && <button type="button" role="menuitem" disabled={busy} onClick={() => { setMoreOpen(false); void run({ action: 'unconfirm' }, 'unconfirm'); }}
              className="flex min-h-10 w-full items-center rounded-lg px-3 text-left text-sm text-text-primary hover:bg-background disabled:opacity-50">Withdraw acceptance</button>}
            <button type="button" role="menuitem" onClick={() => toggle('history')}
              className="flex min-h-10 w-full items-center rounded-lg px-3 text-left text-sm text-text-primary hover:bg-background">History ({item.history.length})</button>
          </div>}
        </div>
        {busy && <span role="status" className="inline-flex items-center gap-1 self-center text-xs text-text-muted"><LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />Saving…</span>}
      </div>

      {form === 'note' && <div className="rounded-xl border border-border bg-background p-4">
        <label htmlFor={`${id}-note`} className="text-xs font-medium text-text-secondary">Why the claim is right (required)</label>
        <textarea id={`${id}-note`} autoFocus value={note} onChange={(event) => setNote(event.target.value)} rows={3} required className="mt-1 w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
        <p className="mt-1 text-xs text-text-muted">A source says otherwise, so your note is kept with the claim and shown in its history.</p>
        <div className="mt-3 flex gap-2">
          <Button type="button" disabled={busy || !note.trim()} onClick={accept}>Accept with this note</Button>
          <Button type="button" variant="ghost" onClick={() => onFormChange(null)}>Cancel</Button>
        </div>
      </div>}

      {form === 'edit' && <div className="rounded-xl border border-border bg-background p-4">
        <label htmlFor={`${id}-after`} className="block text-xs font-medium text-text-secondary">The sentence</label>
        <textarea id={`${id}-after`} autoFocus value={after} onChange={(event) => setAfter(event.target.value)} rows={3} className="mt-1 w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
        <p className="mt-1 text-xs text-text-muted">The new sentence is checked against your sources when you resume.</p>
        <div className="mt-3 flex gap-2">
          <Button type="button" disabled={busy} onClick={saveEdit}>Save the sentence</Button>
          <Button type="button" variant="ghost" onClick={() => onFormChange(null)}>Cancel</Button>
        </div>
      </div>}

      {form === 'hint' && <div className="rounded-xl border border-border bg-background p-4">
        <label htmlFor={`${id}-hint`} className="text-xs font-medium text-text-secondary">What to search for</label>
        <input id={`${id}-hint`} autoFocus value={hint} onChange={(event) => setHint(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') search(); }} className="mt-1 w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
        <p className="mt-1 text-xs text-text-muted">Your words are searched as written, then the claim is checked against what is found.</p>
        <div className="mt-3 flex gap-2">
          <Button type="button" disabled={busy} onClick={search}>Search</Button>
          <Button type="button" variant="ghost" onClick={() => onFormChange(null)}>Cancel</Button>
        </div>
      </div>}

      {form === 'history' && <div className="space-y-3 rounded-xl border border-border bg-background p-4">
        {columns && <div role="group" aria-label="First check and independent check" className="grid gap-2 sm:grid-cols-2">
          <CheckColumn title="First check" side={columns.local} />
          <CheckColumn title="Independent check" side={columns.verifier} />
        </div>}
        <ClaimHistory history={item.history} />
      </div>}

      {(formError || error) && <p role="alert" className="text-xs leading-5 text-danger-fg">{formError || error}</p>}
    </div>
  );
}
