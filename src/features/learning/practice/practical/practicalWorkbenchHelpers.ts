import type {
  LearningPracticalActivityKind,
  LearningPracticalRunStatus,
} from "@/lib/bindings";

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
  if (status === "passed") return "text-success-fg bg-success-muted";
  if (status === "failed") return "text-danger-fg bg-danger-muted";
  if (status === "running" || status === "pending")
    return "text-accent bg-accent-muted";
  return "text-text-secondary bg-surface-raised";
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
