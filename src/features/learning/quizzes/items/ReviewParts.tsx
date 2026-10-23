import type { ReactNode } from 'react';

import { Check, CircleDashed, X } from 'lucide-react';

import { TiptapViewer } from '@/components/TiptapEditor';
import type { LearningAssessmentItemResultDto } from '@/lib/bindings';

export type PartState = 'right' | 'wrong' | 'missed' | 'unknown';

const partStyle: Record<PartState, { icon: ReactNode; className: string }> = {
  right: { icon: <Check size={13} aria-hidden="true" />, className: 'bg-emerald-500/12 text-emerald-800 dark:text-emerald-200' },
  wrong: { icon: <X size={13} aria-hidden="true" />, className: 'bg-rose-500/12 text-rose-800 dark:text-rose-200' },
  missed: { icon: <CircleDashed size={13} aria-hidden="true" />, className: 'bg-amber-500/12 text-amber-800 dark:text-amber-200' },
  unknown: { icon: null, className: 'bg-background text-text-secondary' },
};

/** A small verdict chip for one part of an answer: an icon and words, never colour alone. */
export function PartMark({ state, children }: { state: PartState; children: ReactNode }) {
  const style = partStyle[state];
  return <span className={`inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold ${style.className}`}>{style.icon}{children}</span>;
}

/** The item's explanation, rendered so maths and emphasis survive. */
export function KeyExplanation({ text, label = 'Why' }: { text: string; label?: string }) {
  if (!text.trim()) return null;
  return <div className="mt-4 rounded-xl bg-background/70 px-4 py-3">
    <div className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">{label}</div>
    <div className="mt-1 text-sm leading-6 text-text-secondary"><TiptapViewer content={text} /></div>
  </div>;
}

/** "You chose B; the key is D" and other one-line reasons. */
export function ReasonLine({ children }: { children: ReactNode }) {
  return <p className="mt-3 text-sm leading-6 text-text-primary">{children}</p>;
}

/** Practical tasks never appear in a quiz; if one does, it says where it belongs. */
export function ArtifactNotice() {
  return <p className="rounded-xl bg-background px-4 py-3 text-sm text-text-secondary">This task is answered in Practice › Code &amp; simulations.</p>;
}

/** The grader's verdict on one part (blank, option, pair, position), when the item was graded. */
export function partAt(result: LearningAssessmentItemResultDto | null, index: number) {
  return result?.partResults.find((part) => part.index === index);
}

/** Why one part was wrong, in the grader's words. */
export function PartWhy({ text }: { text?: string }) {
  if (!text?.trim()) return null;
  return <p className="mt-1 text-xs leading-5 text-text-secondary">{text}</p>;
}
