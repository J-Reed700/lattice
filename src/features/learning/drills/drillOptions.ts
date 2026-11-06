import type { AttemptMode } from '@/lib/bindings';
import { codeLanguages } from '@/lib/code/languages';

/** The ways this screen offers to practise a drill. */
export type DrillMode = Extract<AttemptMode, 'practice' | 'interview'>;

export type DrillSizeId = 'short' | 'standard' | 'long';

export interface DrillSize {
  id: DrillSizeId;
  label: string;
  /** About how many lines of code the drill has. */
  lines: number;
  /** How many patterns are planted, one issue each. */
  issues: number;
}

export const DRILL_SIZES: readonly DrillSize[] = [
  { id: 'short', label: 'Short', lines: 40, issues: 3 },
  { id: 'standard', label: 'Standard', lines: 80, issues: 4 },
  { id: 'long', label: 'Long', lines: 140, issues: 5 },
];

export const DEFAULT_LANGUAGE = 'python';
export const DEFAULT_INTERVIEW_MINUTES = 10;

/** What the learner chooses before a drill starts. */
export interface DrillOptions {
  language: string;
  mode: DrillMode;
  size: DrillSizeId;
  /** The interview's time limit. */
  minutes: number;
}

export const DEFAULT_OPTIONS: DrillOptions = {
  language: DEFAULT_LANGUAGE,
  mode: 'practice',
  size: 'standard',
  minutes: DEFAULT_INTERVIEW_MINUTES,
};

export function sizeOf(id: DrillSizeId): DrillSize {
  return DRILL_SIZES.find((size) => size.id === id) ?? DRILL_SIZES[1];
}

/** A language's display name; an id the registry does not know shows as itself. */
export function languageLabel(id: string): string {
  return codeLanguages.find((language) => language.id === id)?.label ?? id;
}

/** Whether a time limit is a whole number of minutes, at least one. */
export function validMinutes(minutes: number): boolean {
  return Number.isInteger(minutes) && minutes >= 1;
}

const CHOICES_KEY = 'lattice.drills.choices';

const isMode = (value: unknown): value is DrillMode => value === 'practice' || value === 'interview';
const isSize = (value: unknown): value is DrillSizeId => DRILL_SIZES.some((size) => size.id === value);

/** The last choices the learner started a drill with, or the defaults. */
export function readChoices(): DrillOptions {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(CHOICES_KEY) ?? 'null');
    if (!stored || typeof stored !== 'object') return DEFAULT_OPTIONS;
    const { language, mode, size, minutes } = stored as Record<string, unknown>;
    return {
      language: typeof language === 'string' && language ? language : DEFAULT_OPTIONS.language,
      mode: isMode(mode) ? mode : DEFAULT_OPTIONS.mode,
      size: isSize(size) ? size : DEFAULT_OPTIONS.size,
      minutes: typeof minutes === 'number' && validMinutes(minutes) ? minutes : DEFAULT_OPTIONS.minutes,
    };
  } catch {
    return DEFAULT_OPTIONS;
  }
}

export function writeChoices(options: DrillOptions): void {
  try {
    const minutes = validMinutes(options.minutes) ? options.minutes : DEFAULT_OPTIONS.minutes;
    localStorage.setItem(CHOICES_KEY, JSON.stringify({ ...options, minutes }));
  } catch {
    // The choices still hold for this session.
  }
}
