import { ChevronRight } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

export interface ProblemPaneProps {
  problem: Wire.PublicProblemDto;
  workedSolution: string | null;
}

function Heading({ children }: { children: string }) {
  return <h3 className="m-0 mb-1.5 text-xs font-semibold uppercase tracking-wide text-text-secondary">{children}</h3>;
}

export function ProblemPane({ problem, workedSolution }: ProblemPaneProps) {
  const { signature } = problem;
  const params = signature.params.map((param) => `${param.name}: ${param.annotation}`).join(', ');

  return (
    <section aria-labelledby="coding-problem-heading" className="rounded-xl border border-border-default bg-surface p-4">
      <h2 id="coding-problem-heading" className="m-0 mb-3 font-serif text-xl font-semibold text-text-primary">
        {problem.title}
      </h2>
      <p className="m-0 mb-4 whitespace-pre-wrap text-sm leading-relaxed text-text-primary">{problem.statement}</p>

      <div className="mb-4">
        <Heading>Signature</Heading>
        <code className="block overflow-x-auto rounded-lg border border-border-default bg-surface-sunken p-2.5 font-mono text-sm text-text-primary">
          {`def ${signature.function}(${params}) -> ${signature.returns}`}
        </code>
      </div>

      {problem.constraints.length > 0 ? (
        <div className="mb-4">
          <Heading>Constraints</Heading>
          <ul className="m-0 list-inside list-disc space-y-1 p-0 text-sm text-text-secondary">
            {problem.constraints.map((constraint) => (
              <li key={constraint}>{constraint}</li>
            ))}
          </ul>
        </div>
      ) : null}

      {problem.examples.length > 0 ? (
        <div className="mb-4">
          <Heading>Examples</Heading>
          <ul className="m-0 list-none space-y-1.5 overflow-x-auto rounded-lg border border-border-default bg-surface-sunken p-2.5 font-mono text-sm text-text-primary">
            {problem.examples.map((example) => (
              <li key={example.caseId} className="whitespace-pre">
                {signature.function}({(example.args ?? []).join(', ')}) <span aria-hidden="true">→</span>
                <span className="sr-only"> returns </span> {example.expected ?? 'None'}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {workedSolution ? (
        <details className="group rounded-lg border border-border-default bg-surface-sunken p-3 text-sm">
          <summary className="flex cursor-pointer list-none items-center gap-2 font-semibold text-text-primary">
            <ChevronRight className="h-4 w-4 transition-transform group-open:rotate-90" aria-hidden="true" />
            <span>The worked solution</span>
          </summary>
          <pre className="m-0 mt-3 overflow-x-auto rounded border border-border-default bg-surface p-3 font-mono text-sm text-text-primary">
            {workedSolution}
          </pre>
        </details>
      ) : null}
    </section>
  );
}
