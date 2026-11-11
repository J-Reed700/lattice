import type { CodingBuildChoice } from '@/features/learning/practice/coding/CodingEntry';
import type { ComposerRequest } from '@/features/learning/practice/practical/PracticalWorkbenchPanel';
import { newId } from '@/lib/api';
import type * as Wire from '@/lib/bindings';


import type { AttemptMode } from './usePracticeView';

export type CardId = 'explain' | 'spot' | 'code' | 'mock' | 'role';
export type FitState = 'fits' | 'does_not_fit' | 'unknown';

export interface ChoiceOption {
  value: string;
  label: string;
  description?: string;
}

/** The cards in their fixed order. */
export const CARD_ORDER: CardId[] = ['explain', 'spot', 'code', 'mock', 'role'];

export const CODING_ROUNDS = ['live_coding', 'debugging'] as const;

export function fitState(fit: Wire.PracticeFitDto | undefined | null): FitState {
  if (fit?.code === 'fits') return 'fits';
  if (fit?.code === 'does_not_fit') return 'does_not_fit';
  return 'unknown';
}

/** Code appears only for a course that teaches program code. */
export function visibleCards(fit: FitState): CardId[] {
  return CARD_ORDER.filter((card) => card !== 'code' || fit === 'fits');
}

export function isCodingRound(round: string): boolean {
  return (CODING_ROUNDS as readonly string[]).includes(round);
}

/** Live coding, debugging and system design need code to fit; technical questions and behavioral never do. */
export function mockRoundOptions(fit: FitState): ChoiceOption[] {
  const code = fit === 'fits';
  return [
    ...(code ? [{ value: 'live_coding', label: 'live coding' }] : []),
    ...(code ? [{ value: 'debugging', label: 'debugging' }] : []),
    { value: 'technical', label: 'technical questions' },
    ...(code ? [{ value: 'system_design', label: 'system design' }] : []),
    { value: 'behavioral', label: 'behavioral' },
  ];
}

export const DIFFICULTY_OPTIONS: ChoiceOption[] = [
  { value: 'easy', label: 'easy' },
  { value: 'medium', label: 'medium' },
  { value: 'hard', label: 'hard' },
];

export const TASK_OPTIONS: ChoiceOption[] = [
  { value: 'build', label: 'problem' },
  { value: 'repair', label: 'bug to fix' },
  { value: 'project', label: 'project' },
];

export const MOCK_SETTING_OPTIONS: ChoiceOption[] = [
  { value: 'mock', label: 'mock', description: 'Timed, with nothing given away.' },
  { value: 'watched', label: 'watched', description: 'An interviewer watches and asks as you work.' },
];

export const MOCK_MINUTES_OPTIONS: ChoiceOption[] = ['20', '30', '45', '60'].map((minutes) => ({
  value: minutes,
  label: `${minutes} min`,
}));

export const SCENARIO_OPTIONS: ChoiceOption[] = [
  { value: 'incident', label: 'an incident' },
  { value: 'design_review', label: 'a design review' },
  { value: 'conversation', label: 'a hard conversation' },
];

export const EXPLAIN_OPTIONS: ChoiceOption[] = [
  { value: 'own', label: 'in your own words' },
  { value: 'beginner', label: 'to a beginner', description: 'Teach it to a beginner.' },
];

/** Coding builds are written in Python only. */
export const CODING_LANGUAGE = 'python';
export const LANGUAGE_OPTIONS: ChoiceOption[] = [{ value: CODING_LANGUAGE, label: 'Python' }];
/** The difficulty of a coding round of the Mock card and of a retry under interview conditions. */
export const MOCK_DIFFICULTY: Wire.ProblemDifficulty = 'medium';

/** Mock interview carries the badge when the course has an interview date or the learner's own details mention one. */
export function suggestsInterview(profile: Pick<Wire.LearningPracticeProfileDto, 'interviewAt' | 'details'> | null | undefined): boolean {
  if (!profile) return false;
  return Boolean(profile.interviewAt) || profile.details.toLowerCase().includes('interview');
}

export type StartAction =
  | { kind: 'coding-build'; choice: CodingBuildChoice }
  | { kind: 'activity'; composer: ComposerRequest }
  | { kind: 'attempt'; mode: AttemptMode }
  | { kind: 'drill' };

export interface StartContext {
  lessonId: string | null;
  profile: Wire.LearningPracticeProfileDto | null | undefined;
}

const ROUND_KINDS: Record<string, Wire.LearningInterviewRoundKind> = {
  technical: 'technical_deep_dive',
  system_design: 'system_design',
  behavioral: 'behavioral',
};

const SCENARIO_KINDS: Record<string, Wire.LearningPracticalActivityKind> = {
  incident: 'incident',
  design_review: 'system_design',
  conversation: 'conversation',
};

/** What the Code card's task slot offers: the project opens a code lab, the rest are coding builds. */
export type CodeCardTask = Wire.CodingTask | 'project';

function asTask(value: string | undefined): CodeCardTask {
  return value === 'repair' || value === 'project' ? value : 'build';
}

function asDifficulty(value: string | undefined): Wire.ProblemDifficulty {
  return value === 'medium' || value === 'hard' ? value : 'easy';
}

/** What a card's Start does with the choices read out of its sentence. */
export function startAction(card: CardId, values: Record<string, string>, context: StartContext): StartAction {
  switch (card) {
    case 'explain':
      return { kind: 'attempt', mode: values.style === 'beginner' ? 'feynman' : 'practice' };
    case 'spot':
      return { kind: 'drill' };
    case 'code': {
      const task = asTask(values.task);
      if (task === 'project') return { kind: 'activity', composer: { id: newId(), kind: 'code_lab' } };
      return {
        kind: 'coding-build',
        choice: {
          lessonId: context.lessonId,
          task,
          setting: 'coached',
          language: CODING_LANGUAGE,
          difficulty: asDifficulty(values.difficulty),
          focus: null,
          budgetSeconds: null,
        },
      };
    }
    case 'mock': {
      const round = values.round ?? 'technical';
      if (isCodingRound(round)) {
        const minutes = Number(values.minutes);
        return {
          kind: 'coding-build',
          choice: {
            lessonId: context.lessonId,
            task: round === 'debugging' ? 'repair' : 'build',
            setting: values.setting === 'watched' ? 'watched' : 'mock',
            language: CODING_LANGUAGE,
            difficulty: MOCK_DIFFICULTY,
            focus: null,
            budgetSeconds: Number.isFinite(minutes) && minutes > 0 ? minutes * 60 : null,
          },
        };
      }
      return {
        kind: 'activity',
        composer: {
          id: newId(),
          kind: 'interview',
          interview: {
            companyName: context.profile?.company ?? null,
            targetRole: context.profile?.role ?? null,
            roundKind: ROUND_KINDS[round] ?? 'full_loop',
          },
        },
      };
    }
    case 'role':
      return { kind: 'activity', composer: { id: newId(), kind: SCENARIO_KINDS[values.scenario ?? ''] ?? 'conversation' } };
  }
}
