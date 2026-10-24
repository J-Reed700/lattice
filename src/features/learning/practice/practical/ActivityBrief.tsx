import { TiptapViewer } from "@/components/TiptapEditor";
import { modeCopy } from "@/features/learning/practice/practical/ActivityComposer";
import type { LearningPracticalActivityDto } from "@/lib/bindings";

export function ActivityBrief({
  activeActivity,
}: {
  activeActivity: LearningPracticalActivityDto;
}) {
  return (
    <section className="rounded-2xl border border-border-default bg-surface p-4 sm:p-6 shadow-xs">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="rounded-full bg-accent-muted px-2.5 py-1 text-xs font-semibold uppercase tracking-wider text-accent">
              {activeActivity.kind.replace("_", " ")}
            </span>
            <span className="text-xs text-text-muted">
              Revision {activeActivity.revision} · generated with{" "}
              {activeActivity.generatorModel}
            </span>
          </div>
          <h3 className="mt-2 font-serif text-2xl text-text-primary">
            {activeActivity.title}
          </h3>
          <div className="mt-3 text-sm leading-7 text-text-secondary">
            <TiptapViewer content={activeActivity.brief} />
          </div>
        </div>
        <span
          className={`shrink-0 rounded-full px-3 py-1.5 text-xs font-medium ${activeActivity.runtimeAvailable ? "bg-success-muted text-success-fg" : "bg-warning-muted text-warning-fg"}`}
        >
          {activeActivity.runtimeKind === "none" ? "Review activity" : activeActivity.runtimeAvailable
            ? "Ready to run"
            : "Execution unavailable"}
        </span>
      </div>
      <details className="mt-4 border-t border-border-subtle pt-3">
        <summary className="cursor-pointer text-sm font-medium text-text-secondary">Success criteria &amp; allowed help</summary>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
        <div className="rounded-xl border border-border-default bg-surface-sunken p-4">
          <div className="text-xs font-semibold uppercase tracking-[.14em] text-text-muted">
            Help available · {activeActivity.practiceMode}
          </div>
          <p className="mt-2 text-xs leading-5 text-text-secondary">
            {modeCopy[activeActivity.practiceMode]}
          </p>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {activeActivity.allowedAids.length ? (
              activeActivity.allowedAids.map((aid) => (
                <span
                  key={aid}
                  className="rounded-full bg-surface px-2.5 py-1 text-xs text-text-muted"
                >
                  {aid}
                </span>
              ))
            ) : (
              <span className="rounded-full bg-surface px-2.5 py-1 text-xs text-text-muted">
                No aids recorded
              </span>
            )}
          </div>
        </div>
        <div className="rounded-xl border border-border-default bg-surface-sunken p-4">
          <div className="text-xs font-semibold uppercase tracking-[.14em] text-text-muted">
            What to notice
          </div>
          <ul className="mt-2 space-y-1 text-xs leading-5 text-text-secondary">
            {activeActivity.rubric.map((criterion) => (
              <li key={criterion.id}>
                <span className="font-medium text-text-primary">
                  {criterion.title}
                </span>{" "}
                · {criterion.description}
              </li>
            ))}
          </ul>
        </div>
        </div>
      </details>
    </section>
  );
}
