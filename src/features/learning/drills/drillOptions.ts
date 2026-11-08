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
export function materialOf(_options: DrillOptions): DrillMaterialDto | null {
  return null;
}

/** A language's display name; an id the registry does not know shows as itself. */
export function languageLabel(id: string): string {
  return codeLanguages.find((language) => language.id === id)?.label ?? id;
}

/** Whether a time limit is a whole number of minutes, at least one. */
export function validMinutes(minutes: number): boolean {
  return Number.isInteger(minutes) && minutes >= 1;
}

/** The last choices the learner started a drill with, or the defaults. */
export function readChoices(): DrillOptions {
  return DEFAULT_OPTIONS;
}

export function writeChoices(_options: DrillOptions): void {
  // Nothing is kept yet.
}

/** The language the learner last started a code drill of the course in; none before one. */
export function readCourseLanguage(_programId: string): string | null {
  return null;
}

export function writeCourseLanguage(_programId: string, _language: string): void {
  // Nothing is kept yet.
}
