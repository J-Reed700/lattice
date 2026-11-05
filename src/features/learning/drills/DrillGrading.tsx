import { LoaderCircle } from 'lucide-react';

import { Button } from '@/components/ui/button';
import type { CodeReviewResultDto } from '@/lib/bindings';

/** A review that is handed in and waits for its grade, or could not be graded and can be graded again. */
export function DrillGrading({ result, message, regrading, onRegrade }: {
  result: CodeReviewResultDto;
  message: string | null;
  regrading: boolean;
  onRegrade: () => void;
}) {
  if (result.status === 'couldnt_grade') {
    return (
      <section className="rounded-2xl border border-border bg-surface p-6">
        <h2 className="font-serif text-xl text-text-primary">Your review could not be graded</h2>
        <p className="mt-2 text-sm text-text-secondary">{result.error ?? 'The grader gave no assessment.'}</p>
        <p className="mt-2 text-sm text-text-secondary">Your review is saved. Grade it again when you are ready.</p>
        <Button type="button" className="mt-4" disabled={regrading} onClick={onRegrade}>
          {regrading ? 'Grading again…' : 'Grade again'}
        </Button>
      </section>
    );
  }
  return (
    <section className="rounded-2xl border border-border bg-surface p-6">
      <div role="status" aria-live="polite">
        <div className="flex items-center gap-3">
          <LoaderCircle size={18} className="shrink-0 animate-spin text-accent motion-reduce:animate-none" aria-hidden="true" />
          <h2 className="font-serif text-xl text-text-primary">Grading your review…</h2>
        </div>
        {message && <p className="mt-3 text-sm text-text-secondary">{message}</p>}
      </div>
      <p className="mt-3 text-xs leading-5 text-text-muted">
        This takes a minute or two. You can leave this page: your review is saved, and the grade is here when you come back.
      </p>
    </section>
  );
}
