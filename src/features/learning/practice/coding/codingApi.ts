import { invoke } from '@tauri-apps/api/core';
import type {
  LearningLabFile,
  LearningPracticalFileDto,
  LearningPracticalRunDto,
  LearningSimulationTurnDto,
  StudyRating,
} from '@/lib/bindings';

export type CodingTask = 'problem' | 'bug_fix' | 'project';
export type Setting = 'coached' | 'mock' | 'watched';
export type Difficulty = 'easy' | 'medium' | 'hard';
export type CodingStage = 'understand' | 'plan' | 'code' | 'verify';
export type ExampleVerdict = 'right' | 'wrong' | 'invalid';
export type RunScope = 'examples' | 'all' | 'input';
export type CodeHintRung = 'nudge' | 'idea' | 'plan' | 'point_at_line';
export type HelpLevel = 'full_help' | 'hints_only' | 'no_help';

export type ScorecardCriterionId =
  | 'problem_solving'
  | 'code'
  | 'testing'
  | 'communication';

export type ScorecardStatus = 'observed' | 'insufficient_evidence';

export type EvidenceKind =
  | 'quote'
  | 'lines'
  | 'run'
  | 'example'
  | 'hint'
  | 'note';

export interface EvidenceItem {
  kind: EvidenceKind;
  text: string;
  startLine?: number | null;
  endLine?: number | null;
}

export interface ScorecardRow {
  criterionId: string;
  title: string;
  description: string;
  status: ScorecardStatus;
  /** 0..=4 when status == 'observed', null when 'insufficient_evidence'. Silence never scores 0. */
  points: number | null;
  maxPoints: number;
  evidence: EvidenceItem[];
  feedback: string;
  nextStep: string;
}

export interface Scorecard {
  sessionId: string;
  rows: ScorecardRow[];
  graderModel: string;
}

export type Compare =
  | { kind: 'exact' }
  | { kind: 'unordered' }
  | { kind: 'float_within'; tolerance: number };

export type Visibility = 'example' | 'hidden';

export interface Case {
  id: string;
  category: string;
  args: string[];
  expected?: string | null;
  compare?: Compare;
  visibility?: Visibility;
  scale?: boolean;
}

export interface Param {
  name: string;
  annotation: string;
}

export interface Signature {
  function: string;
  params: Param[];
  returns: string;
}

export interface Clarification {
  question: string;
  answer: string;
}

export interface Subgoal {
  id: string;
  label: string;
  completed?: boolean;
}

export interface PublicProblemDto {
  title: string;
  statement: string;
  signature: Signature;
  examples: Case[];
  constraints: string[];
  subgoals: Subgoal[];
  files: LearningPracticalFileDto[];
  /** Present only when setting.discloses(Hidden::WorkedSolution, submitted) is true. */
  workedSolution?: string | null;
  targetComplexity?: string | null;
  /** Total number of misreadings in the problem, so the learner sees e.g. "Covers 2 of 3 common misreadings". */
  totalMisreadings: number;
}

export interface CodingExampleDto {
  ordinal: number;
  argsJson: string;
  expectedJson: string;
  verdict: ExampleVerdict;
  /** Descriptions of misreadings this example catches. */
  catches: string[];
}

export interface CheckedCodeHint {
  rung: CodeHintRung;
  content: string;
  reason: string;
  lines?: number[];
}

export interface StageTimingDto {
  stage: CodingStage;
  elapsedMs: number;
}

export interface PatternDebriefDto {
  patternId: string;
  name: string;
  family: string;
  recognitionCue: string;
  rating: StudyRating;
  dueAt: number;
}

export interface CodingDebriefDto {
  sessionId: string;
  scorecard: Scorecard;
  patterns: PatternDebriefDto[];
  approachSummary: string;
  targetComplexity: string;
  referenceSolution: string;
  uncaughtMisreadings: string[];
  stageTimings: StageTimingDto[];
  runCount: number;
  hintsUsed: CodeHintRung[];
  setting: Setting;
}

export interface CodingSessionDto {
  id: string;
  programId: string;
  activityId: string;
  task: CodingTask;
  setting: Setting;
  stage: CodingStage;
  budgetSeconds?: number | null;
  activeMs: number;
  openedAt?: number | null;
  finishedAt?: number | null;
  revision: number;
  problem: PublicProblemDto;
  draftFiles: LearningLabFile[];
  examples: CodingExampleDto[];
  hints: CheckedCodeHint[];
  availableNextRung?: CodeHintRung | null;
  turns: LearningSimulationTurnDto[];
  runs: LearningPracticalRunDto[];
  stageTimings: StageTimingDto[];
  debrief?: CodingDebriefDto | null;
}

export interface StartCodingBuildRequestDto {
  programId: string;
  lessonId?: string | null;
  task: CodingTask;
  setting: Setting;
  language: string;
  difficulty: Difficulty;
  focus?: string | null;
}

export interface StartedCodingBuildDto {
  jobId: string;
}

export interface StartCodingSessionRequestDto {
  operationId: string;
  sessionId: string;
  programId: string;
  activityId: string;
  setting: Setting;
  budgetSeconds?: number | null;
}

export interface MoveCodingStageRequestDto {
  operationId: string;
  sessionId: string;
  stage: CodingStage;
  expectedRevision: number;
}

export interface CheckCodingExampleRequestDto {
  operationId: string;
  sessionId: string;
  argsJson: string;
  expectedJson: string;
}

export interface RunCodingSessionRequestDto {
  operationId: string;
  sessionId: string;
  scope: RunScope;
  files: LearningLabFile[];
  customInputJson?: string | null;
}

export interface AskCodingHintRequestDto {
  operationId: string;
  sessionId: string;
  expectedRevision: number;
}

export interface SendCodingTurnRequestDto {
  operationId: string;
  sessionId: string;
  content: string;
  spoken: boolean;
  expectedRevision: number;
}

export interface ObserveCodingDraftRequestDto {
  sessionId: string;
  files: LearningLabFile[];
}

export interface SetCodingClockRequestDto {
  sessionId: string;
  open: boolean;
}

export interface FinishCodingSessionRequestDto {
  operationId: string;
  sessionId: string;
  expectedRevision: number;
}

async function invokeLearning<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return await invoke<T>(`plugin:learning|${command}`, args ?? {});
}

export const codingApi = {
  startCodingBuild: async (request: StartCodingBuildRequestDto): Promise<StartedCodingBuildDto> => {
    return invokeLearning<StartedCodingBuildDto>('start_coding_build', { request });
  },

  startCodingSession: async (request: StartCodingSessionRequestDto): Promise<CodingSessionDto> => {
    return invokeLearning<CodingSessionDto>('start_coding_session', { request });
  },

  getCodingSession: async (sessionId: string): Promise<CodingSessionDto> => {
    return invokeLearning<CodingSessionDto>('get_coding_session', {
      sessionId,
      session_id: sessionId,
    });
  },

  moveCodingStage: async (request: MoveCodingStageRequestDto): Promise<CodingSessionDto> => {
    return invokeLearning<CodingSessionDto>('move_coding_stage', { request });
  },

  checkCodingExample: async (request: CheckCodingExampleRequestDto): Promise<CodingExampleDto> => {
    return invokeLearning<CodingExampleDto>('check_coding_example', { request });
  },

  runCodingSession: async (request: RunCodingSessionRequestDto): Promise<CodingSessionDto> => {
    return invokeLearning<CodingSessionDto>('run_coding_session', { request });
  },

  askCodingHint: async (request: AskCodingHintRequestDto): Promise<CodingSessionDto> => {
    return invokeLearning<CodingSessionDto>('ask_coding_hint', { request });
  },

  sendCodingTurn: async (request: SendCodingTurnRequestDto): Promise<CodingSessionDto> => {
    return invokeLearning<CodingSessionDto>('send_coding_turn', { request });
  },

  observeCodingDraft: async (request: ObserveCodingDraftRequestDto): Promise<CodingSessionDto> => {
    return invokeLearning<CodingSessionDto>('observe_coding_draft', { request });
  },

  setCodingClock: async (request: SetCodingClockRequestDto): Promise<CodingSessionDto> => {
    return invokeLearning<CodingSessionDto>('set_coding_clock', { request });
  },

  finishCodingSession: async (request: FinishCodingSessionRequestDto): Promise<CodingSessionDto> => {
    return invokeLearning<CodingSessionDto>('finish_coding_session', { request });
  },
};
