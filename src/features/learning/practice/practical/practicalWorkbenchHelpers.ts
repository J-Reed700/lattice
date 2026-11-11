import type {
  LearningPracticalActivityKind,
  LearningPracticalRunStatus,
} from "@/lib/bindings";

export function uuid() {
  return (
    globalThis.crypto?.randomUUID?.() ?? "00000000-0000-4000-8000-000000000003"
  );
}
export function unwrap<T>(
  result: { ok: true; data: T } | { ok: false; error: string },
): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
export function errorText(error: unknown) {
  return error instanceof Error
    ? error.message
    : "The request could not be completed.";
}
export function dateText(value?: number | null) {
  if (value == null) return "Time unavailable";
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? "Time unavailable"
    : date.toLocaleString();
}
export function isSimulationKind(kind: LearningPracticalActivityKind) {
  return ["incident", "system_design", "interview", "conversation"].includes(
    kind,
  );
}

export function statusStyle(status: LearningPracticalRunStatus) {
  if (status === "passed") return "text-emerald-700 dark:text-emerald-300 bg-emerald-500/10";
  if (status === "failed") return "text-rose-700 dark:text-rose-300 bg-rose-500/10";
  if (status === "running" || status === "pending")
    return "text-accent bg-accent/10";
  return "text-text-muted bg-background";
}

/**
 * A planned interview question as a short plain-text label: its spoken words
 * without code samples, image or link markup, cut at a word within `limit`
 * characters.
 */
export function questionLabel(markdown: string, limit = 36) {
  const text = markdown
    .replace(/(```|~~~)[\s\S]*?(\1|$)/g, " ")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, " ")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[`*_#>]/g, "")
    .replace(/\s+/g, " ")
    .trim();
  if (text.length <= limit) return text;
  const cut = text.slice(0, limit);
  const space = cut.lastIndexOf(" ");
  return `${(space > limit / 2 ? cut.slice(0, space) : cut).trimEnd()}…`;
}
