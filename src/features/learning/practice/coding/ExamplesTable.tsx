import { useState } from 'react';
import { AlertCircle, Check, X } from 'lucide-react';
import type { CodingExampleDto, ExampleVerdict } from './codingApi';

export interface ExamplesTableProps {
  examples: CodingExampleDto[];
  totalMisreadings: number;
  onCheckExample: (argsJson: string, expectedJson: string) => Promise<void> | void;
  checking?: boolean;
  paramNames?: string[];
  className?: string;
}

function VerdictBadge({ verdict }: { verdict: ExampleVerdict }) {
  if (verdict === 'right') {
    return (
      <span className="inline-flex min-h-[28px] items-center gap-1.5 rounded-full bg-success-muted px-2.5 text-xs font-medium text-success-fg">
        <Check className="h-3.5 w-3.5" aria-hidden="true" />
        <span>Right</span>
      </span>
    );
  }

  if (verdict === 'wrong') {
    return (
      <span className="inline-flex min-h-[28px] items-center gap-1.5 rounded-full bg-danger-muted px-2.5 text-xs font-medium text-danger-fg">
        <X className="h-3.5 w-3.5" aria-hidden="true" />
        <span>Not right</span>
      </span>
    );
  }

  return (
    <span className="inline-flex min-h-[28px] items-center gap-1.5 rounded-full bg-warning-muted px-2.5 text-xs font-medium text-warning-fg">
      <AlertCircle className="h-3.5 w-3.5" aria-hidden="true" />
      <span>Invalid</span>
    </span>
  );
}

export function ExamplesTable({
  examples,
  totalMisreadings,
  onCheckExample,
  checking = false,
  paramNames = ['sentence'],
  className = '',
}: ExamplesTableProps) {
  const [argsInput, setArgsInput] = useState('');
  const [expectedInput, setExpectedInput] = useState('');

  // Collect unique misreadings caught across all examples
  const caughtSet = new Set<string>();
  for (const ex of examples) {
    if (ex.verdict === 'right' && ex.catches) {
      for (const c of ex.catches) {
        caughtSet.add(c);
      }
    }
  }
  const caughtCount = Math.min(caughtSet.size, totalMisreadings);
  const total = Math.max(totalMisreadings, 1);

  const primaryParam = paramNames[0] || 'sentence';

  const handleCheck = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!argsInput.trim() || checking) return;

    // Normalize args to JSON array if user typed single string or raw value
    let normalizedArgs = argsInput.trim();
    if (!normalizedArgs.startsWith('[')) {
      normalizedArgs = `[${normalizedArgs}]`;
    }

    const normalizedExpected = expectedInput.trim();
    await onCheckExample(normalizedArgs, normalizedExpected);
    setArgsInput('');
    setExpectedInput('');
  };

  return (
    <section
      aria-labelledby="examples-table-heading"
      className={`rounded-xl border border-border-default bg-surface p-5 shadow-xs ${className}`}
    >
      <h2
        id="examples-table-heading"
        className="mb-2 font-serif text-xl font-semibold text-text-primary"
      >
        Your examples
      </h2>

      {/* Misreading coverage summary */}
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <p role="status" className="m-0 font-medium text-sm text-text-primary">
          Your examples catch {caughtCount} of {total} common mistakes
        </p>
        <div aria-hidden="true" className="flex gap-1">
          {Array.from({ length: total }).map((_, i) => (
            <span
              key={i}
              className={`block h-2 w-9 rounded-full border border-border-subtle transition-colors ${
                i < caughtCount ? 'bg-success-fg' : 'bg-surface-raised'
              }`}
            />
          ))}
        </div>
      </div>

      {/* Examples Table */}
      {examples.length > 0 && (
        <div className="mb-4 overflow-x-auto rounded-lg border border-border-subtle">
          <table className="w-full min-w-[500px] border-collapse text-left text-xs">
            <caption className="sr-only">Your checked examples</caption>
            <thead>
              <tr className="border-b border-border-default bg-surface-sunken">
                <th scope="col" className="p-2.5 font-mono font-semibold text-text-secondary">
                  {primaryParam}
                </th>
                <th scope="col" className="p-2.5 font-mono font-semibold text-text-secondary">
                  expected
                </th>
                <th scope="col" className="p-2.5 font-semibold text-text-secondary">
                  Result
                </th>
                <th scope="col" className="p-2.5 font-semibold text-text-secondary">
                  Catches
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border-subtle">
              {examples.map((ex, idx) => {
                // Parse or display argsJson nicely
                let displayArgs = ex.argsJson;
                try {
                  const parsed = JSON.parse(ex.argsJson);
                  if (Array.isArray(parsed) && parsed.length === 1) {
                    displayArgs = typeof parsed[0] === 'string' ? `"${parsed[0]}"` : String(parsed[0]);
                  }
                } catch {
                  // Keep raw string
                }

                return (
                  <tr key={idx} className="bg-surface hover:bg-surface-sunken/40">
                    <td className="p-2.5 font-mono text-text-primary align-top whitespace-pre-wrap">
                      {displayArgs}
                    </td>
                    <td className="p-2.5 font-mono text-text-primary align-top whitespace-pre-wrap">
                      {ex.expectedJson || 'None'}
                    </td>
                    <td className="p-2.5 align-top">
                      <VerdictBadge verdict={ex.verdict} />
                    </td>
                    <td className="p-2.5 text-text-secondary align-top">
                      {ex.catches && ex.catches.length > 0
                        ? ex.catches.join(', ')
                        : '—'}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {/* Example input form */}
      <form onSubmit={handleCheck} className="flex flex-wrap items-end gap-3 pt-2">
        <div className="flex flex-col gap-1.5 flex-1 min-w-[140px]">
          <label htmlFor="ex-args" className="text-xs font-semibold text-text-primary">
            {primaryParam}
          </label>
          <input
            id="ex-args"
            type="text"
            value={argsInput}
            onChange={(e) => setArgsInput(e.target.value)}
            placeholder={`"a b a"`}
            className="min-h-[44px] rounded-lg border border-border-default bg-surface px-3 py-2 font-mono text-xs text-text-primary placeholder:text-text-muted focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          />
        </div>
        <div className="flex flex-col gap-1.5 flex-1 min-w-[120px]">
          <label htmlFor="ex-expected" className="text-xs font-semibold text-text-primary">
            expected
          </label>
          <input
            id="ex-expected"
            type="text"
            value={expectedInput}
            onChange={(e) => setExpectedInput(e.target.value)}
            placeholder={`"a"`}
            className="min-h-[44px] rounded-lg border border-border-default bg-surface px-3 py-2 font-mono text-xs text-text-primary placeholder:text-text-muted focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          />
        </div>
        <button
          type="submit"
          disabled={checking || !argsInput.trim()}
          className="inline-flex min-h-[44px] cursor-pointer items-center justify-center rounded-lg border border-border-default bg-surface-raised px-4 text-sm font-semibold text-text-primary transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {checking ? 'Checking…' : 'Check'}
        </button>
      </form>
      <p className="mt-2 text-xs text-text-tertiary">
        Type Python values, like &quot;a b&quot; or None.
      </p>
    </section>
  );
}
