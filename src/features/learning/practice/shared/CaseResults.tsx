import { AlertCircle, CheckCircle2, Clock, XCircle } from 'lucide-react';
import type { LearningPracticalCheckResultDto } from '@/lib/bindings';

export type CaseRowStatus = 'passed' | 'failed' | 'error' | 'ran' | 'timed_out' | 'not_run';
export type CaseOutcome = CaseRowStatus;

export interface CaseRow {
  id: string;
  label: string;
  outcome: CaseOutcome;
  input?: string;
  expected?: string;
  actual?: string;
  printed?: string;
  error?: string;
  elapsedMs?: number;
}

export interface CaseRowItem {
  id: string;
  name: string;
  status: CaseRowStatus;
  args?: string[] | null;
  expected?: string | null;
  actual?: string | null;
  printed?: string | null;
  error?: string | null;
  durationMs?: number | null;
  message?: string | null;
}

export interface CaseResultsProps {
  cases?: CaseRowItem[];
  rows?: CaseRow[];
  heading?: string;
  running?: boolean;
  output?: { stdout: string; stderr: string; truncated: boolean };
  stdout?: string;
  stderr?: string;
  summary?: string;
  className?: string;
}

export function rowsFromChecks(
  checks: LearningPracticalCheckResultDto[],
  paramNames?: string[],
): CaseRow[] {
  return checks.map((check) => {
    const detail = (check as LearningPracticalCheckResultDto & {
      detail?: {
        caseId: string;
        category: string | null;
        args: string[];
        expected: string | null;
        actual: string | null;
        printed: string;
        error: string | null;
        status: CaseOutcome;
      } | null;
    }).detail;
    if (detail) {
      const input =
        detail.args.length > 0
          ? detail.args
              .map((arg, idx) => (paramNames?.[idx] ? `${paramNames[idx]} = ${arg}` : arg))
              .join(', ')
          : undefined;
      return {
        id: detail.caseId || (check as { id?: string; checkId?: string }).id || (check as { checkId?: string }).checkId || check.name,
        label: detail.category ?? check.name,
        outcome: detail.status,
        input,
        expected: detail.expected ?? undefined,
        actual: detail.actual ?? undefined,
        printed: detail.printed || undefined,
        error: detail.error ?? undefined,
        elapsedMs: check.durationMs ?? undefined,
      };
    }
    const rawStatus = check.status as string;
    const outcome: CaseOutcome =
      rawStatus === 'passed'
        ? 'passed'
        : rawStatus === 'failed'
          ? 'failed'
          : rawStatus === 'timed_out'
            ? 'timed_out'
            : rawStatus === 'error'
              ? 'error'
              : rawStatus === 'ran'
                ? 'ran'
                : 'not_run';
    return {
      id: (check as { id?: string; checkId?: string }).id || (check as { checkId?: string }).checkId || check.name,
      label: check.name,
      outcome,
      error: check.message || undefined,
      elapsedMs: check.durationMs ?? undefined,
    };
  });
}

function statusPriority(status: CaseRowStatus): number {
  switch (status) {
    case 'failed':
    case 'error':
      return 0;
    case 'timed_out':
      return 1;
    case 'not_run':
      return 2;
    case 'ran':
      return 3;
    case 'passed':
      return 4;
  }
}

function caseStatusBadge(status: CaseRowStatus) {
  switch (status) {
    case 'passed':
      return {
        label: 'Passed',
        icon: <CheckCircle2 size={14} className="shrink-0 text-success-fg" aria-hidden="true" />,
        className: 'bg-success-muted text-success-fg',
      };
    case 'failed':
      return {
        label: 'Failed',
        icon: <XCircle size={14} className="shrink-0 text-danger-fg" aria-hidden="true" />,
        className: 'bg-danger-muted text-danger-fg',
      };
    case 'error':
      return {
        label: 'Error',
        icon: <AlertCircle size={14} className="shrink-0 text-danger-fg" aria-hidden="true" />,
        className: 'bg-danger-muted text-danger-fg',
      };
    case 'timed_out':
      return {
        label: 'Timed out',
        icon: <Clock size={14} className="shrink-0 text-warning-fg" aria-hidden="true" />,
        className: 'bg-warning-muted text-warning-fg',
      };
    case 'ran':
      return {
        label: 'Ran',
        icon: <CheckCircle2 size={14} className="shrink-0 text-text-secondary" aria-hidden="true" />,
        className: 'bg-surface-raised text-text-secondary',
      };
    case 'not_run':
      return {
        label: 'Not run',
        icon: <Clock size={14} className="shrink-0 text-text-muted" aria-hidden="true" />,
        className: 'bg-surface-raised text-text-muted',
      };
  }
}

export function CaseResults({
  cases,
  rows,
  heading,
  running,
  output,
  stdout,
  stderr,
  summary,
  className = '',
}: CaseResultsProps) {
  const normalizedCases: CaseRowItem[] =
    cases ??
    (rows ?? []).map((row) => ({
      id: row.id,
      name: row.label,
      status: row.outcome,
      args: row.input ? [row.input] : undefined,
      expected: row.expected,
      actual: row.actual,
      printed: row.printed,
      error: row.error,
      durationMs: row.elapsedMs,
    }));

  // Failures and errors first, then timeouts, preserving relative order
  const sortedCases = [...normalizedCases].sort((a, b) => statusPriority(a.status) - statusPriority(b.status));

  const passedCount = normalizedCases.filter((c) => c.status === 'passed').length;
  const failing = normalizedCases.filter((c) => c.status === 'failed' || c.status === 'error' || c.status === 'timed_out');
  const computedSummary = running
    ? `${heading ? `${heading}: ` : ''}Running…`
    : (summary ??
      `${passedCount} of ${normalizedCases.length} passed${
        failing.length > 0 ? `. Failing: ${failing.map((c) => c.name).join(', ')}` : ''
      }`);

  const effectiveStdout = output?.stdout ?? stdout;
  const effectiveStderr = output?.stderr ?? stderr;

  return (
    <section className={`rounded-2xl border border-border-default bg-surface p-4 sm:p-5 ${className}`}>
      {heading && (
        <h3 className="mb-3 font-serif text-lg text-text-primary">{heading}</h3>
      )}
      {/* Live region for accessibility */}
      <div
        role="status"
        aria-live="polite"
        className="mb-4 rounded-xl border border-border-subtle bg-surface-sunken p-3 text-xs font-medium text-text-primary"
      >
        {computedSummary}
      </div>

      {sortedCases.length > 0 && (
        <div className="space-y-3">
          <h4 className="text-xs font-semibold uppercase tracking-wider text-text-muted">
            Checks
          </h4>
          <ul className="space-y-3">
            {sortedCases.map((caseItem) => {
              const badge = caseStatusBadge(caseItem.status);
              return (
                <li
                  key={caseItem.id}
                  data-case-name={caseItem.name}
                  className="rounded-xl border border-border-subtle bg-surface-sunken p-3 sm:p-4"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-semibold text-text-primary">{caseItem.name}</span>
                      {caseItem.durationMs !== undefined && caseItem.durationMs !== null && (
                        <span className="text-xs tabular-nums text-text-muted">
                          {caseItem.durationMs}ms
                        </span>
                      )}
                    </div>
                    <span
                      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${badge.className}`}
                    >
                      {badge.icon}
                      <span>{badge.label}</span>
                    </span>
                  </div>

                  {caseItem.message && (
                    <p className="mt-2 text-xs text-text-secondary">{caseItem.message}</p>
                  )}

                  {/* Details grid: args, expected, actual, printed, error */}
                  {(caseItem.args?.length ||
                    caseItem.expected !== undefined ||
                    caseItem.actual !== undefined ||
                    caseItem.printed ||
                    caseItem.error) && (
                    <div className="mt-3 space-y-2 border-t border-border-subtle pt-2 font-mono text-xs">
                      {caseItem.args && caseItem.args.length > 0 && (
                        <div>
                          <span className="font-sans font-medium text-text-muted">Input: </span>
                          <span className="text-text-primary">{caseItem.args.join(', ')}</span>
                        </div>
                      )}
                      {caseItem.expected !== undefined && caseItem.expected !== null && (
                        <div>
                          <span className="font-sans font-medium text-text-muted">Expected: </span>
                          <span className="text-success-fg">{caseItem.expected}</span>
                        </div>
                      )}
                      {caseItem.actual !== undefined && caseItem.actual !== null && (
                        <div>
                          <span className="font-sans font-medium text-text-muted">Output: </span>
                          <span className="text-text-primary">{caseItem.actual}</span>
                        </div>
                      )}
                      {caseItem.printed && (
                        <div>
                          <span className="font-sans font-medium text-text-muted">Printed: </span>
                          <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap rounded-lg bg-surface-raised p-2 text-text-secondary">
                            {caseItem.printed}
                          </pre>
                        </div>
                      )}
                      {caseItem.error && (
                        <div className="rounded-lg bg-danger-muted p-2 text-danger-fg">
                          <span className="font-sans font-medium">Error: </span>
                          <span>{caseItem.error}</span>
                        </div>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}

      {/* Stdout and Stderr sections */}
      {(effectiveStdout || effectiveStderr) && (
        <div className="mt-4 grid gap-3 border-t border-border-subtle pt-4 sm:grid-cols-2">
          {effectiveStdout && (
            <div>
              <h5 className="mb-1 text-xs font-semibold uppercase tracking-wider text-text-muted">
                Output
              </h5>
              <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-xl border border-border-subtle bg-surface-sunken p-3 font-mono text-xs text-text-primary">
                {effectiveStdout}
              </pre>
            </div>
          )}
          {effectiveStderr && (
            <div>
              <h5 className="mb-1 text-xs font-semibold uppercase tracking-wider text-text-muted">
                Errors
              </h5>
              <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-xl border border-border-subtle bg-surface-sunken p-3 font-mono text-xs text-danger-fg">
                {effectiveStderr}
              </pre>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
