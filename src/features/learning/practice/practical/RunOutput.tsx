import { CheckCircle2, Clock3, XCircle } from "lucide-react";

import { dateText, statusStyle } from "@/features/learning/practice/practical/practicalWorkbenchHelpers";
import type { LearningPracticalRunDto } from "@/lib/bindings";

export function RunOutput({
  output,
  cancelling,
  onCancel,
}: {
  output: LearningPracticalRunDto;
  cancelling: boolean;
  onCancel: () => void;
}) {
  return (
    <section
      aria-label="Run output"
      className="rounded-2xl border border-border bg-[#211f1b] p-4 text-[#f3eee6] sm:p-5"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <span
            className={`rounded-full px-2.5 py-1 text-xs font-semibold capitalize ${statusStyle(output.status)}`}
          >
            {output.status === "interrupted"
              ? "Interrupted · partial output"
              : output.status.replace("_", " ")}
          </span>
          <span className="text-xs text-[#b8b0a6]">
            {dateText(output.createdAt)} · activity revision{" "}
            {output.activityRevision}
          </span>
        </div>
        {output.status === "running" && (
          <button
            type="button"
            disabled={cancelling}
            onClick={onCancel}
            className="rounded-full border border-white/20 px-3 py-1.5 text-xs"
          >
            {cancelling ? "Cancelling…" : "Cancel run"}
          </button>
        )}
      </div>
      <div className="mt-4 grid gap-3 lg:grid-cols-2">
        <div>
          <h4 className="text-xs font-semibold uppercase tracking-[.14em] text-[#b8b0a6]">
            Output
          </h4>
          <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap wrap-break-word rounded-xl border border-white/10 bg-black/25 p-3 font-mono text-[11px] leading-5">
            {output.stdout ||
              (output.status === "running"
                ? "Waiting for runtime output…"
                : "No standard output.")}
          </pre>
        </div>
        <div>
          <h4 className="text-xs font-semibold uppercase tracking-[.14em] text-[#b8b0a6]">
            Errors
          </h4>
          <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap wrap-break-word rounded-xl border border-white/10 bg-black/25 p-3 font-mono text-[11px] leading-5">
            {output.stderr || "No error output."}
          </pre>
        </div>
      </div>
      {output.outputTruncated && (
        <p className="mt-2 text-xs text-amber-200">
          Output was truncated by the configured runtime limit.
        </p>
      )}
      {output.checks.length > 0 && (
        <div className="mt-4">
          <h4 className="text-xs font-semibold uppercase tracking-[.14em] text-[#b8b0a6]">
            Checks
          </h4>
          <div className="mt-2 grid gap-2 sm:grid-cols-2">
            {output.checks.map((check) => (
              <div
                key={check.name}
                className="rounded-lg border border-white/10 bg-white/5 p-3"
              >
                <div className="flex items-center gap-2 text-xs">
                  <span>
                    {check.status === "passed" ? (
                      <CheckCircle2
                        size={13}
                        className="text-emerald-300"
                      />
                    ) : check.status === "failed" ||
                      check.status === "error" ? (
                      <XCircle
                        size={13}
                        className="text-rose-300"
                      />
                    ) : (
                      <Clock3
                        size={13}
                        className="text-[#b8b0a6]"
                      />
                    )}
                  </span>
                  <span>{check.name}</span>
                  <span className="ml-auto capitalize text-xs text-[#b8b0a6]">
                    {check.status}
                  </span>
                </div>
                <p className="mt-1 text-xs leading-4 text-[#c8c0b5]">
                  {check.message}
                </p>
              </div>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}
