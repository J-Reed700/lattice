import type * as Wire from '@/lib/bindings';

export interface SolutionControlProps {
  session: Wire.PracticeSessionDto;
  onReveal: () => void;
}

/** The button that opens the worked solution, and why it is not open yet. Nothing once the solution is open. */
export function SolutionControl({ session, onReveal }: SolutionControlProps) {
  if (session.solutionShown) return null;

  return (
    <div className="flex flex-col items-start gap-1">
      <button
        type="button"
        onClick={onReveal}
        disabled={!session.solutionAvailable}
        aria-describedby={session.solutionAvailable ? undefined : 'practice-solution-reason'}
        className="min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50"
      >
        Show the worked solution
      </button>
      {!session.solutionAvailable ? (
        <span id="practice-solution-reason" className="text-sm text-text-secondary">
          {session.setting === 'coached' ? 'It opens after your first submit.' : 'It comes with your debrief.'}
        </span>
      ) : null}
    </div>
  );
}
