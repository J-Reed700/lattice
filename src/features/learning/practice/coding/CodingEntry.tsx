import type { CodingTask, Difficulty, Setting } from '@/lib/bindings';

export interface CodingEntryRequest {
  task: CodingTask;
  setting: Setting;
  language: string;
  difficulty: Difficulty;
  focus: string | null;
  budgetMinutes: number | null;
}

export interface CodingEntryProps {
  programId: string;
  lessonId: string | null;
  request: CodingEntryRequest;
  onExit: () => void;
}

/**
 * Entry point for coding sessions from PracticeHome (Code card & Mock interview coding rounds).
 * Wave 2 replaces this stub with the problem build progress and CodingSession screen.
 */
export function CodingEntry({ onExit }: CodingEntryProps): JSX.Element {
  return (
    <section className="rounded-2xl border border-border-default bg-surface p-6">
      <div className="flex items-center justify-between gap-3">
        <h3 className="font-serif text-xl text-text-primary">Coding session</h3>
        <button
          type="button"
          onClick={onExit}
          className="rounded-lg border border-border-default bg-surface-raised px-3 py-1.5 text-xs font-medium text-text-primary"
        >
          Back to Practice
        </button>
      </div>
    </section>
  );
}
