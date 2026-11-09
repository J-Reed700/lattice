import { useState, type FormEvent } from 'react';
import { AlertTriangle, Check, X } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

export interface ExamplesTableProps {
  signature: Wire.ProblemSignature;
  examples: Wire.CodingExampleDto[];
  coverage: Wire.ExampleCoverageDto;
  busy?: boolean;
  onCheck: (example: { ordinal: number | null; args: string[]; expected: string }) => Promise<void>;
  onRemove: (ordinal: number) => Promise<void>;
}

const FIELD =
  'min-h-[44px] rounded-lg border border-border-default bg-surface px-3 py-2 font-mono text-sm text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring';

function Verdict({ example }: { example: Wire.CodingExampleDto }) {
  const look = {
    right: { Icon: Check, text: 'Right', tone: 'bg-success-muted text-success-fg' },
    wrong: { Icon: X, text: 'Not right', tone: 'bg-danger-muted text-danger-fg' },
    invalid: { Icon: AlertTriangle, text: 'Invalid', tone: 'bg-warning-muted text-warning-fg' },
  }[example.verdict];
  return (
    <div className="flex flex-col gap-1">
      <span className={`inline-flex w-fit items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium ${look.tone}`}>
        <look.Icon className="h-3.5 w-3.5" aria-hidden="true" />
        <span>{look.text}</span>
      </span>
      {example.verdict === 'wrong' ? (
        <span className="text-xs text-text-secondary">
          {example.expects != null
            ? `The problem expects ${example.expects}`
            : "That's not what the problem expects. Check the statement."}
        </span>
      ) : null}
      {example.verdict === 'invalid' && example.whyInvalid ? (
        <span className="text-xs text-text-secondary">{example.whyInvalid}</span>
      ) : null}
    </div>
  );
}

export function ExamplesTable({ signature, examples, coverage, busy = false, onCheck, onRemove }: ExamplesTableProps) {
  const blank = signature.params.map(() => '');
  const [args, setArgs] = useState<string[]>(blank);
  const [expected, setExpected] = useState('');
  const [editing, setEditing] = useState<number | null>(null);

  const complete = args.every((value) => value.trim() !== '') && expected.trim() !== '';

  const reset = () => {
    setArgs(blank);
    setExpected('');
    setEditing(null);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!complete || busy) return;
    try {
      await onCheck({ ordinal: editing, args: args.map((value) => value.trim()), expected: expected.trim() });
      reset();
    } catch {
      /* the screen shows why, and the fields keep what was typed */
    }
  };

  const edit = (example: Wire.CodingExampleDto) => {
    setArgs(signature.params.map((_, index) => example.args[index] ?? ''));
    setExpected(example.expected);
    setEditing(example.ordinal);
  };

  return (
    <section aria-labelledby="coding-examples-heading" className="rounded-xl border border-border-default bg-surface p-4">
      <h2 id="coding-examples-heading" className="m-0 mb-2 font-serif text-lg font-semibold text-text-primary">
        Your examples
      </h2>
      <p role="status" className="m-0 text-sm font-medium text-text-primary">
        Your examples catch {coverage.caught} of {coverage.total} common mistakes
      </p>
      {coverage.uncaught ? (
        <p className="m-0 mt-1 text-sm text-text-secondary">Not caught yet: {coverage.uncaught}</p>
      ) : null}

      {examples.length > 0 ? (
        <div className="mt-3 overflow-x-auto rounded-lg border border-border-subtle">
          <table className="w-full border-collapse text-left text-sm">
            <caption className="sr-only">Your checked examples</caption>
            <thead>
              <tr className="border-b border-border-default bg-surface-sunken text-xs text-text-secondary">
                <th scope="col" className="p-2 font-semibold">Call</th>
                <th scope="col" className="p-2 font-semibold">Expected</th>
                <th scope="col" className="p-2 font-semibold">Verdict</th>
                <th scope="col" className="p-2 font-semibold"><span className="sr-only">Actions</span></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border-subtle">
              {examples.map((example, index) => (
                <tr key={example.ordinal} className="align-top">
                  <td className="whitespace-pre-wrap p-2 font-mono text-text-primary">
                    {signature.function}({example.args.join(', ')})
                  </td>
                  <td className="whitespace-pre-wrap p-2 font-mono text-text-primary">{example.expected}</td>
                  <td className="p-2"><Verdict example={example} /></td>
                  <td className="p-2">
                    <div className="flex flex-wrap gap-2">
                      <button
                        type="button"
                        onClick={() => edit(example)}
                        aria-label={`Edit example ${index + 1}`}
                        className="min-h-[36px] cursor-pointer rounded-lg border border-border-default bg-surface px-3 text-xs font-semibold text-text-primary hover:bg-surface-raised"
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        onClick={() => void onRemove(example.ordinal)}
                        aria-label={`Remove example ${index + 1}`}
                        className="min-h-[36px] cursor-pointer rounded-lg border border-border-default bg-surface px-3 text-xs font-semibold text-text-primary hover:bg-surface-raised"
                      >
                        Remove
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}

      <form onSubmit={submit} className="mt-3 flex flex-col gap-3">
        <div className="flex flex-wrap items-end gap-3">
          {signature.params.map((param, index) => (
            <div key={param.name} className="flex min-w-[8rem] flex-1 flex-col gap-1">
              <label htmlFor={`coding-example-${param.name}`} className="text-xs font-semibold text-text-primary">
                {param.name}
              </label>
              <input
                id={`coding-example-${param.name}`}
                value={args[index] ?? ''}
                onChange={(event) => setArgs((current) => current.map((value, at) => (at === index ? event.target.value : value)))}
                spellCheck={false}
                autoComplete="off"
                className={FIELD}
              />
              <span className="font-mono text-xs text-text-secondary">{param.annotation}</span>
            </div>
          ))}
          <div className="flex min-w-[8rem] flex-1 flex-col gap-1">
            <label htmlFor="coding-example-expected" className="text-xs font-semibold text-text-primary">
              expected
            </label>
            <input
              id="coding-example-expected"
              value={expected}
              onChange={(event) => setExpected(event.target.value)}
              spellCheck={false}
              autoComplete="off"
              className={FIELD}
            />
            <span className="font-mono text-xs text-text-secondary">{signature.returns}</span>
          </div>
        </div>
        <div className="flex flex-wrap items-start gap-3">
          <div className="flex flex-col gap-1">
            <button
              type="submit"
              disabled={!complete || busy}
              aria-describedby={!complete ? 'coding-example-reason' : undefined}
              className="min-h-[44px] cursor-pointer rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
            >
              Check
            </button>
            {!complete ? (
              <span id="coding-example-reason" className="text-xs text-text-secondary">
                Fill in every argument and the expected result, as Python values.
              </span>
            ) : null}
          </div>
          {editing != null ? (
            <button
              type="button"
              onClick={reset}
              className="min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised"
            >
              Cancel edit
            </button>
          ) : null}
        </div>
      </form>
    </section>
  );
}
