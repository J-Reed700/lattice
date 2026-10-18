import { useId, useState, type ReactNode, type Ref } from 'react';

import { ChevronDown, LoaderCircle } from 'lucide-react';

import { Passage } from '@/features/learning/lessons/LessonEvidencePanel';
import { ClaimEvidenceAttach } from '@/features/learning/lessons/review/ClaimEvidenceAttach';
import { ClaimHistory } from '@/features/learning/lessons/review/ClaimHistory';
import {
  ACTION_ORDER,
  addedByLabel,
  checkerColumns,
  confirmNeedsNote,
  repairOutcomeLabel,
  stateLabel,
  unitLabel,
  verdictLabel,
  verdictMeaning,
  type CheckerSide,
} from '@/features/learning/lessons/review/claimReviewPresentation';
import { ClaimSufficiency } from '@/features/learning/sources/trust/ClaimSufficiency';
import type {
  ClaimActionKindDto,
  ClaimActionRequestDto,
  ClaimAttachReportDto,
  ClaimReviewItemDto,
  LearningEvidencePassageDto,
} from '@/lib/bindings';

export type ClaimActionHandler = (
  item: ClaimReviewItemDto,
  request: ClaimActionRequestDto,
  kind: ClaimActionKindDto,
) => Promise<ClaimAttachReportDto | null>;

type Form = 'confirm' | 'edit' | 'research' | 'attach' | null;

const ACTION_LABELS: Record<ClaimActionKindDto, string> = {
  confirm: 'Confirm', edit: 'Edit the sentence', remove: 'Remove', general_knowledge: 'Mark as general knowledge',
  attach_evidence: 'Add evidence', research_with_hint: 'Research with a hint', unconfirm: 'Unconfirm', undo: 'Undo',
};

const DIRECT: Partial<Record<ClaimActionKindDto, ClaimActionRequestDto>> = {
  remove: { action: 'remove' },
  general_knowledge: { action: 'general_knowledge' },
  unconfirm: { action: 'unconfirm' },
  undo: { action: 'undo' },
};

function date(at: number) {
  return new Date(at).toLocaleDateString([], { day: 'numeric', month: 'short' });
}

const passageKey = (passage: { sourceVersionId: string; startByte: number; endByte: number }) =>
  `${passage.sourceVersionId}:${passage.startByte}:${passage.endByte}`;

function Heading({ children }: { children: ReactNode }) {
  return <h5 className="text-[11px] font-semibold uppercase tracking-[.12em] text-text-muted">{children}</h5>;
}

function CheckerColumn({ title, side, onOpenPassage }: {
  title: string; side: CheckerSide | null; onOpenPassage: (passage: LearningEvidencePassageDto) => void;
}) {
  return (
    <div className="min-w-0 rounded-lg border border-border bg-background p-3">
      <Heading>{title}</Heading>
      {!side ? <p className="mt-2 text-xs text-text-muted">No check recorded.</p> : <>
        <p className="mt-2 text-sm font-semibold text-text-primary">{side.verdict}</p>
        <p className="break-all text-[11px] text-text-muted">{side.model}</p>
        {side.reason && <p className="mt-2 text-xs leading-5 text-text-secondary">{side.reason}</p>}
        {side.supportingQuote && <blockquote className="mt-2 border-l-2 border-accent pl-3 text-xs leading-5 text-text-primary">
          {side.supportingQuote}
          {side.quotedSource && <span className="mt-1 block text-[11px] text-text-muted">— {side.quotedSource.title}{addedByLabel(side.quotedSource.addedBy) ? ` · ${addedByLabel(side.quotedSource.addedBy)}` : ''}</span>}
        </blockquote>}
        {side.passages.length > 0 && <ul className="mt-2 space-y-1 text-xs">
          {side.passages.map(({ passage }) => (
            <li key={passageKey(passage)}>
              <button type="button" onClick={() => onOpenPassage(passage)} className="text-left text-accent underline">{passage.title} · bytes {passage.startByte}–{passage.endByte}</button>
            </li>
          ))}
        </ul>}
      </>}
    </div>
  );
}

/**
 * One claim: the sentence, the honest verdict and reason, the passages
 * compared, what the machine tried, and the user's actions (§10).
 */
export function ClaimReviewCard({
  item, programId, expanded, alwaysOpen = false, headerTabIndex = 0, headerRef, onToggle, onHeaderFocus,
  busy, error, onAction, onOpenPassage, lessonAnchor,
}: {
  item: ClaimReviewItemDto;
  programId: string;
  expanded: boolean;
  /** One claim in the list: no collapsing. */
  alwaysOpen?: boolean;
  headerTabIndex?: number;
  headerRef?: Ref<HTMLButtonElement>;
  onToggle: () => void;
  onHeaderFocus?: () => void;
  busy: boolean;
  error: string | null;
  onAction: ClaimActionHandler;
  onOpenPassage: (passage: LearningEvidencePassageDto) => void;
  /** `lesson-<id>-section-` for a published lesson; null while the lesson is a draft. */
  lessonAnchor?: string | null;
}) {
  const [form, setForm] = useState<Form>(null);
  const [note, setNote] = useState('');
  const [bound, setBound] = useState<string[]>([]);
  const [after, setAfter] = useState(item.location.quote);
  const [hint, setHint] = useState('');
  const [formError, setFormError] = useState('');
  const id = useId();
  const open = alwaysOpen || expanded;
  const needsNote = confirmNeedsNote(item);
  const columns = checkerColumns(item);
  const sources = [...new Map(item.evidence.map(({ passage }) => [passage.sourceVersionId, passage.title])).entries()];
  const actions = ACTION_ORDER.filter((action) => item.availableActions.includes(action));

  const act = async (request: ClaimActionRequestDto, kind: ClaimActionKindDto) => {
    setFormError('');
    try {
      await onAction(item, request, kind);
      setForm(null);
    } catch {
      // The panel shows the refusal on the card (`error`) and reloads the review.
    }
  };

  const choose = (kind: ClaimActionKindDto) => {
    const direct = DIRECT[kind];
    if (direct) { void act(direct, kind); return; }
    const next: Form = kind === 'confirm' ? 'confirm' : kind === 'edit' ? 'edit' : kind === 'research_with_hint' ? 'research' : 'attach';
    setFormError('');
    setForm(form === next ? null : next);
  };

  const submitConfirm = () => {
    if (needsNote && !note.trim()) { setFormError('Write a note saying why the claim is right despite the passage.'); return; }
    void act({ action: 'confirm', note: note.trim() || null, sourceVersionIds: bound }, 'confirm');
  };
  const submitEdit = () => {
    if (!after.trim()) { setFormError('To delete the sentence, choose Remove.'); return; }
    if (after === item.location.quote) { setFormError('Change the sentence first.'); return; }
    void act({ action: 'edit', before: item.location.quote, after }, 'edit');
  };
  const submitHint = () => {
    if (!hint.trim()) { setFormError('Write what to search for.'); return; }
    void act({ action: 'research_with_hint', hint: hint.trim() }, 'research_with_hint');
  };

  const header = (
    <span className="flex min-w-0 flex-1 flex-col gap-1.5">
      <span className="flex flex-wrap items-center gap-2 text-[11px]">
        <span className="rounded-full border border-border bg-background px-2 py-0.5 font-semibold text-text-primary">{verdictLabel(item.verdictKind)}</span>
        <span className="text-text-muted">{stateLabel(item)}</span>
      </span>
      <span className={`wrap-anywhere font-serif text-sm leading-6 text-text-primary ${open ? '' : 'line-clamp-2'}`}>“{item.location.quote}”</span>
    </span>
  );

  return (
    <article aria-labelledby={`${id}-header`} className="rounded-xl border border-border bg-surface">
      {alwaysOpen ? <div id={`${id}-header`} className="flex p-4">{header}</div> : (
        <button
          id={`${id}-header`} ref={headerRef} type="button" tabIndex={headerTabIndex}
          data-claim-header="" data-claim-id={item.claimId}
          aria-expanded={open} aria-controls={`${id}-body`}
          onClick={onToggle} onFocus={onHeaderFocus}
          className="flex w-full items-start gap-3 p-4 text-left focus-visible:outline-2 focus-visible:outline-accent"
        >
          {header}
          <ChevronDown size={16} aria-hidden="true" className={`mt-1 shrink-0 text-text-muted ${open ? 'rotate-180' : ''}`} />
        </button>
      )}
      {open && <div id={`${id}-body`} className="space-y-4 border-t border-border px-4 pb-4 pt-3">
        <div>
          <p className="text-xs text-text-muted">
            {unitLabel(item.location)} · {item.location.field}
            {lessonAnchor && item.location.unitKind === 'block'
              ? <> · <a href={`#${lessonAnchor}${item.location.unitIndex}`} className="text-accent underline">Open in lesson</a></>
              : <> · in the lesson draft, not published yet</>}
          </p>
          {item.statement.trim() !== item.location.quote.trim() && <p className="mt-2 text-sm leading-6 text-text-secondary"><span className="font-medium text-text-primary">Claim checked: </span>{item.statement}</p>}
        </div>

        <div>
          <Heading>Verdict</Heading>
          <p className="mt-1 text-sm text-text-primary"><span className="font-semibold">{verdictLabel(item.verdictKind)}.</span> {verdictMeaning(item.verdictKind)}</p>
          {item.reason && <p className="mt-1 text-sm leading-6 text-text-secondary"><span className="font-medium text-text-primary">Checker’s reason: </span>{item.reason}</p>}
          {item.human && <p className="mt-2 rounded-lg border border-border bg-background p-2 text-xs leading-5 text-text-secondary">
            {item.state.state === 'confirmed' ? 'Confirmed by you' : 'Your earlier verdict'} · {date(item.human.at)}
            {item.human.note && <> · “{item.human.note}”</>}
            {item.human.boundSources.length > 0 && <> · bound to {item.human.boundSources.map((source) => source.title).join(', ')}</>}
          </p>}
        </div>

        {columns && <div role="group" aria-label="Local checker and Verifier" className="grid gap-2 sm:grid-cols-2">
          <CheckerColumn title="Local checker said" side={columns.local} onOpenPassage={onOpenPassage} />
          <CheckerColumn title="Verifier said" side={columns.verifier} onOpenPassage={onOpenPassage} />
        </div>}

        <div>
          <Heading>What was compared</Heading>
          {item.supportingQuote && <blockquote className="mt-2 border-l-2 border-accent pl-3 text-xs leading-5 text-text-primary"><span className="font-medium">Supporting passage: </span>{item.supportingQuote}</blockquote>}
          {item.evidence.length === 0
            ? <p className="mt-1 text-xs text-text-muted">No passage was compared: no saved source mentions this claim’s terms.</p>
            : <div className="mt-2 space-y-2">{item.evidence.map((evidence, index) => (
              <Passage key={passageKey(evidence.passage)} passage={evidence.passage} number={index + 1} defaultOpen readerAlways
                provenance={addedByLabel(evidence.addedBy)}
                onOpenPassage={(passage) => onOpenPassage(passage)} />
            ))}</div>}
        </div>

        <ClaimSufficiency item={item} programId={programId} />

        {(item.research.length > 0 || item.repairs.length > 0) && <div>
          <Heading>What was tried</Heading>
          <ul className="mt-2 space-y-2 text-xs leading-5 text-text-secondary">
            {item.research.map((attempt, index) => <li key={`research-${index}`}>
              {attempt.queries.length ? <>Searched: {attempt.queries.map((query) => `“${query}”`).join(', ')}. </> : <>Research had nothing to search. </>}
              {attempt.sourcesAdded.length
                ? <>Saved {attempt.sourcesAdded.map((source) => source.title).join(', ')}; {attempt.passagesFound} {attempt.passagesFound === 1 ? 'passage' : 'passages'} found.</>
                : <>No page added.</>}
            </li>)}
            {item.repairs.map((repair) => <li key={`repair-${repair.attempt}-${repair.at}`}>
              <span className="font-medium text-text-primary">Repair {repair.attempt}{repair.escalated ? ' (last try)' : ''}: </span>{repairOutcomeLabel(repair.outcome)}
              <span className="mt-1 block wrap-anywhere text-text-muted">“{repair.before}” → {repair.after == null ? 'removed' : `“${repair.after}”`}</span>
            </li>)}
          </ul>
        </div>}

        <div role="toolbar" aria-label="Claim actions" className="flex flex-wrap gap-2">
          {actions.map((kind) => {
            const label = kind === 'confirm' && needsNote ? 'Confirm anyway (note required)' : ACTION_LABELS[kind];
            const toggles = !DIRECT[kind];
            return <button key={kind} type="button" disabled={busy} onClick={() => choose(kind)}
              aria-expanded={toggles ? form === (kind === 'research_with_hint' ? 'research' : kind === 'attach_evidence' ? 'attach' : kind) : undefined}
              className={`min-h-9 rounded-full border px-3 text-xs font-medium disabled:opacity-50 ${kind === 'confirm' ? 'border-accent/50 text-accent' : 'border-border text-text-secondary'}`}>{label}</button>;
          })}
          {busy && <span role="status" className="inline-flex items-center gap-1 text-xs text-text-muted"><LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />Saving…</span>}
        </div>

        {form === 'confirm' && <div className="rounded-xl border border-border bg-background p-4">
          <label htmlFor={`${id}-note`} className="text-xs font-medium text-text-secondary">{needsNote ? 'Why the claim is right (required)' : 'Note (optional)'}</label>
          <textarea id={`${id}-note`} value={note} onChange={(event) => setNote(event.target.value)} rows={3} required={needsNote} aria-describedby={needsNote ? `${id}-note-help` : undefined} className="mt-1 w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
          {needsNote && <p id={`${id}-note-help`} className="mt-1 text-xs text-text-muted">A passage says otherwise, so your note is kept with the claim and shown in its history.</p>}
          {sources.length > 0 && <fieldset className="mt-3"><legend className="text-xs font-medium text-text-secondary">The source that convinced you (optional)</legend>
            {sources.map(([versionId, title]) => <label key={versionId} className="mt-1 flex items-center gap-2 text-xs text-text-secondary">
              <input type="checkbox" checked={bound.includes(versionId)} onChange={(event) => setBound(event.target.checked ? [...bound, versionId] : bound.filter((value) => value !== versionId))} />{title}
            </label>)}
          </fieldset>}
          <button type="button" disabled={busy || (needsNote && !note.trim())} onClick={submitConfirm} className="mt-3 min-h-9 rounded-full bg-accent px-4 text-xs font-semibold text-accent-fg disabled:opacity-50">{needsNote ? 'Confirm with this note' : 'Confirm'}</button>
        </div>}

        {form === 'edit' && <div className="rounded-xl border border-border bg-background p-4">
          <p className="text-xs text-text-muted">Before: “{item.location.quote}”</p>
          <label htmlFor={`${id}-after`} className="mt-2 block text-xs font-medium text-text-secondary">New sentence</label>
          <textarea id={`${id}-after`} value={after} onChange={(event) => setAfter(event.target.value)} rows={3} className="mt-1 w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
          <p className="mt-1 text-xs text-text-muted">The new sentence is checked against your references at Resume.</p>
          <button type="button" disabled={busy} onClick={submitEdit} className="mt-3 min-h-9 rounded-full bg-accent px-4 text-xs font-semibold text-accent-fg disabled:opacity-50">Save the edit</button>
        </div>}

        {form === 'research' && <div className="rounded-xl border border-border bg-background p-4">
          <label htmlFor={`${id}-hint`} className="text-xs font-medium text-text-secondary">What to search for</label>
          <input id={`${id}-hint`} value={hint} onChange={(event) => setHint(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') submitHint(); }} className="mt-1 w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
          <p className="mt-1 text-xs text-text-muted">Your words are searched as written, then the claim is checked against what is found.</p>
          <button type="button" disabled={busy} onClick={submitHint} className="mt-3 min-h-9 rounded-full bg-accent px-4 text-xs font-semibold text-accent-fg disabled:opacity-50">Research</button>
        </div>}

        {form === 'attach' && <ClaimEvidenceAttach programId={programId} onCancel={() => setForm(null)}
          onSubmit={(request) => onAction(item, request, 'attach_evidence')} />}

        {(formError || error) && <p role="alert" className="text-xs leading-5 text-rose-700">{formError || error}</p>}

        <details>
          <summary className="cursor-pointer text-xs font-medium text-text-secondary">History ({item.history.length})</summary>
          <div className="mt-2"><ClaimHistory history={item.history} /></div>
        </details>
      </div>}
    </article>
  );
}
