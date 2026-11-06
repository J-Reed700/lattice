import { FileSearch } from 'lucide-react';
import { useSearchParams } from 'react-router';

/**
 * Code review drills on the Studio landing page, beside Flashcards: it opens the drill screen at
 * `/studio?drills=open`.
 */
export function CodeReviewDrillsSection() {
  const [, setParams] = useSearchParams();
  return (
    <section aria-labelledby="studio-drills-heading" className="mt-10 scroll-mt-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-[.16em] text-text-muted">Your practice</div>
          <h2 id="studio-drills-heading" className="mt-1 font-serif text-2xl text-text-primary">Code review drills</h2>
        </div>
        <button
          type="button"
          onClick={() => setParams({ drills: 'open' })}
          className="inline-flex items-center gap-1.5 rounded-full border border-border px-3.5 py-1.5 text-xs font-medium text-text-primary transition hover:border-accent/50 hover:text-accent"
        >
          <FileSearch size={14} aria-hidden="true" />
          Start a drill
        </button>
      </div>
      <div className="mt-5 rounded-2xl border border-dashed border-border bg-surface p-6">
        <h3 className="font-serif text-xl text-text-primary">Read code you did not write.</h3>
        <p className="mt-2 max-w-xl text-sm leading-6 text-text-secondary">
          Review a realistic piece of code the way you would a pull request: select lines, say what is wrong and why it matters, and see every issue that was planted in it, with how to say it in an interview.
        </p>
      </div>
    </section>
  );
}
