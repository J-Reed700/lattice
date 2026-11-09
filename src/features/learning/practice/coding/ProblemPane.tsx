import { ChevronRight } from 'lucide-react';
import type { PublicProblemDto, Setting } from './codingApi';

export interface ProblemPaneProps {
  problem: PublicProblemDto;
  setting: Setting;
  className?: string;
}

export function ProblemPane({
  problem,
  setting,
  className = '',
}: ProblemPaneProps) {
  const sig = problem.signature;
  const paramStr = sig.params
    ? sig.params.map((p) => `${p.name}: ${p.annotation}`).join(', ')
    : '';
  const formattedSig = `def ${sig.function}(${paramStr}) -> ${sig.returns}`;

  return (
    <section
      aria-labelledby="problem-pane-title"
      className={`rounded-xl border border-border-default bg-surface p-5 shadow-xs ${className}`}
    >
      <h2
        id="problem-pane-title"
        className="mb-3 font-serif text-xl font-semibold text-text-primary"
      >
        {problem.title || 'Problem'}
      </h2>

      {/* Statement */}
      <div className="mb-4 text-sm leading-relaxed text-text-primary whitespace-pre-wrap">
        {problem.statement}
      </div>

      {/* Signature */}
      <div className="mb-4">
        <div className="mb-1.5 text-xxs font-semibold uppercase tracking-wider text-text-tertiary">
          Signature
        </div>
        <div className="overflow-x-auto rounded-lg border border-border-default bg-surface-sunken p-2.5 font-mono text-xs text-text-primary">
          <code>{formattedSig}</code>
        </div>
      </div>

      {/* Constraints */}
      {problem.constraints && problem.constraints.length > 0 && (
        <div className="mb-4">
          <div className="mb-1.5 text-xxs font-semibold uppercase tracking-wider text-text-tertiary">
            Constraints
          </div>
          <ul className="list-inside list-disc space-y-1 text-sm text-text-secondary m-0 p-0">
            {problem.constraints.map((c, i) => (
              <li key={i}>{c}</li>
            ))}
          </ul>
        </div>
      )}

      {/* Examples */}
      {problem.examples && problem.examples.length > 0 && (
        <div className="mb-4">
          <div className="mb-1.5 text-xxs font-semibold uppercase tracking-wider text-text-tertiary">
            Examples
          </div>
          <div className="space-y-1.5 overflow-x-auto rounded-lg border border-border-default bg-surface-sunken p-2.5 font-mono text-xs text-text-primary">
            {problem.examples.map((ex) => {
              const argStr = ex.args ? ex.args.join(', ') : '';
              const callStr = `${sig.function}(${argStr})`;
              const expectedStr = ex.expected !== undefined && ex.expected !== null ? ex.expected : 'None';
              return (
                <div key={ex.id} className="whitespace-pre">
                  <span className="text-accent">{callStr}</span>
                  <span className="text-text-tertiary"> → </span>
                  <span className="text-text-primary">{expectedStr}</span>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Subgoals (Coached only) */}
      {setting === 'coached' && problem.subgoals && problem.subgoals.length > 0 && (
        <div className="mb-4">
          <div className="mb-1.5 text-xxs font-semibold uppercase tracking-wider text-text-tertiary">
            Subgoals
          </div>
          <ul className="space-y-1.5 text-sm text-text-secondary list-none m-0 p-0">
            {problem.subgoals.map((sg) => (
              <li key={sg.id} className="flex items-center gap-2">
                <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
                <span>{sg.label}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Worked solution disclosure (only when present) */}
      {problem.workedSolution && (
        <details className="mt-4 rounded-lg border border-border-default bg-surface-sunken p-3 text-sm group">
          <summary className="flex cursor-pointer items-center gap-2 font-semibold text-text-primary list-none">
            <ChevronRight className="h-4 w-4 transition-transform group-open:rotate-90 text-text-tertiary" aria-hidden="true" />
            <span>Show the worked solution</span>
          </summary>
          <div className="mt-3 overflow-x-auto rounded border border-border-default bg-surface p-3 font-mono text-xs text-text-primary whitespace-pre">
            {problem.workedSolution}
          </div>
        </details>
      )}
    </section>
  );
}
