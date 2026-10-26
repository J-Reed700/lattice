import { useState } from "react";

import { Layers, LoaderCircle } from "lucide-react";
import { useInRouterContext } from "react-router";

import { useCreateInterviewDeck } from "@/features/learning/recall/flashcards/useFlashcards";
import type { LearningPracticalActivityDto } from "@/lib/bindings";

/** Tomorrow at 9:00 local time, as a datetime-local value: a likely interview slot to start from. */
function tomorrowMorning() {
  const date = new Date();
  date.setDate(date.getDate() + 1);
  date.setHours(9, 0, 0, 0);
  return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
}

/**
 * Turns an interview's questions and private answer guides into a recall
 * deck. The mock interview is the realistic rehearsal; the drill is where each
 * answer gets retrieved from memory again and again, spaced to fit the
 * interview date, with the guide shown only after each attempt.
 */
export function InterviewDrill(props: { programId: string; activity: LearningPracticalActivityDto }) {
  // The drill opens its deck in Studio, so it needs the app's router.
  return useInRouterContext() ? <DrillForm {...props} /> : null;
}

function DrillForm({ programId, activity }: { programId: string; activity: LearningPracticalActivityDto }) {
  const create = useCreateInterviewDeck();
  const [when, setWhen] = useState(tomorrowMorning);
  const questions = activity.interviewPlan?.questions?.length ?? 0;
  const targetAt = when ? new Date(when).getTime() : NaN;
  const validTarget = Number.isFinite(targetAt) && targetAt > Date.now();
  if (!questions) return null;
  return (
    <section aria-labelledby={`interview-drill-${activity.id}`} className="rounded-2xl border border-border bg-surface p-4 sm:p-5">
      <div className="flex items-start gap-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent">
          <Layers size={18} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-xs font-semibold uppercase tracking-[.14em] text-accent">Spaced drill</div>
          <h3 id={`interview-drill-${activity.id}`} className="mt-1 font-serif text-xl text-text-primary">
            Drill these {questions} {questions === 1 ? "question" : "questions"} until they stick
          </h3>
          <p className="mt-1 text-xs leading-5 text-text-secondary">
            Each question becomes a card. You answer it aloud from memory, then tick off the points a strong answer covers. Misses come back in the same session, and reviews are spaced to land before your interview. Works best after one cold attempt at the mock interview above, and before a final one.
          </p>
        </div>
      </div>
      <form
        className="mt-4 flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          if (create.isPending) return;
          create.mutate({ programId, activityId: activity.id, targetAt: validTarget ? targetAt : null });
        }}
      >
        <label className="text-xs font-medium text-text-secondary">
          Interview date and time
          <input
            type="datetime-local"
            value={when}
            onChange={(event) => setWhen(event.target.value)}
            className="mt-1.5 block rounded-lg border border-border bg-background px-3 py-2 text-sm"
          />
        </label>
        <button
          type="submit"
          disabled={create.isPending}
          className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-45"
        >
          {create.isPending ? <LoaderCircle size={13} className="animate-spin" /> : <Layers size={13} />}
          {create.isPending ? "Making the drill…" : "Make drill deck"}
        </button>
        {when && !validTarget && <span className="text-xs text-text-muted">That time has passed; the deck will use its normal schedule.</span>}
      </form>
      {create.isError && (
        <p role="alert" className="mt-3 rounded-lg bg-rose-500/10 p-3 text-xs text-rose-700 dark:text-rose-300">
          {create.error.message}
        </p>
      )}
    </section>
  );
}
