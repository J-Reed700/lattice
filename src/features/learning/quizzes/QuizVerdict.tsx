import type { ReactNode } from 'react';

import { Check, Circle, CircleDot, LoaderCircle, Sparkles, TriangleAlert, UserCheck, X } from 'lucide-react';

import type { Tone, Words } from '@/features/learning/quizzes/quizCopy';

const toneStyle: Record<Tone, { icon: ReactNode; chip: string; text: string }> = {
  passed: { icon: <Check size={14} strokeWidth={2.5} />, chip: 'bg-emerald-500/12 text-emerald-800 dark:text-emerald-200', text: 'text-emerald-800 dark:text-emerald-200' },
  notPassed: { icon: <X size={14} strokeWidth={2.5} />, chip: 'bg-rose-500/12 text-rose-800 dark:text-rose-200', text: 'text-rose-800 dark:text-rose-200' },
  partial: { icon: <CircleDot size={14} />, chip: 'bg-amber-500/14 text-amber-900 dark:text-amber-200', text: 'text-amber-900 dark:text-amber-200' },
  provisional: { icon: <Sparkles size={14} />, chip: 'bg-sky-500/12 text-sky-900 dark:text-sky-200', text: 'text-sky-900 dark:text-sky-200' },
  pending: { icon: <LoaderCircle size={14} className="animate-spin motion-reduce:animate-none" />, chip: 'bg-accent/10 text-accent', text: 'text-accent' },
  warning: { icon: <TriangleAlert size={14} />, chip: 'bg-amber-500/14 text-amber-900 dark:text-amber-200', text: 'text-amber-900 dark:text-amber-200' },
  override: { icon: <UserCheck size={14} />, chip: 'bg-violet-500/12 text-violet-900 dark:text-violet-200', text: 'text-violet-900 dark:text-violet-200' },
  neutral: { icon: <Circle size={14} />, chip: 'bg-background text-text-secondary', text: 'text-text-secondary' },
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
