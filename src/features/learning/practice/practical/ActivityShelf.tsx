import { Code2 } from "lucide-react";

import type { LearningLessonDto, LearningPracticalActivityDto } from "@/lib/bindings";

export function ActivityShelf({
  lesson,
  activities,
  activeActivity,
  generatePending,
  onCreate,
  onSelectActivity,
}: {
  lesson: LearningLessonDto | null;
  activities: LearningPracticalActivityDto[];
  activeActivity: LearningPracticalActivityDto | undefined;
  generatePending: boolean;
  onCreate: () => void;
  onSelectActivity: (activity: LearningPracticalActivityDto) => Promise<void>;
}) {
  return (
    <aside className="min-w-0 space-y-4">
      <section className="rounded-2xl border border-border-default bg-surface p-4 shadow-xs">
        <div className="flex items-center justify-between gap-2">
          <div>
            <div className="text-xs font-semibold uppercase tracking-[.15em] text-text-muted">
              Activity shelf
            </div>
            <h3 className="mt-1 font-serif text-xl text-text-primary">
              {lesson?.title ?? "Choose a lesson"}
            </h3>
          </div>
          <span className="rounded-full bg-surface-sunken px-2.5 py-1 text-xs text-text-muted">
            {activities.length}
          </span>
        </div>
        {lesson?.preparation !== "ready" ? (
          <div className="mt-4 rounded-xl border border-warning-muted bg-warning-muted/15 p-3 text-xs leading-5 text-warning-fg">
            Prepare the selected lesson before creating a practical
            activity.
          </div>
        ) : null}
        {activities.length === 0 ? (
          <div className="mt-4 rounded-xl border border-dashed border-border-subtle bg-surface-sunken/60 p-5 text-center">
            <Code2 className="mx-auto h-6 w-6 text-text-muted" />
            <p className="mt-2 text-xs leading-5 text-text-muted">
              No activities for this lesson yet.
            </p>
            <button
              type="button"
              disabled={lesson?.preparation !== "ready" || generatePending}
              onClick={onCreate}
              className="mt-3 text-xs font-semibold text-accent disabled:opacity-40"
            >
              Create first activity
            </button>
          </div>
        ) : (
          <div className="mt-4 space-y-2">
            {activities.map((activity) => (
              <button
                type="button"
                key={activity.id}
                aria-pressed={activeActivity?.id === activity.id}
                onClick={() => onSelectActivity(activity)}
                className={`w-full rounded-xl border p-3 text-left transition ${activeActivity?.id === activity.id ? "border-accent bg-accent-muted/15" : "border-border-default bg-surface-sunken/50 hover:border-accent/30"}`}
              >
                <span className="flex items-center justify-between gap-2">
                  <span className="truncate text-xs font-semibold uppercase tracking-[.12em] text-accent">
                    {activity.kind.replace("_", " ")}
                  </span>
                  <span className="text-xs capitalize text-text-muted">
                    {activity.status}
                  </span>
                </span>
                <span className="mt-1 block font-serif text-base leading-5 text-text-primary">
                  {activity.title}
                </span>
                <span className="mt-2 line-clamp-2 block text-xs leading-4 text-text-secondary">
                  {activity.brief}
                </span>
                <span className="mt-2 block text-xs text-text-muted">
                  Revision {activity.revision} ·{" "}
                  {activity.runtimeKind === "none"
                    ? "No runtime needed"
                    : activity.runtimeAvailable
                      ? activity.runtimeKind === "builtin"
                        ? "Included environment ready"
                        : "Container environment ready"
                      : "Runtime unavailable"}
                </span>
              </button>
            ))}
          </div>
        )}
      </section>
    </aside>
  );
}
