import { useRef, useState, type RefObject } from "react";

import { LoaderCircle, Plus } from "lucide-react";

import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import type {
  GenerateLearningPracticalActivityRequestDto,
  LearningBuiltinRuntimeCapability,
  LearningLessonDto,
  LearningPracticalActivityKind,
  LearningPracticalWorkspaceDto,
  LearningPracticeMode,
  LearningProgramDto,
} from "@/lib/bindings";

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
    id: "debugging",
    label: "Debugging",
    description: "Trace a failure and explain the repair.",
  },
  {
    id: "code_review",
    label: "Code review",
    description: "Inspect a change for risk and clarity.",
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
    id: "project",
    label: "Project",
    description: "Make a small artifact against a brief.",
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
  {
    id: "writing_revision",
    label: "Writing revision",
    description: "Revise a piece of technical writing.",
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
function uuid() { return crypto.randomUUID(); }
function errorText(error: unknown) { return error instanceof Error ? error.message : "The activity could not be prepared."; }

export function ActivityComposer({
  program,
  lesson,
  profiles,
  builtinRuntimes,
  returnFocusRef,
  onClose,
  onCreated,
  onRequestRuntimeSetup,
}: {
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
  const [kind, setKind] = useState<LearningPracticalActivityKind>("code_lab");
  const [brief, setBrief] = useState("");
  const [runtimeSelection, setRuntimeSelection] = useState("");
  const [mode, setMode] = useState<LearningPracticeMode>("practice");
  const [aids, setAids] = useState<string[]>(["Saved sources"]);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
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
  const toggleAid = (aid: string) =>
    setAids((current) =>
      current.includes(aid)
        ? current.filter((entry) => entry !== aid)
        : [...current, aid],
    );
  const submit = async () => {
    if (submittingRef.current || !lesson || !brief.trim() || (needsExecution && !runtimeSelection)) return;
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
      learnerBrief: brief.trim(),
      runtimeProfileId,
      builtinRuntime,
      practiceMode: mode,
      allowedAids: mode === "demonstrate" ? [] : aids,
    };
    const fingerprint = JSON.stringify(payload);
    let request =
      retryRef.current?.fingerprint === fingerprint
        ? retryRef.current.request
        : null;
    if (!request) {
      request = { operationId: uuid(), activityId: uuid(), ...payload };
      retryRef.current = { fingerprint, request };
    }
    try {
      await onCreated(request);
      retryRef.current = null;
      onClose();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent
        className="max-h-[90dvh] max-w-xl gap-0 overflow-y-auto rounded-2xl border border-border bg-surface p-5 sm:p-6"
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (!openingSetup.current) returnFocusRef.current?.focus();
        }}
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">
              New practical activity
            </div>
            <DialogTitle className="mt-1 pr-6 font-serif text-2xl text-text-primary">
              Practice in context
            </DialogTitle>
            <DialogDescription className="mt-2 text-sm leading-6 text-text-secondary">
              For {lesson?.title ?? "this lesson"}. The brief is generated from
              the accepted lesson and saved sources.
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
            className="mt-1.5 w-full min-w-0 rounded-lg border border-border bg-background px-3 py-2.5 text-sm"
          >
            {kinds.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.label} — {entry.description}
              </option>
            ))}
          </select>
        </label>
        {offersExecution && (
          <div className="mt-4">
            <label className="block text-xs font-medium text-text-secondary">
              Execution environment
              <select
                value={runtimeSelection}
                onChange={(event) => setRuntimeSelection(event.target.value)}
                className="mt-1.5 w-full min-w-0 rounded-lg border border-border bg-background px-3 py-2.5 text-sm"
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
              Set up C#, Rust, or React & TypeScript
            </button>
            {supportedBuiltins.length === 0 &&
            supportedProfiles.length === 0 ? (
              <p className="mt-2 rounded-lg bg-background p-3 text-[10px] leading-4 text-text-muted">
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
                  className="mt-2 text-[10px] leading-4 text-text-muted"
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
            maxLength={2000}
            rows={3}
            placeholder="What would you like to practice or demonstrate?"
            className="mt-1.5 w-full rounded-lg border border-border bg-background px-3 py-2.5 text-sm leading-6 outline-none focus:border-accent"
          />
        </label>
        <section className="mt-4 rounded-xl border border-border bg-background/50 p-4">
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
                className={`rounded-full border px-3 py-2 text-xs font-medium capitalize ${mode === option ? "border-accent bg-accent text-accent-fg" : "border-border text-text-secondary"}`}
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
                  className="inline-flex items-center gap-2 rounded-lg border border-border bg-surface px-3 py-2 text-xs text-text-secondary"
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
            className="mt-3 rounded-lg bg-rose-500/10 p-3 text-xs text-rose-700"
          >
            {error}
          </p>
        )}
        <div className="mt-5 flex justify-end">
          <button
            type="button"
            disabled={!lesson || !brief.trim() || submitting || (needsExecution && !runtimeSelection)}
            aria-busy={submitting}
            onClick={() => void submit()}
            className="inline-flex items-center gap-2 rounded-full bg-accent px-5 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-40"
          >
            {submitting ? (
              <LoaderCircle size={14} className="animate-spin" />
            ) : (
              <SparklesIcon />
            )}
            {submitting
              ? "Generating…"
              : error
                ? "Retry generation"
                : "Generate activity"}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function SparklesIcon() {
  return <Plus size={14} />;
}
