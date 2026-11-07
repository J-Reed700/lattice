import type { AttemptMode, ReviewMaterialDto } from '@/lib/bindings';
import { codeLanguages } from '@/lib/code/languages';

/** The ways this screen offers to practise a drill. */
export type DrillMode = Extract<AttemptMode, 'practice' | 'interview'>;

export type DrillSizeId = 'short' | 'standard' | 'long';

export interface DrillSize {
  id: DrillSizeId;
  label: string;
  /** About how many lines of code, or sentences of an explanation, the drill has. */
  lines: number;
  /** How many patterns are planted, one issue each. */
  issues: number;
}

export const DRILL_SIZES: readonly DrillSize[] = [
  { id: 'short', label: 'Short', lines: 40, issues: 3 },
  { id: 'standard', label: 'Standard', lines: 80, issues: 4 },
  { id: 'long', label: 'Long', lines: 140, issues: 5 },
];

/** The sizes of a drill on a passage: sentences of an explanation, with an error planted in some of them. */
export const PASSAGE_SIZES: readonly DrillSize[] = [
  { id: 'short', label: 'Short', lines: 6, issues: 2 },
  { id: 'standard', label: 'Standard', lines: 10, issues: 3 },
  { id: 'long', label: 'Long', lines: 14, issues: 4 },
];

/** What a drill reviews. */
export type ReviewKind = 'code' | 'passage';

export const DEFAULT_LANGUAGE = 'python';
export const DEFAULT_INTERVIEW_MINUTES = 10;

/** What the learner chooses before a drill starts. */
export interface DrillOptions {
  /** Code in a language, or a passage from a course. */
  material: ReviewKind;
  language: string;
  /** The course a passage is taken from; none until one is chosen. */
  programId: string | null;
  /** What the passage should be about; empty for anything in the course's sources. */
  topic: string;
  mode: DrillMode;
  size: DrillSizeId;
  /** The interview's time limit. */
  minutes: number;
}

export const DEFAULT_OPTIONS: DrillOptions = {
  material: 'code',
  language: DEFAULT_LANGUAGE,
  programId: null,
  topic: '',
  mode: 'practice',
  size: 'standard',
  minutes: DEFAULT_INTERVIEW_MINUTES,
};

export function sizesFor(material: ReviewKind): readonly DrillSize[] {
  return material === 'passage' ? PASSAGE_SIZES : DRILL_SIZES;
}

export function sizeOf(id: DrillSizeId, material: ReviewKind = 'code'): DrillSize {
  const sizes = sizesFor(material);
  return sizes.find((size) => size.id === id) ?? sizes[1];
}

/** What is asked of the backend to review: the language, or the course and the topic, if one was given. */
export function materialOf(options: DrillOptions): ReviewMaterialDto | null {
  if (options.material === 'code') return { kind: 'code', language: options.language };
  if (!options.programId) return null;
  return { kind: 'passage', programId: options.programId, topic: options.topic.trim() || null };
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

const isKind = (value: unknown): value is ReviewKind => value === 'code' || value === 'passage';
const isMode = (value: unknown): value is DrillMode => value === 'practice' || value === 'interview';
const isSize = (value: unknown): value is DrillSizeId => DRILL_SIZES.some((size) => size.id === value);

/** The last choices the learner started a drill with, or the defaults. */
export function readChoices(): DrillOptions {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(CHOICES_KEY) ?? 'null');
    if (!stored || typeof stored !== 'object') return DEFAULT_OPTIONS;
    const { material, language, programId, topic, mode, size, minutes } = stored as Record<string, unknown>;
    return {
      material: isKind(material) ? material : DEFAULT_OPTIONS.material,
      programId: typeof programId === 'string' && programId ? programId : null,
      topic: typeof topic === 'string' ? topic : DEFAULT_OPTIONS.topic,
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
