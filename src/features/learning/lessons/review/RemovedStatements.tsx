import { useId, useState } from 'react';

import { PlainError } from '@/components/PlainError';
import { Button } from '@/components/ui/button';
import type { RewrittenSentence, TakenOutSentence } from '@/features/learning/lessons/review/claimReviewPresentation';
import { ReviewCheckbox, SelectionBar } from '@/features/learning/lessons/review/SelectionBar';
import { useReviewSelection } from '@/features/learning/lessons/review/useReviewSelection';
import type { ReviewSectionDto } from '@/lib/bindings';

const WHERE_IT_WAS = 'where-it-was';
const NONE = '';

export const sectionLabel = (section: ReviewSectionDto) => (section.title.trim() ? `Section ${section.index + 1}: ${section.title.trim()}` : `Section ${section.index + 1}`);

/** "Put it back at the end of": where it was, or the end of a section. */
function SectionPicker({ sections, known, value, onChange, disabled }: {
  sections: ReviewSectionDto[];
  known: boolean;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className="flex flex-wrap items-center gap-2 text-sm">
      <label htmlFor={id} className="text-text-secondary">Put it back at the end of</label>
      <select id={id} value={value} disabled={disabled} onChange={(event) => onChange(event.target.value)}
        className="h-9 rounded-md border border-border bg-background px-2 text-sm text-text-primary">
        {known ? <option value={WHERE_IT_WAS}>Where it was</option> : <option value={NONE} disabled>Choose a section</option>}
        {sections.map((section) => <option key={section.index} value={String(section.index)}>{sectionLabel(section)}</option>)}
      </select>
    </div>
  );
}

const sectionOf = (value: string): number | null => (value === WHERE_IT_WAS || value === NONE ? null : Number(value));

function TakenOutRow({ entry, sections, busy, checked, onToggle, onPutBack, onAddSource, onEditSection }: {
  entry: TakenOutSentence;
  sections: ReviewSectionDto[];
  busy: boolean;
  checked: boolean;
  onToggle: (shiftKey: boolean) => void;
  onPutBack: (text: string | null, section: number | null) => void;
  onAddSource: () => void;
  onEditSection: (() => void) | null;
}) {
  const [only] = entry.statements;
  const alone = entry.statements.length === 1 && only?.statement === entry.sentence;
  const known = entry.place === 'known';
  const [place, setPlace] = useState(known ? WHERE_IT_WAS : NONE);
  const [text, setText] = useState<string | null>(null);
  const actionable = entry.claimId !== null;
  const unplaced = !known && place === NONE;
  const editing = text !== null;
  const editBox = useId();
  const editSection = onEditSection && <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={onEditSection}>Edit the section</Button>;
  return (
    <li className="flex gap-3 rounded-xl border border-border p-4">
      {actionable && <ReviewCheckbox checked={checked} label={`Select: ${entry.sentence}`} onToggle={onToggle} disabled={busy} />}
      <div className="min-w-0 flex-1">
        <p className="font-serif text-base leading-6 text-text-primary">{entry.sentence}</p>
        {alone ? <p className="mt-2 text-xs leading-5 text-text-secondary">{only.why}</p> : (
          <ul role="list" aria-label="Statements that stood on this sentence" className="mt-3 space-y-2">
            {entry.statements.map((item) => (
              <li key={item.claimId} className="text-sm leading-5 text-text-primary">
                {item.statement}
                <span className="block text-xs leading-5 text-text-secondary">{item.why}</span>
              </li>
            ))}
          </ul>
        )}
        {actionable && <div className="mt-3 space-y-2">
          {editing && <div>
            <label htmlFor={editBox} className="text-xs font-medium text-text-secondary">The sentence to put back</label>
            <textarea id={editBox} value={text} rows={3} onChange={(event) => setText(event.target.value)}
              className="mt-1 w-full rounded-md border border-border bg-background p-2 font-serif text-base leading-6 text-text-primary" />
          </div>}
          <SectionPicker sections={sections} known={known} value={place} onChange={setPlace} disabled={busy} />
          <div className="flex flex-wrap gap-2">
            <Button type="button" size="sm" disabled={busy || unplaced || (editing && !text.trim())}
              onClick={() => onPutBack(editing ? text.trim() : null, sectionOf(place))}>
              Put it back
            </Button>
            {editing
              ? <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={() => setText(null)}>Cancel</Button>
              : <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => setText(entry.sentence)}>Edit and put back</Button>}
            <Button type="button" size="sm" variant="outline" disabled={busy} onClick={onAddSource}>Add a source</Button>
            {editSection}
          </div>
        </div>}
        {!actionable && editSection && <div className="mt-3">{editSection}</div>}
      </div>
    </li>
  );
}

/**
 * What the checks changed in the lesson (design 3.12): the sentences taken out,
 * each once with every statement that stood on it and why it went, and the
 * sentences rewritten. Each can be put back or its original restored; nothing
 * here waits for the user.
 */
export function RemovedStatements({ sentences, rewritten = [], sections = [], busy = false, error = null, onPutBack, onRestore, onAddSource, editSection }: {
  sentences: TakenOutSentence[];
  rewritten?: RewrittenSentence[];
  sections?: ReviewSectionDto[];
  busy?: boolean;
  error?: Error | null;
  /** Puts back the sentences of these claims, as written or as `text` writes the one. */
  onPutBack?: (claimIds: string[], text: string | null, section: number | null) => void;
  onRestore?: (claimId: string) => void;
  onAddSource?: (claimId: string) => void;
  /** Opens the editor on the section the sentence came out of; null when it has none to edit. */
  editSection?: (entry: TakenOutSentence) => (() => void) | null;
}) {
  const ids = sentences.flatMap((entry) => (entry.claimId ? [entry.claimId] : []));
  const selection = useReviewSelection(ids);
  const [bulkPlace, setBulkPlace] = useState(WHERE_IT_WAS);
  const chosen = sentences.filter((entry) => entry.claimId && selection.isSelected(entry.claimId));
  const bulkNeedsSection = chosen.some((entry) => entry.place === 'choose') && sectionOf(bulkPlace) === null;
  const acting = Boolean(onPutBack);
  return (
    <section aria-label="Statements taken out of the lesson">
      {error && <PlainError error={error} />}
      <h3 className="font-serif text-xl text-text-primary">Taken out of the lesson</h3>
      <p className="mt-1 text-sm leading-6 text-text-secondary">
        The checks did not stand these statements up, so their sentences were taken out. Nothing below is in the lesson now.
      </p>
      {sentences.length === 0 && <p className="mt-4 text-sm text-text-secondary">Nothing was taken out.</p>}
      <ul role="list" className="mt-4 space-y-3">
        {sentences.map((entry) => (
          <TakenOutRow key={entry.key} entry={entry} sections={sections} busy={busy || !acting}
            checked={entry.claimId ? selection.isSelected(entry.claimId) : false}
            onToggle={(shiftKey) => entry.claimId && selection.toggle(entry.claimId, { shiftKey })}
            onPutBack={(text, section) => entry.claimId && onPutBack?.([entry.claimId], text, section)}
            onAddSource={() => entry.claimId && onAddSource?.(entry.claimId)} onEditSection={editSection?.(entry) ?? null} />
        ))}
      </ul>
      {acting && chosen.length > 0 && <div className="sticky bottom-0 z-10 mt-3 space-y-2 bg-surface pt-2">
        <SectionPicker sections={sections} known value={bulkPlace} onChange={setBulkPlace} disabled={busy} />
        <SelectionBar count={chosen.length} noun={['sentence', 'sentences']} busy={busy} onClear={selection.clear}
          actions={[{
            label: 'Put back', disabled: bulkNeedsSection,
            hint: 'Choose a section: the place of some of these is not known.',
            onClick: () => {
              const section = sectionOf(bulkPlace);
              onPutBack?.(chosen.flatMap((entry) => (entry.claimId ? [entry.claimId] : [])), null, section);
            },
          }]} />
      </div>}
      {rewritten.length > 0 && <>
        <h3 className="mt-8 font-serif text-xl text-text-primary">Rewritten in the lesson</h3>
        <p className="mt-1 text-sm leading-6 text-text-secondary">The checks rewrote these sentences. Restore the original to take the rewrite out.</p>
        <ul role="list" aria-label="Sentences the checks rewrote" className="mt-4 space-y-3">
          {rewritten.map((entry) => (
            <li key={entry.claimId} className="rounded-xl border border-border p-4">
              <p className="text-xs font-medium text-text-secondary">It said</p>
              <p className="font-serif text-base leading-6 text-text-primary">{entry.original}</p>
              <p className="mt-2 text-xs font-medium text-text-secondary">It says now</p>
              <p className="font-serif text-base leading-6 text-text-primary">{entry.rewrite}</p>
              {entry.why && <p className="mt-2 text-xs leading-5 text-text-secondary">{entry.why}</p>}
              <Button type="button" size="sm" variant="outline" className="mt-3" disabled={busy || !onRestore} onClick={() => onRestore?.(entry.claimId)}>Restore the original</Button>
            </li>
          ))}
        </ul>
      </>}
    </section>
  );
}
