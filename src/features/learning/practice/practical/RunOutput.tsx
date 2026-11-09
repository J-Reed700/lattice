import { dateText, statusStyle } from '@/features/learning/practice/practical/practicalWorkbenchHelpers';
import { CaseResults, type CaseRowItem } from '@/features/learning/practice/shared/CaseResults';
import type { LearningPracticalRunDto } from '@/lib/bindings';

export function RunOutput({
  output,
  cancelling,
  onCancel,
}: {
  output: LearningPracticalRunDto;
  cancelling: boolean;
  onCancel: () => void;
}) {
  const cases: CaseRowItem[] = output.checks.map((check, index) => {
    const detail = (check as {
      detail?: {
        caseId?: string;
        args?: string[];
        expected?: string | null;
        actual?: string | null;
        printed?: string;
        error?: string | null;
      } | null;
    }).detail;

    // Apply §6.3 vocabulary: 'Checks' instead of 'Contained evaluator'
    const name = check.name === 'Contained evaluator' ? 'Checks' : check.name;

    return {
      id: detail?.caseId ?? `check-${index}`,
      name,
      status:
        check.status === 'passed' || check.status === 'failed' || check.status === 'error'
          ? check.status
          : 'ran',
      args: detail?.args ?? null,
      expected: detail?.expected ?? null,
      actual: detail?.actual ?? null,
      printed: detail?.printed || null,
      error: detail?.error ?? null,
      durationMs: check.durationMs,
      message: check.message,
    };
  });

  return (
    <section
      aria-label="Run output"
      className="space-y-4 rounded-2xl border border-border-default bg-surface p-4 sm:p-5"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <span
            className={`rounded-full px-2.5 py-1 text-xs font-semibold capitalize ${statusStyle(output.status)}`}
          >
            {output.status === 'interrupted'
              ? 'Interrupted · partial output'
              : output.status.replace('_', ' ')}
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

      {output.outputTruncated && (
        <p className="rounded-lg bg-warning-muted p-2 text-xs text-warning-fg">
          Output was truncated by the configured runtime limit.
        </p>
      )}

      <CaseResults
        cases={cases}
        stdout={output.stdout}
        stderr={output.stderr}
      />
    </section>
  );
}
