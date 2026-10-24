import { dateText, statusStyle } from "@/features/learning/practice/practical/practicalWorkbenchHelpers";
import type { LearningPracticalRunDto } from "@/lib/bindings";

export function RunHistory({
  runs,
  selectedRun,
  setSelectedRunId,
}: {
  runs: LearningPracticalRunDto[];
  selectedRun: LearningPracticalRunDto | undefined;
  setSelectedRunId: (runId: string) => void;
}) {
  return (
    <section className="rounded-2xl border border-border-default bg-surface p-4 sm:p-5 shadow-xs">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-xs font-semibold uppercase tracking-[.14em] text-text-muted">
            Run history
          </div>
          <h3 className="mt-1 font-serif text-lg text-text-primary">
            This activity’s attempts
          </h3>
        </div>
        <span className="rounded-full bg-surface-sunken px-3 py-1.5 text-xs text-text-muted">
          {runs.length}
        </span>
      </div>
      {runs.length === 0 ? (
        <p className="mt-3 rounded-xl bg-surface-sunken p-4 text-xs text-text-muted">
          Runs appear here after a local runtime starts. Conversation
          turns are kept separately.
        </p>
      ) : (
        <div className="mt-3 divide-y divide-border-subtle">
          {runs.map((run) => (
            <button
              type="button"
              key={run.id}
              aria-pressed={selectedRun?.id === run.id}
              onClick={() => setSelectedRunId(run.id)}
              className={`flex w-full flex-wrap items-center justify-between gap-2 py-3 text-left first:pt-0 ${selectedRun?.id === run.id ? "text-accent" : ""}`}
            >
              <span className="text-xs text-text-secondary">
                {dateText(run.createdAt)} · revision{" "}
                {run.activityRevision}
              </span>
              <span
                className={`rounded-full px-2.5 py-1 text-xs capitalize ${statusStyle(run.status)}`}
              >
                {run.status}
              </span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
