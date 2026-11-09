import { MisconceptionCard } from '@/features/learning/practice/workbench/MisconceptionCard';
import type * as Wire from '@/lib/bindings';

type Proposal = Wire.LearningPracticeSessionDto['proposals'][number];

const STATUS_WORDS: Record<Wire.LearningPracticeProposalStatus, string> = { pending: 'Waiting for you', accepted: 'Accepted', rejected: 'Not used' };

export interface ProposalListProps {
  programId: string;
  lessonId: string;
  proposals: Proposal[];
  active: boolean;
  busy: boolean;
  onDecide: (proposalId: string, accepted: boolean) => void;
}

export function ProposalList({ programId, lessonId, proposals, active, busy, onDecide }: ProposalListProps) {
  if (proposals.length === 0) return null;
  return (
    <section className="rounded-2xl border border-border-default bg-surface p-5">
      <div className="text-xs font-semibold uppercase tracking-[.15em] text-accent">Follow-up ideas</div>
      <h4 className="mt-1 font-serif text-xl text-text-primary">Tutor proposals</h4>
      <div className="mt-4 space-y-3">
        {proposals.map((proposal) => (
          <article key={proposal.id} className="rounded-xl border border-border-default bg-surface-sunken p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="rounded-full bg-accent-muted px-2.5 py-1 text-xs font-semibold uppercase tracking-wider text-accent">
                {proposal.kind === 'follow_up' ? 'Follow-up' : 'Possible misconception'}
              </span>
              <span className="text-xs text-text-muted">{STATUS_WORDS[proposal.status]}</span>
            </div>
            <p className="mt-2 text-sm leading-6 text-text-secondary">{proposal.text}</p>
            {proposal.evidenceQuote && (
              <blockquote className="mt-3 border-l-2 border-accent pl-3 text-xs italic leading-5 text-text-muted">
                “{proposal.evidenceQuote}”
              </blockquote>
            )}
            {active && proposal.status === 'pending' && (
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onDecide(proposal.id, true)}
                  className="rounded-full bg-accent px-3 py-2 text-xs font-semibold text-accent-fg disabled:opacity-50"
                >
                  Accept idea
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onDecide(proposal.id, false)}
                  className="rounded-full border border-border-default px-3 py-2 text-xs font-semibold text-text-secondary disabled:opacity-50"
                >
                  Not useful
                </button>
                {busy && <p className="text-xs text-text-muted">Another action is in progress.</p>}
              </div>
            )}
            {proposal.kind === 'misconception' && proposal.status === 'accepted' && (
              <MisconceptionCard programId={programId} lessonId={lessonId} idea={proposal.text} />
            )}
          </article>
        ))}
      </div>
    </section>
  );
}
