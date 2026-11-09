import { AlertCircle, CheckCircle2, CircleDashed, EyeOff, Play, Timer, XCircle } from 'lucide-react';

export interface CaseResultView {
  id: string;
  category: string;
  status: 'passed' | 'failed' | 'error' | 'ran' | 'timed_out' | 'not_run';
  args?: string[] | null;
  expected?: string | null;
  actual?: string | null;
  printed?: string | null;
  error?: string | null;
  elapsedMs?: number | null;
  hidden?: boolean;
  mine?: boolean;
}

const FAILING = new Set<CaseResultView['status']>(['failed', 'error', 'timed_out']);
const REST: CaseResultView['status'][] = ['not_run', 'ran', 'passed'];
const rank = (status: CaseResultView['status']) => (FAILING.has(status) ? -1 : REST.indexOf(status));

export function caseSummary(cases: CaseResultView[]): string {
  if (cases.length === 0) return '';
  const checked = cases.filter((c) => c.status !== 'ran');
  if (checked.length === 0) return `Ran ${cases.length} ${cases.length === 1 ? 'input' : 'inputs'}.`;
  const passed = checked.filter((c) => c.status === 'passed').length;
  const failing = checked.filter((c) => FAILING.has(c.status));
  const base = `${passed} of ${checked.length} passed`;
  return failing.length > 0 ? `${base}. Failing: ${failing.map((c) => c.category).join(', ')}` : base;
}

const STATUS: Record<CaseResultView['status'], { text: string; Icon: typeof XCircle; tone: string }> = {
  passed: { text: 'Passed', Icon: CheckCircle2, tone: 'text-success-fg' },
  failed: { text: 'Failed', Icon: XCircle, tone: 'text-danger-fg' },
  error: { text: 'Error', Icon: AlertCircle, tone: 'text-danger-fg' },
  timed_out: { text: 'Timed out', Icon: Timer, tone: 'text-warning-fg' },
  ran: { text: 'Ran', Icon: Play, tone: 'text-text-secondary' },
  not_run: { text: 'Not run', Icon: CircleDashed, tone: 'text-text-secondary' },
};

function Detail({ name, value }: { name: string; value: string }) {
  return (
    <div className="space-y-0.5">
      <dt className="text-xs font-medium text-text-secondary">{name}</dt>
      <dd>
        <code className="block whitespace-pre-wrap break-words rounded-md bg-surface-sunken px-2 py-1 font-mono text-xs text-text-primary">
          {value}
        </code>
      </dd>
    </div>
  );
}

function Row({ item }: { item: CaseResultView }) {
  const { text, Icon, tone } = STATUS[item.status];
  const input = item.args?.join(', ');
  return (
    <li data-case-id={item.id} className="rounded-xl border border-border-subtle bg-surface-sunken p-3">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className={`inline-flex items-center gap-1.5 font-medium ${tone}`}>
          <Icon size={14} className="shrink-0" aria-hidden="true" />
          {text}
        </span>
        <span className="text-sm text-text-primary">{item.category}</span>
        {item.mine && <span className="rounded-full bg-accent-muted px-2 py-0.5 font-medium text-accent">Yours</span>}
        {item.elapsedMs !== null && item.elapsedMs !== undefined && (
          <span className="ml-auto tabular-nums text-text-muted">{item.elapsedMs} ms</span>
        )}
      </div>
      {item.hidden ? (
        <p className="mt-2 flex items-center gap-1.5 text-xs text-text-secondary">
          <EyeOff size={13} aria-hidden="true" />
          The input and expected output are kept back.
        </p>
      ) : (
        <dl className="mt-2 space-y-2">
          {input ? <Detail name="Input" value={input} /> : null}
          {item.expected ? <Detail name="Expected" value={item.expected} /> : null}
          {item.actual ? <Detail name="Got" value={item.actual} /> : null}
          {item.printed ? <Detail name="Printed" value={item.printed} /> : null}
          {item.error ? <Detail name="Error" value={item.error} /> : null}
        </dl>
      )}
    </li>
  );
}

/** Per-case results: a spoken summary, failures first, details in code. */
export function CaseResults({
  cases,
  title = 'Checks',
  emptyText = 'No results yet.',
}: {
  cases: CaseResultView[];
  title?: string;
  emptyText?: string;
}) {
  const sorted = [...cases].sort((a, b) => rank(a.status) - rank(b.status));
  return (
    <section className="space-y-3">
      <h3 className="text-sm font-semibold text-text-primary">{title}</h3>
      {cases.length === 0 ? (
        <p className="text-xs text-text-secondary">{emptyText}</p>
      ) : (
        <>
          <p role="status" className="text-xs text-text-secondary">
            {caseSummary(cases)}
          </p>
          <ul className="space-y-2">
            {sorted.map((item) => (
              <Row key={item.id} item={item} />
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
