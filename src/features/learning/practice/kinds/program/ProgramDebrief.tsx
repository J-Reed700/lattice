import { ChevronRight } from 'lucide-react';

import type * as Wire from '@/lib/bindings';

import { caseResultViews } from './caseViews';
import { programDebriefWork } from './work';
import { CaseResults } from '../../shared/CaseResults';

const CARD = 'rounded-xl border border-border-default bg-surface p-4';
const HEADING = 'm-0 mb-3 font-serif text-lg font-semibold text-text-primary';

export function ProgramDebrief({ debrief }: { session: Wire.PracticeSessionDto; debrief: Wire.PracticeDebriefDto }) {
  const { strongAnswer, cases } = programDebriefWork(debrief);

  return (
    <>
      <section aria-labelledby="coding-strong-heading" className={CARD}>
        <h2 id="coding-strong-heading" className={HEADING}>
          A strong answer
        </h2>
        <p className="m-0 mb-2 text-sm leading-relaxed text-text-primary">{strongAnswer.approach}</p>
        <dl className="m-0 mb-3 text-sm">
          <dt className="text-xs font-semibold uppercase tracking-wide text-text-secondary">Cost</dt>
          <dd className="m-0 text-text-primary">{strongAnswer.complexity}</dd>
        </dl>
        <details className="group rounded-lg border border-border-default bg-surface-sunken p-3">
          <summary className="flex cursor-pointer list-none items-center gap-2 text-sm font-semibold text-text-primary">
            <ChevronRight className="h-4 w-4 transition-transform group-open:rotate-90" aria-hidden="true" />
            <span>Reference solution</span>
          </summary>
          <pre className="m-0 mt-3 overflow-x-auto font-mono text-sm text-text-primary">{strongAnswer.reference}</pre>
        </details>
      </section>

      <CaseResults cases={caseResultViews(cases)} title="Every case, in full" />
    </>
  );
}
