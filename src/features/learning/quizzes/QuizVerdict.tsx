import type { ReactNode } from 'react';

import { Check, Circle, CircleDot, CirclePause, LoaderCircle, Sparkles, TriangleAlert, UserCheck, X } from 'lucide-react';

import type { Tone, Words } from '@/features/learning/quizzes/quizCopy';

const toneStyle: Record<Tone, { icon: ReactNode; chip: string; text: string }> = {
  passed: { icon: <Check size={14} strokeWidth={2.5} />, chip: 'bg-success-muted text-success-fg', text: 'text-success-fg' },
  notPassed: { icon: <X size={14} strokeWidth={2.5} />, chip: 'bg-danger-muted text-danger-fg', text: 'text-danger-fg' },
  partial: { icon: <CircleDot size={14} />, chip: 'bg-warning-muted text-warning-fg', text: 'text-warning-fg' },
  provisional: { icon: <Sparkles size={14} />, chip: 'bg-sky-500/12 text-sky-700 dark:text-sky-300', text: 'text-sky-700 dark:text-sky-300' },
  pending: { icon: <LoaderCircle size={14} className="animate-spin motion-reduce:animate-none" />, chip: 'bg-accent-muted text-accent', text: 'text-accent' },
  paused: { icon: <CirclePause size={14} />, chip: 'bg-accent-muted text-accent', text: 'text-accent' },
  warning: { icon: <TriangleAlert size={14} />, chip: 'bg-warning-muted text-warning-fg', text: 'text-warning-fg' },
  override: { icon: <UserCheck size={14} />, chip: 'bg-violet-500/12 text-violet-700 dark:text-violet-300', text: 'text-violet-700 dark:text-violet-300' },
  neutral: { icon: <Circle size={14} />, chip: 'bg-surface-sunken text-text-secondary', text: 'text-text-secondary' },
};

/** A verdict as an icon and words. The colour only repeats what the words say. */
export function VerdictChip({ words, size = 'sm' }: { words: Words; size?: 'sm' | 'md' }) {
  const style = toneStyle[words.tone];
  return <span className={`inline-flex max-w-full items-center gap-1.5 rounded-full font-semibold ${style.chip} ${size === 'md' ? 'px-3 py-1.5 text-sm' : 'px-2.5 py-1 text-xs'}`}>
    <span aria-hidden="true" className="shrink-0">{style.icon}</span>
    <span className="min-w-0">{words.label}</span>
  </span>;
}

/** The icon alone, for the result's big headline; the words sit beside it. */
export function VerdictIcon({ tone, className = '' }: { tone: Tone; className?: string }) {
  return <span aria-hidden="true" className={`grid shrink-0 place-items-center rounded-full ${toneStyle[tone].chip} ${className}`}>{toneStyle[tone].icon}</span>;
}

export function toneText(tone: Tone) {
  return toneStyle[tone].text;
}
