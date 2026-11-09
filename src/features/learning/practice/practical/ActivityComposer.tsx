import { newId } from '@/lib/api';
import { getErrorMessage } from '@/lib/errorUtils';
import { useRef, useState, type RefObject } from "react";

import { LoaderCircle, Plus } from "lucide-react";

import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { LEARNER_TEXT_MAX_CHARS } from "@/features/learning/model/learnerTextLimit";
import type {
  GenerateLearningPracticalActivityRequestDto,
  LearningBuiltinRuntimeCapability,
  LearningInterviewRoundKind,
  LearningInterviewTargetDto,
  LearningLessonDto,
  LearningPracticalActivityKind,
  LearningPracticalWorkspaceDto,
  LearningPracticeMode,
  LearningProgramDto,
} from "@/lib/bindings";

export const INTERVIEW_ROUND_LABELS: Record<LearningInterviewRoundKind, string> = {
  full_loop: "Full interview loop",
  technical_deep_dive: "Technical deep-dive",
  system_design: "System design & architecture",
  behavioral: "Behavioral & engineering culture",
  debugging_and_code: "Debugging & code walkthrough",
};

export const roundOptions: { value: LearningInterviewRoundKind; label: string }[] = (
  Object.entries(INTERVIEW_ROUND_LABELS) as [LearningInterviewRoundKind, string][]
).map(([value, label]) => ({ value, label }));

const kinds: {
  id: LearningPracticalActivityKind;
  label: string;
  description: string;
}[] = [
  {
    id: "code_lab",
    label: "Code lab",
    description: "Build and run a small, contained program.",
  },
  {
    id: "project",
    label: "Project",
    description: "Make a small artifact against a brief.",
  },
  {
    id: "incident",
    label: "Incident",
    description: "Triage evidence and communicate decisions.",
  },
  {
    id: "system_design",
    label: "System design",
    description: "Propose a design and discuss tradeoffs.",
  },
  {
    id: "interview",
    label: "Interview",
    description: "Practice a role-based technical conversation.",
  },
  {
    id: "conversation",
    label: "Conversation",
    description: "Respond to a realistic counterpart.",
  },
];

export const modeCopy: Record<LearningPracticeMode, string> = {
  explore:
    "Explore the idea with sources and tutor help.",
  practice:
    "Choose the help you want available while you practice.",
  demonstrate:
    "Try independently, without in-app hints or source assistance.",
};


export function ActivityComposer({
  program,
  lesson,
  profiles,
  builtinRuntimes,
  returnFocusRef,
  onClose,
  onCreated,
  onRequestRuntimeSetup,
  initialBrief,
  initialKind,
}: {
  initialBrief?: string;
  initialKind?: LearningPracticalActivityKind;
  program: LearningProgramDto;
  lesson: LearningLessonDto | null;
  profiles: LearningPracticalWorkspaceDto["runtimeProfiles"];
  builtinRuntimes: LearningBuiltinRuntimeCapability[];
  returnFocusRef: RefObject<HTMLButtonElement | null>;
  onClose: () => void;
  onCreated: (
    request: GenerateLearningPracticalActivityRequestDto,
  ) => Promise<void>;
  onRequestRuntimeSetup: () => void;
}) {
  const [kind, setKind] = useState<LearningPracticalActivityKind>(initialKind ?? "code_lab");
  const [brief, setBrief] = useState(initialBrief ?? "");
  const [runtimeSelection, setRuntimeSelection] = useState("");
  const [mode, setMode] = useState<LearningPracticeMode>("practice");
  const [aids, setAids] = useState<string[]>(["Saved sources"]);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [companyName, setCompanyName] = useState("");
  const [companyUrl, setCompanyUrl] = useState("");
  const [targetRole, setTargetRole] = useState("");
  const [roundKind, setRoundKind] = useState<LearningInterviewRoundKind>("full_loop");
  const [companyContext, setCompanyContext] = useState("");
  const openingSetup = useRef(false);
  const submittingRef = useRef(false);
  const retryRef = useRef<{
    fingerprint: string;
    request: GenerateLearningPracticalActivityRequestDto;
  } | null>(null);
  const supportedProfiles = profiles.filter((profile) => profile.enabled);
  const supportedBuiltins = builtinRuntimes.filter(
    (runtime) => runtime.available,
  );
  const needsExecution = kind === "code_lab" || kind === "debugging";
  const offersExecution = needsExecution || kind === "project";
  const hasInterviewTarget =
    kind === "interview" &&
    Boolean(
      companyName.trim() ||
        companyUrl.trim() ||
        targetRole.trim() ||
        companyContext.trim(),
    );
  const interviewTarget: LearningInterviewTargetDto | null =
    kind === "interview" &&
    (companyName.trim() ||
      companyUrl.trim() ||
      targetRole.trim() ||
      companyContext.trim() ||
      roundKind)
      ? {
          companyName: companyName.trim() || null,
          companyUrl: companyUrl.trim() || null,
          targetRole: targetRole.trim() || null,
          companyContext: companyContext.trim() || null,
          roundKind,
        }
      : null;
  const roundLabel =
    roundOptions.find((entry) => entry.value === roundKind)?.label ?? "interview";
  const synthesizedBrief = `Conduct a tailored ${roundLabel} interview${companyName.trim() ? ` for ${companyName.trim()}` : ""}${targetRole.trim() ? ` (${targetRole.trim()})` : ""}.`;
  const effectiveBrief = brief.trim() || synthesizedBrief;
  const canSubmit =
    Boolean(lesson) &&
    Boolean(brief.trim() || hasInterviewTarget) &&
    (!needsExecution || Boolean(runtimeSelection));

  const toggleAid = (aid: string) =>
    setAids((current) =>
      current.includes(aid)
        ? current.filter((entry) => entry !== aid)
        : [...current, aid],
    );
  const submit = async () => {
    if (submittingRef.current || !canSubmit || !lesson) return;
    submittingRef.current = true;
    setSubmitting(true);
    setError(null);
    const supportsRuntime =
      kind === "code_lab" || kind === "debugging" || kind === "project";
    const builtinRuntime =
      supportsRuntime && runtimeSelection.startsWith("builtin:")
        ? (runtimeSelection.slice("builtin:".length) as "javascript" | "python")
        : null;
    const runtimeProfileId =
      supportsRuntime && !builtinRuntime ? runtimeSelection || null : null;
    const payload = {
      programId: program.summary.id,
      lessonId: lesson.id,
      expectedProgramRevision: program.summary.revision,
      kind,
      learnerBrief: effectiveBrief,
      runtimeProfileId,
      builtinRuntime,
      practiceMode: mode,
      allowedAids: mode === "demonstrate" ? [] : aids,
      ...(interviewTarget ? { interviewTarget } : {}),
    };
    const fingerprint = JSON.stringify(payload);
    let request =
      retryRef.current?.fingerprint === fingerprint
        ? retryRef.current.request
        : null;
    if (!request) {
      request = { operationId: newId(), activityId: newId(), ...payload };
      retryRef.current = { fingerprint, request };
    }
    try {
      await onCreated(request);
      retryRef.current = null;
      onClose();
    } catch (cause) {
      setError(getErrorMessage(cause));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent
        className="max-h-[90dvh] max-w-xl gap-0 overflow-y-auto rounded-2xl border border-border-default bg-surface p-5 sm:p-6"
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (!openingSetup.current) returnFocusRef.current?.focus();
        }}
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="text-xxs font-semibold uppercase tracking-[.15em] text-accent">
              New practical activity
            </div>
            <DialogTitle className="mt-1 pr-6 font-serif text-2xl text-text-primary">
              Practice in context
            </DialogTitle>
            <DialogDescription className="mt-2 text-sm leading-6 text-text-secondary">
              For {lesson?.title ?? "this lesson"}. The brief is generated from
              the accepted lesson and any saved sources.
            </DialogDescription>
          </div>
        </div>
        <label className="mt-5 block text-xs font-medium text-text-secondary">
          Activity type
          <select
            value={kind}
            onChange={(event) =>
              setKind(event.target.value as LearningPracticalActivityKind)
            }
            className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface-sunken px-3 py-2.5 text-sm text-text-primary"
          >
            {kinds.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.label} — {entry.description}
              </option>
            ))}
          </select>
        </label>
        {kind === "interview" && (
          <section className="mt-4 rounded-xl border border-border-default bg-surface-sunken p-4 space-y-3">
            <div className="text-xs font-semibold text-text-primary">
              Company &amp; Role Research
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="block text-xs font-medium text-text-secondary">
                Company name
                <input
                  type="text"
                  aria-label="Company name"
                  value={companyName}
                  onChange={(event) => setCompanyName(event.target.value)}
                  placeholder="e.g. Example Org"
                  maxLength={160}
                  className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface px-3 py-2 text-sm text-text-primary placeholder:text-text-muted"
                />
              </label>
              <label className="block text-xs font-medium text-text-secondary">
                Company website or URL
                <input
                  type="text"
                  aria-label="Company website or URL"
                  inputMode="url"
                  autoCapitalize="none"
                  autoCorrect="off"
                  value={companyUrl}
                  onChange={(event) => setCompanyUrl(event.target.value)}
                  placeholder="company.example or https://…"
                  maxLength={2048}
                  className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface px-3 py-2 text-sm text-text-primary placeholder:text-text-muted"
                />
              </label>
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="block text-xs font-medium text-text-secondary">
                Target role
                <input
                  type="text"
                  aria-label="Target role"
                  value={targetRole}
                  onChange={(event) => setTargetRole(event.target.value)}
                  placeholder="e.g., Senior Backend Engineer"
                  maxLength={160}
                  className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface px-3 py-2 text-sm text-text-primary placeholder:text-text-muted"
                />
              </label>
              <label className="block text-xs font-medium text-text-secondary">
                Interview round
                <select
                  aria-label="Interview round"
                  value={roundKind}
                  onChange={(event) =>
                    setRoundKind(
                      event.target.value as LearningInterviewRoundKind,
                    )
                  }
                  className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface px-3 py-2 text-sm text-text-primary"
                >
                  {roundOptions.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <label className="block text-xs font-medium text-text-secondary">
              Job description or company notes
              <textarea
                aria-label="Job description or company notes"
                value={companyContext}
                onChange={(event) => setCompanyContext(event.target.value)}
                placeholder="Paste the job description, tech stack, product domain, or specific areas you want the interviewer to probe…"
                maxLength={LEARNER_TEXT_MAX_CHARS}
                rows={3}
                className="mt-1.5 w-full rounded-lg border border-border-default bg-surface px-3 py-2 text-sm leading-6 text-text-primary placeholder:text-text-muted outline-hidden focus:border-accent"
              />
            </label>
            <p className="text-xs leading-5 text-text-muted">
              Provide a company URL or paste context. Lattice will research the company, save captured pages to your course Source Library, and tailor the questions to the available evidence. If research is unavailable, the dossier will explain the gaps.
            </p>
          </section>
        )}
        {offersExecution && (
          <div className="mt-4">
            <label className="block text-xs font-medium text-text-secondary">
              Execution environment
              <select
                value={runtimeSelection}
                onChange={(event) => setRuntimeSelection(event.target.value)}
                className="mt-1.5 w-full min-w-0 rounded-lg border border-border-default bg-surface-sunken px-3 py-2.5 text-sm text-text-primary"
              >
                <option value="">{needsExecution ? "Choose a language to run" : "Review this project without running code"}</option>
                <optgroup label="Ready inside Lattice">
                {supportedBuiltins.map((runtime) => (
                  <option key={runtime.id} value={`builtin:${runtime.id}`}>
                    {runtime.name} · Included with Lattice
                  </option>
                ))}
                </optgroup>
                {supportedProfiles.length > 0 && <optgroup label="Prepared on this device">
                {supportedProfiles.map((profile) => (
                  <option key={profile.id} value={profile.id}>
                    {profile.name} environment · {profile.engine}
                  </option>
                ))}
                </optgroup>}
              </select>
            </label>
            <p className="mt-2 text-xs leading-5 text-text-muted">
              {needsExecution ? "Choose where your code will run before generating the exercise." : "Choose a language for executable projects, or keep this as a review activity."}
              {supportedBuiltins.length > 0 && " Python and JavaScript can run inside Lattice."}
            </p>
            <button type="button" onClick={() => { openingSetup.current = true; onRequestRuntimeSetup(); }} className="mt-2 text-xs font-medium text-accent underline underline-offset-4">
              Set up C#, Rust, or React &amp; TypeScript
            </button>
            {supportedBuiltins.length === 0 &&
            supportedProfiles.length === 0 ? (
              <p className="mt-2 rounded-lg bg-surface-sunken p-3 text-xs leading-4 text-text-muted">
                No execution environments are ready yet.{" "}
                <button
                  type="button"
                  onClick={() => { openingSetup.current = true; onRequestRuntimeSetup(); }}
                  className="font-semibold text-accent underline underline-offset-2"
                >
                  Set up an environment
                </button>
              </p>
            ) : null}
            {builtinRuntimes
              .filter((runtime) => !runtime.available)
              .map((runtime) => (
                <p
                  key={runtime.id}
                  className="mt-2 text-xs leading-4 text-text-muted"
                >
                  {runtime.name} is not ready:{" "}
                  {runtime.reason || "availability is being checked"}.
                </p>
              ))}
          </div>
        )}
        <label className="mt-4 block text-xs font-medium text-text-secondary">
          Your focus for this activity
          <textarea
            value={brief}
            onChange={(event) => setBrief(event.target.value)}
            maxLength={LEARNER_TEXT_MAX_CHARS}
            rows={3}
            placeholder="What would you like to practice or demonstrate?"
            className="mt-1.5 w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2.5 text-sm leading-6 text-text-primary placeholder:text-text-muted outline-hidden focus:border-accent"
          />
        </label>
        <section className="mt-4 rounded-xl border border-border-default bg-surface-sunken p-4">
          <div className="text-xs font-semibold text-text-primary">
            Help during practice
          </div>
          <div
            className="mt-3 flex flex-wrap gap-2"
            role="group"
            aria-label="Practice mode"
          >
            {(
              ["explore", "practice", "demonstrate"] as LearningPracticeMode[]
            ).map((option) => (
              <button
                key={option}
                type="button"
                aria-pressed={mode === option}
                onClick={() => setMode(option)}
                className={`rounded-full border px-3 py-2 text-xs font-medium capitalize ${mode === option ? "border-accent bg-accent text-accent-fg" : "border-border-default bg-surface text-text-secondary"}`}
              >
                {option}
              </button>
            ))}
          </div>
          <p className="mt-3 text-xs leading-5 text-text-muted">
            {modeCopy[mode]}
          </p>
          {mode !== "demonstrate" && (
            <div className="mt-3 flex flex-wrap gap-2">
              {["Saved sources", "Tutor", "Notes"].map((aid) => (
                <label
                  key={aid}
                  className="inline-flex items-center gap-2 rounded-lg border border-border-default bg-surface px-3 py-2 text-xs text-text-secondary"
                >
                  <input
                    type="checkbox"
                    checked={aids.includes(aid)}
                    onChange={() => toggleAid(aid)}
                    className="accent-accent"
                  />
                  {aid}
                </label>
              ))}
            </div>
          )}
        </section>
        {error && (
          <p
            role="alert"
            className="mt-3 rounded-lg bg-danger-muted p-3 text-xs text-danger-fg"
          >
            {error}
          </p>
        )}
        <div className="mt-5 flex justify-end">
          <button
            type="button"
            disabled={!canSubmit || submitting}
            aria-busy={submitting}
            onClick={() => void submit()}
            className="inline-flex min-h-[44px] items-center gap-2 rounded-full bg-accent px-5 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-40"
          >
            {submitting ? (
              <LoaderCircle size={14} className="animate-spin" />
            ) : (
              <PlusIcon />
            )}
            {submitting
              ? kind === "interview" && interviewTarget
                ? "Researching & generating…"
                : "Generating…"
              : error
                ? "Retry generation"
                : "Generate activity"}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function PlusIcon() {
  return <Plus size={14} />;
}
