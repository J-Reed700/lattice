import type * as Wire from '@/lib/bindings';

import { caseResultViews } from './caseViews';
import { programWork } from './work';
import type { KindPartProps } from '../../session/kind';
import { CaseResults } from '../../shared/CaseResults';

const RUN_TITLES: Record<Wire.CodingRunScope, string> = {
  shown: 'Examples',
  all: 'Every case',
  input: 'Your input',
};

export function ProgramResults({ session }: KindPartProps) {
  const { runs } = programWork(session);
  const lastRun = runs[runs.length - 1] as Wire.ProgramRunDto | undefined;

  return (
    <section aria-labelledby="coding-results-heading" className="flex flex-col gap-2">
      <h2 id="coding-results-heading" className="sr-only">
        Results
      </h2>
      <CaseResults
        cases={caseResultViews(lastRun?.cases ?? [])}
        title={lastRun ? RUN_TITLES[lastRun.scope] : 'Results'}
        emptyText="Run your examples to see the results here."
      />
      {lastRun?.referenceActual != null ? (
        <p className="m-0 text-sm text-text-primary">
          The reference returns <code className="font-mono">{lastRun.referenceActual}</code>
        </p>
      ) : null}
      {lastRun && (lastRun.stdout || lastRun.stderr) ? (
        <details className="rounded-lg border border-border-default bg-surface-sunken p-3 text-sm">
          <summary className="cursor-pointer font-semibold text-text-primary">What your program printed</summary>
          <pre className="m-0 mt-2 overflow-x-auto font-mono text-sm text-text-primary">
            {lastRun.stdout}
            {lastRun.stderr}
          </pre>
        </details>
      ) : null}
    </section>
  );
}
