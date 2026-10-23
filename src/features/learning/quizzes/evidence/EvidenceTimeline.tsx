import { shortDate } from '@/features/learning/quizzes/quizCopy';
import type { LearningEvidenceDimension, LearningEvidenceEventDto, LearningEvidenceResult, LearningOutcomeDefinitionDto } from '@/lib/bindings';

const dimensions: { id: LearningEvidenceDimension; label: string }[] = [
  { id: 'recall', label: 'Recall' },
  { id: 'explanation', label: 'Explanation' },
  { id: 'application', label: 'Application' },
  { id: 'transfer', label: 'Transfer' },
];

const resultWords: Record<LearningEvidenceResult, string> = {
  observed: 'Observed', not_observed: 'Not observed yet', uncertain: 'Uncertain', not_assessed: 'Not assessed',
};

/** Outcome evidence in four dimensions. An empty dimension means nothing recorded, not a miss. */
export function EvidenceTimeline({ outcomes, evidence }: { outcomes: LearningOutcomeDefinitionDto[]; evidence: LearningEvidenceEventDto[] }) {
  return <section aria-labelledby="evidence-heading" className="rounded-2xl border border-border bg-surface p-5">
    <div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Outcome evidence</div>
    <h3 id="evidence-heading" className="mt-1 font-serif text-xl text-text-primary">Four ways to observe learning</h3>
    <p className="mt-1.5 text-xs leading-5 text-text-muted">Each signal is recorded separately. An empty row means nothing has been recorded yet, not that an outcome was missed.</p>
    <div className="mt-4 space-y-2.5">{dimensions.map(({ id, label }) => {
      const events = evidence.filter((event) => event.dimension === id).sort((a, b) => b.observedAt - a.observedAt);
      return <div key={id} className="rounded-xl border border-border bg-background/40 p-3">
        <div className="flex items-baseline justify-between gap-2">
          <h4 className="text-xs font-semibold text-text-primary">{label}</h4>
          <span className="text-[11px] tabular-nums text-text-muted">{events.length} {events.length === 1 ? 'signal' : 'signals'}</span>
        </div>
        {events.length === 0 ? <p className="mt-1 text-[11px] text-text-muted">No recorded evidence yet</p> : <ul className="mt-2 space-y-2">{events.slice(0, 3).map((event) => {
          const outcome = outcomes.find((entry) => entry.id === event.outcomeId);
          return <li key={event.id} className="border-t border-border pt-2 first:border-0 first:pt-0">
            <div className="flex flex-wrap justify-between gap-2 text-[11px]">
              <span className="font-medium text-text-secondary">{outcome?.title ?? event.sourceKind.replace('_', ' ')}</span>
              <span className="text-text-muted">{resultWords[event.result]}</span>
            </div>
            <p className="mt-0.5 text-[11px] leading-4 text-text-muted">{event.observation || 'Evidence recorded'} · {shortDate(event.observedAt)}</p>
            {event.evidenceQuote && <blockquote className="mt-1 border-l border-accent/50 pl-2 text-[11px] italic text-text-muted">{event.evidenceQuote}</blockquote>}
          </li>;
        })}</ul>}
      </div>;
    })}</div>
  </section>;
}
