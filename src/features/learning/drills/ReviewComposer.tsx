import { useState } from 'react';

import { X } from 'lucide-react';

import { Button } from '@/components/ui/button';
import type { CodeReviewSeverity } from '@/lib/bindings';

import { SEVERITIES, severityLabel } from './severity';

import type { DraftFinding } from './findings';

/**
 * Where the learner writes a comment: on the lines they selected in the code ("Lines 12–14 of api.py"), or on no
 * lines at all. Editing a comment puts its words and severity back here.
 */
export function ReviewComposer({ selection, onClearSelection, editing, onSave, onCancel }: {
  /** "Lines 12–14 of api.py" while lines are selected. */
  selection: string | null;
  onClearSelection: () => void;
  /** The comment being edited; none while a new one is written. */
  editing: DraftFinding | null;
  onSave: (input: { text: string; severity: CodeReviewSeverity | null }) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState(editing?.text ?? '');
  const [severity, setSeverity] = useState<CodeReviewSeverity | null>(editing?.severity ?? null);
  const ready = text.trim().length > 0;

  const save = () => {
    if (!ready) return;
    onSave({ text: text.trim(), severity });
    setText('');
    setSeverity(null);
  };

  return (
    <section aria-label={editing ? 'Edit comment' : 'Add a comment'} className="rounded-2xl border border-border bg-surface p-4">
      <h3 className="text-sm font-medium text-text-primary">{editing ? 'Edit comment' : 'Add a comment'}</h3>
      <div className="mt-2 flex min-h-6 items-center gap-2 text-xs text-text-secondary" aria-live="polite">
        {selection ? (
          <>
            <span className="min-w-0 truncate font-mono text-text-primary">{selection}</span>
            <Button type="button" size="icon" variant="ghost" className="h-6 w-6" aria-label="Clear the selected lines" onClick={onClearSelection}>
              <X aria-hidden="true" />
            </Button>
          </>
        ) : (
          <span>A general comment. Click line numbers in the code to put it on lines.</span>
        )}
      </div>
      <textarea
        aria-label="What is wrong, why it matters and what to do"
        rows={4}
        value={text}
        placeholder="What is wrong, why it matters, and what you would do about it"
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
            event.preventDefault();
            save();
          }
        }}
        className="mt-3 w-full resize-y rounded-xl border border-border bg-background px-3 py-2 text-sm leading-6 text-text-primary outline-hidden focus:border-accent focus:ring-2 focus:ring-accent/15"
      />
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
        <select
          aria-label="Severity"
          value={severity ?? ''}
          onChange={(event) => setSeverity(SEVERITIES.find((known) => known === event.target.value) ?? null)}
          className="rounded-lg border border-border bg-background px-2.5 py-1.5 text-xs text-text-primary outline-hidden focus:border-accent"
        >
          <option value="">No severity</option>
          {SEVERITIES.map((known) => (
            <option key={known} value={known}>{severityLabel(known)}</option>
          ))}
        </select>
        <div className="flex items-center gap-2">
          {editing && <Button type="button" variant="ghost" onClick={onCancel}>Cancel</Button>}
          <Button type="button" disabled={!ready} onClick={save}>{editing ? 'Save comment' : 'Add comment'}</Button>
        </div>
      </div>
    </section>
  );
}
