import { dateText, statusStyle } from '@/features/learning/practice/practical/practicalWorkbenchHelpers';
import { CaseResults, type CaseResultView } from '@/features/learning/practice/shared/CaseResults';
import type { LearningPracticalRunDto } from '@/lib/bindings';

function viewStatus(status: string): CaseResultView['status'] {
  if (status === 'passed' || status === 'failed' || status === 'error' || status === 'timed_out') return status;
  if (status === 'not_run') return 'not_run';
  return 'ran';
}

export function RunOutput({
  output,
  cancelling,
  onCancel,
}: {
  output: LearningPracticalRunDto;
  cancelling: boolean;
  onCancel: () => void;
}) {
  const cases: CaseResultView[] = output.checks.map((check, index) => {
    const detail = check.detail ?? undefined;
    return {
      id: detail?.caseId ?? `check-${index}`,
      category: detail?.category ?? (check.name === 'Contained evaluator' ? 'Checks' : check.name),
      status: viewStatus(check.status),
      args: detail?.args ?? null,
      expected: detail?.expected ?? null,
      actual: detail?.actual ?? null,
      printed: detail?.printed || null,
      error: detail ? (detail.error ?? null) : check.message || null,
      elapsedMs: check.durationMs,
      hidden: detail?.visibility === 'hidden',
      mine: detail?.mine,
    };
  });

  return (
    <section
      aria-label="Run output"
      className="space-y-4 rounded-2xl border border-border-default bg-surface p-4 sm:p-5"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <span className={`rounded-full px-2.5 py-1 text-xs font-semibold capitalize ${statusStyle(output.status)}`}>
            {output.status === 'interrupted' ? 'Interrupted · partial output' : output.status.replace('_', ' ')}
          </span>
          <span className="text-xs text-text-muted">
            {dateText(output.createdAt)} · activity revision {output.activityRevision}
          </span>
        </div>
        {output.status === 'running' && (
          <button
            type="button"
            disabled={cancelling}
            onClick={onCancel}
            className="rounded-full border border-border-default px-3 py-1.5 text-xs text-text-secondary hover:bg-surface-raised"
          >
            {cancelling ? 'Cancelling…' : 'Cancel run'}
          </button>
        )}
      </div>

      <div className="grid gap-3 lg:grid-cols-2">
        <div>
          <h4 className="text-xs font-semibold uppercase tracking-[.14em] text-text-secondary">Output</h4>
          <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap wrap-break-word rounded-xl border border-border-subtle bg-surface-sunken p-3 font-mono text-xs leading-5 text-text-primary">
            {output.stdout || (output.status === 'running' ? 'Waiting for runtime output…' : 'No standard output.')}
          </pre>
        </div>
        <div>
          <h4 className="text-xs font-semibold uppercase tracking-[.14em] text-text-secondary">Errors</h4>
          <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap wrap-break-word rounded-xl border border-border-subtle bg-surface-sunken p-3 font-mono text-xs leading-5 text-text-primary">
            {output.stderr || 'No error output.'}
          </pre>
        </div>
      </div>

      {output.outputTruncated && (
        <p className="rounded-lg bg-warning-muted p-2 text-xs text-warning-fg">
          Output was truncated by the configured runtime limit.
        </p>
      )}

      {cases.length > 0 && <CaseResults cases={cases} title="Checks" />}
    </section>
  );
}
