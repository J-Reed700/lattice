import type { AttemptMode, DrillMaterialDto } from '@/lib/bindings';
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

export const DEFAULT_INTERVIEW_MINUTES = 10;

/** What the learner chooses before a drill starts. */
export interface DrillOptions {
  /** Code in a language, or an explanation of a passage of the course's sources. */
  material: ReviewKind;
  /** The language picked for this course; none for the one the course offers. */
  language: string | null;
  /** The lesson picked; none for the one next up. */
  lessonId: string | null;
  mode: DrillMode;
  size: DrillSizeId;
  /** The interview's time limit. */
  minutes: number;
}

export const DEFAULT_OPTIONS: DrillOptions = {
  material: 'code',
  language: null,
  lessonId: null,
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

/** What is asked of the backend to review: a passage, or code in the language chosen; none for code with no language. */
export function materialOf(options: DrillOptions): DrillMaterialDto | null {
  if (options.material === 'passage') return { kind: 'passage' };
  return options.language ? { kind: 'code', language: options.language } : null;
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
const LANGUAGE_KEY = 'lattice.drills.language';

const isKind = (value: unknown): value is ReviewKind => value === 'code' || value === 'passage';
const isMode = (value: unknown): value is DrillMode => value === 'practice' || value === 'interview';
const isSize = (value: unknown): value is DrillSizeId => DRILL_SIZES.some((size) => size.id === value);

/** The last material, mode, size and time limit the learner started a drill with, or the defaults; they hold across courses. */
export function readChoices(): DrillOptions {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(CHOICES_KEY) ?? 'null');
    if (!stored || typeof stored !== 'object') return DEFAULT_OPTIONS;
    const { material, mode, size, minutes } = stored as Record<string, unknown>;
    return {
      material: isKind(material) ? material : DEFAULT_OPTIONS.material,
      language: null,
      lessonId: null,
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
    localStorage.setItem(CHOICES_KEY, JSON.stringify({ material: options.material, mode: options.mode, size: options.size, minutes }));
  } catch {
    // The choices still hold for this session.
  }
}

/** The language the learner last started a code drill of the course in; none before one. */
export function readCourseLanguage(programId: string): string | null {
  try {
    const stored = localStorage.getItem(`${LANGUAGE_KEY}.${programId}`);
    return stored || null;
  } catch {
    return null;
  }
}

export function writeCourseLanguage(programId: string, language: string): void {
  try {
    localStorage.setItem(`${LANGUAGE_KEY}.${programId}`, language);
  } catch {
    // The language still holds for this session.
  }
}
