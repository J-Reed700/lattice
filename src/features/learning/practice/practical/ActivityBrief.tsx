import { TiptapViewer } from "@/components/TiptapEditor";
import { modeCopy } from "@/features/learning/practice/practical/ActivityComposer";
import type {
  LearningInterviewRoundKind,
  LearningPracticalActivityDto,
} from "@/lib/bindings";

const roundLabels: Record<LearningInterviewRoundKind, string> = {
  full_loop: "Full interview loop",
  technical_deep_dive: "Technical deep-dive",
  system_design: "System design & architecture",
  behavioral: "Behavioral & engineering culture",
  debugging_and_code: "Debugging & code walkthrough",
};

function websiteLink(value: string | null | undefined): string | null {
  if (!value) return null;
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password
      ? url.href
      : null;
  } catch {
    return null;
  }
}

export function ActivityBrief({
  activeActivity,
}: {
  activeActivity: LearningPracticalActivityDto;
}) {
  const companyUrl = websiteLink(activeActivity.interviewPlan?.companyUrl);
  const isInterview = activeActivity.kind === "interview" || Boolean(activeActivity.interviewPlan);
  return (
    <section className="rounded-2xl border border-border bg-surface p-4 sm:p-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="rounded-full bg-accent/10 px-2.5 py-1 text-xs font-semibold uppercase tracking-wider text-accent">
              {activeActivity.kind.replace("_", " ")}
            </span>
            <span className="text-xs text-text-muted">
              Revision {activeActivity.revision} · generated with{" "}
              {activeActivity.generatorModel}
            </span>
          </div>
          <h3 className="mt-2 break-words font-serif text-2xl text-text-primary">
            {activeActivity.title}
          </h3>
          <div className="mt-3 text-sm leading-7 text-text-secondary">
            <TiptapViewer content={activeActivity.brief} images={isInterview ? [] : undefined} />
          </div>
        </div>
        <span
          className={`shrink-0 rounded-full px-3 py-1.5 text-xs font-medium ${activeActivity.runtimeKind === "none" ? "bg-accent/10 text-accent" : activeActivity.runtimeAvailable ? "bg-emerald-500/10 text-emerald-800 dark:text-emerald-200" : "bg-amber-500/10 text-amber-900 dark:text-amber-100"}`}
        >
          {activeActivity.runtimeKind === "none" ? activeActivity.kind === "interview" ? "Interview practice" : "Review activity" : activeActivity.runtimeAvailable
            ? "Ready to run"
            : "Execution unavailable"}
        </span>
      </div>

      {activeActivity.interviewPlan && (
        <section
          aria-label="Company interview dossier"
          className="mt-5 min-w-0 rounded-xl border border-border bg-background/50 p-4 [overflow-wrap:anywhere] sm:p-5"
        >
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border pb-3">
            <div className="min-w-0">
              <div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">
                Company Interview Dossier
              </div>
              <div className="mt-1 flex flex-wrap items-center gap-2">
                <h4 className="font-serif text-lg font-medium text-text-primary">
                  {activeActivity.interviewPlan.companyName}
                </h4>
                <span className="text-sm text-text-muted">·</span>
                <span className="text-sm font-medium text-text-secondary">
                  {activeActivity.interviewPlan.targetRole}
                </span>
              </div>
            </div>
            <div className="flex max-w-full flex-wrap items-center gap-2">
              <span className="rounded-full bg-accent/10 px-2.5 py-1 text-xs font-medium text-accent">
                {roundLabels[activeActivity.interviewPlan.roundKind] ||
                  activeActivity.interviewPlan.roundKind}
              </span>
              {companyUrl && (
                <a
                  href={companyUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="max-w-full truncate rounded-full border border-border bg-surface px-2.5 py-1 text-xs text-text-muted hover:text-accent sm:max-w-xs"
                >
                  {activeActivity.interviewPlan.companyUrl}
                </a>
              )}
              {(activeActivity.interviewPlan.researchedSourceVersionIds?.length ?? 0) > 0 ? (
                <span className="rounded-full bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-800 dark:text-emerald-200">
                  {activeActivity.interviewPlan.researchedSourceVersionIds!.length} preparation source(s) in library
                </span>
              ) : (
                <span className="text-xs text-text-muted">No company sources captured</span>
              )}
            </div>
          </div>

          {activeActivity.practiceMode === "demonstrate" ? (
            <p className="mt-3 text-xs leading-5 text-text-muted">
              Preparation notes, questions, follow-up probes, and evaluation signals are hidden during unassisted demonstration. The interviewer will ask each question in turn.
            </p>
          ) : <>
          <div className="mt-3">
            <div className="text-xs font-semibold uppercase tracking-[.14em] text-text-muted">
              Company engineering &amp; culture research
            </div>
            <div className="mt-2 text-sm leading-6 text-text-secondary">
              <TiptapViewer content={activeActivity.interviewPlan.companySummary} images={[]} />
            </div>
          </div>

          {(activeActivity.interviewPlan.questions?.length ?? 0) > 0 && (
            <div className="mt-4 border-t border-border pt-4">
              <div className="mb-3 text-xs font-semibold uppercase tracking-[.14em] text-text-muted">
                Tailored Interview Questions
              </div>
              <div className="space-y-3">
                {activeActivity.interviewPlan.questions!.map((question) => (
                  <article
                    key={question.id}
                    className="rounded-xl border border-border bg-surface p-4"
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="rounded-full bg-accent/10 px-2.5 py-0.5 text-xs font-semibold text-accent">
                        {roundLabels[question.round] || question.round}
                      </span>
                    </div>
                    <div className="mt-2 text-sm font-semibold text-text-primary">
                      <TiptapViewer content={question.question} images={[]} />
                    </div>
                    <div className="mt-1 text-xs leading-5 text-text-secondary">
                      <span className="font-semibold text-text-primary">
                        Company rationale:
                      </span>{" "}
                      {question.companyRationale}
                    </div>

                    <details className="mt-3 border-t border-border pt-2 text-xs">
                        <summary className="cursor-pointer font-medium text-text-secondary hover:text-text-primary">
                          Study preparation: follow-up probes &amp; evaluation signals
                        </summary>
                        <div className="mt-2 grid gap-3 sm:grid-cols-2">
                          {question.followUpProbes?.length > 0 && (
                            <div className="rounded-lg bg-background/50 p-2.5">
                              <div className="font-semibold text-text-secondary">
                                Interviewer follow-up probes
                              </div>
                              <ul className="mt-1 list-inside list-disc space-y-1 text-text-muted">
                                {question.followUpProbes.map((probe, idx) => (
                                  <li key={idx}>{probe}</li>
                                ))}
                              </ul>
                            </div>
                          )}
                          {question.evaluationSignals?.length > 0 && (
                            <div className="rounded-lg bg-background/50 p-2.5">
                              <div className="font-semibold text-text-secondary">
                                Evaluation signals
                              </div>
                              <ul className="mt-1 list-inside list-disc space-y-1 text-text-muted">
                                {question.evaluationSignals.map((sig, idx) => (
                                  <li key={idx}>{sig}</li>
                                ))}
                              </ul>
                            </div>
                          )}
                        </div>
                    </details>
                  </article>
                ))}
              </div>
            </div>
          )}
          </>}
        </section>
      )}

      <details className="mt-4 border-t border-border pt-3">
        <summary className="cursor-pointer text-sm font-medium text-text-secondary">Success criteria &amp; allowed help</summary>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
        <div className="rounded-xl border border-border bg-background/45 p-4">
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
        <div className="rounded-xl border border-border bg-background/45 p-4">
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
