import { SaveStatus } from '@/components/ui/SaveStatus';
import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import { saveStatusOf, type UseAutosaveResult } from '@/features/learning/practice/shared/useAutosave';

export interface ResponseEditorProps {
  text: string;
  dirty: boolean;
  active: boolean;
  autosave: UseAutosaveResult;
  actionError: string;
  onRetryAction: (() => void) | undefined;
  conflict: boolean;
  busy: boolean;
  submitting: boolean;
  onChange: (value: string) => void;
  onReload: () => void;
  onSubmit: () => void;
}

export function ResponseEditor({
  text,
  dirty,
  active,
  autosave,
  actionError,
  onRetryAction,
  conflict,
  busy,
  submitting,
  onChange,
  onReload,
  onSubmit,
}: ResponseEditorProps) {
  const saveState = saveStatusOf(autosave.state, dirty);
  const saveFailed = autosave.state === 'error' || autosave.state === 'conflict';
  const submitReason = !text.trim() ? 'Write a response to submit it.' : busy && !submitting ? 'Another action is in progress.' : null;

  return (
    <section className="overflow-hidden rounded-2xl border border-border-default bg-surface">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-default px-5 py-4">
        <div>
          <div className="text-xs font-semibold uppercase tracking-[.15em] text-accent">Your artifact</div>
          <h4 className="mt-1 font-serif text-xl text-text-primary">Work through the reasoning</h4>
        </div>
        <SaveStatus state={saveState} />
      </div>
      <label htmlFor="workbench-artifact" className="sr-only">
        Your response
      </label>
      <textarea
        id="workbench-artifact"
        value={text}
        disabled={!active}
        onChange={(event) => onChange(event.target.value)}
        placeholder="Write your response, reasoning, or working here…"
        className="min-h-[250px] w-full resize-y bg-transparent px-5 py-5 text-sm leading-7 text-text-primary placeholder:text-text-muted focus:outline-hidden focus:ring-2 focus:ring-accent disabled:opacity-70 sm:min-h-[330px] sm:px-7"
      />
      <div className="space-y-3 border-t border-border-default bg-surface-sunken px-5 py-3">
        {saveFailed && (
          <StatusLine
            state={{
              kind: 'error',
              message: autosave.error ?? 'Your work could not be saved.',
              retry: autosave.state === 'error' ? () => void autosave.flush() : undefined,
            }}
          />
        )}
        {actionError && <StatusLine state={{ kind: 'error', message: actionError, retry: onRetryAction }} />}
        <div className="flex flex-wrap items-start justify-between gap-3">
          <p className="text-xs leading-5 text-text-muted">
            Your writing autosaves to this attempt. Submitting freezes this version for feedback.
          </p>
          <div className="flex flex-col items-end gap-1.5">
            <div className="flex flex-wrap gap-2">
              {conflict && (
                <button
                  type="button"
                  onClick={onReload}
                  className="rounded-full border border-warning-muted px-3 py-2 text-xs font-semibold text-warning-fg"
                >
                  Reload the saved attempt · keep my text
                </button>
              )}
              {active && (
                <button
                  type="button"
                  disabled={busy || !text.trim()}
                  onClick={onSubmit}
                  className="rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg disabled:opacity-45"
                >
                  {submitting ? 'Submitting…' : 'Submit response'}
                </button>
              )}
            </div>
            {active && submitReason && <p className="text-xs text-text-muted">{submitReason}</p>}
          </div>
        </div>
      </div>
    </section>
  );
}
