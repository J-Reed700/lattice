import type { JobDto } from '@/features/jobs/api';
import type { AttemptMode, CodeReviewDrillDto, CodeReviewResultDto } from '@/lib/bindings';

import { DEFAULT_INTERVIEW_MINUTES, type DrillOptions } from './drillOptions';
import { moveItem, newFinding, type DraftFinding, type FindingInput } from './findings';

import type { ReviewDraft } from './reviewDraft';

export type DrillStage = 'choose' | 'starting' | 'loading' | 'reviewing' | 'submitting' | 'grading' | 'revealed';

/** The attempt the learner is on. */
export interface Session {
  attemptId: string;
  mode: AttemptMode;
  startedAt: number;
  drill: CodeReviewDrillDto;
  /** The interview's time limit in milliseconds; none when practising. */
  limitMs: number | null;
}

/** The build of a drill being waited for. */
export interface Building {
  jobId: string;
  message: string;
  current: number;
  total: number;
  /** Why the last try failed, while the job tries again. */
  error: string | null;
}

/** What a failed call is asked again with. */
export type Retry = 'start' | 'restore' | 'submit' | 'regrade' | 'follow';

export interface Failure {
  message: string;
  retry: Retry;
}

export interface DrillState {
  stage: DrillStage;
  options: DrillOptions;
  session: Session | null;
  /** The attempt as last read after the learner handed it in. */
  result: CodeReviewResultDto | null;
  building: Building | null;
  failure: Failure | null;
  regrading: boolean;
  /** What the grading job is doing, when it says. */
  gradingMessage: string | null;
  findings: DraftFinding[];
  /** The text of each drill file the learner changed, by path. */
  fixEdits: Record<string, string>;
}

export type DrillAction =
  | { type: 'options'; patch: Partial<DrillOptions> }
  | { type: 'starting' }
  | { type: 'building'; jobId: string }
  | { type: 'job'; job: JobDto }
  | { type: 'opened'; session: Session; draft: ReviewDraft }
  | { type: 'loading' }
  | { type: 'submitting' }
  | { type: 'result'; result: CodeReviewResultDto; draft: ReviewDraft }
  | { type: 'grading-job'; message: string }
  | { type: 'regrading' }
  | { type: 'failed'; failure: Failure; stage?: DrillStage }
  | { type: 'reset' }
  | { type: 'add'; input: FindingInput }
  | { type: 'update'; id: string; input: FindingInput }
  | { type: 'remove'; id: string }
  | { type: 'move'; id: string; delta: -1 | 1 }
  | { type: 'edit-fix'; path: string; text: string }
  | { type: 'discard-fix' };

export function initialState(stage: DrillStage, options: DrillOptions): DrillState {
  return {
    stage,
    options,
    session: null,
    result: null,
    building: null,
    failure: null,
    regrading: false,
    gradingMessage: null,
    findings: [],
    fixEdits: {},
  };
}

/** An interview's time limit in milliseconds; none for any other mode. */
export function limitOf(mode: AttemptMode, minutes: number | null): number | null {
  return mode === 'interview' ? (minutes ?? DEFAULT_INTERVIEW_MINUTES) * 60_000 : null;
}

function sessionOf(result: CodeReviewResultDto, minutes: number | null): Session {
  return {
    attemptId: result.attemptId,
    mode: result.mode,
    startedAt: result.startedAt,
    drill: result.drill,
    limitMs: limitOf(result.mode, minutes),
  };
}

function stageOf(result: CodeReviewResultDto): DrillStage {
  switch (result.status) {
    case 'open':
      return 'reviewing';
    case 'graded':
      return 'revealed';
    default:
      return 'grading';
  }
}

export function reduce(state: DrillState, action: DrillAction): DrillState {
  switch (action.type) {
    case 'options':
      return { ...state, options: { ...state.options, ...action.patch } };
    case 'starting':
      return initialState('starting', state.options);
    case 'loading':
      return initialState('loading', state.options);
    case 'reset':
      return initialState('choose', state.options);
    case 'building':
      if (state.stage !== 'starting' || state.building?.jobId === action.jobId) return state;
      return { ...state, building: { jobId: action.jobId, message: '', current: 0, total: 0, error: null } };
    case 'job': {
      const { building } = state;
      if (state.stage !== 'starting' || !building || building.jobId !== action.job.id) return state;
      return {
        ...state,
        building: {
          ...building,
          message: action.job.progressMessage,
          current: action.job.progressCurrent,
          total: action.job.progressTotal,
          error: action.job.error,
        },
      };
    }
    case 'opened':
      return {
        ...initialState('reviewing', state.options),
        session: action.session,
        findings: action.draft.findings,
        fixEdits: action.draft.fix,
      };
    case 'submitting':
      return { ...state, stage: 'submitting', failure: null };
    case 'result': {
      const { result, draft } = action;
      const known = state.session;
      const restoring = known === null;
      const session = known?.attemptId === result.attemptId ? known : sessionOf(result, draft.minutes);
      return {
        ...state,
        stage: stageOf(result),
        session,
        result: result.status === 'open' ? null : result,
        building: null,
        failure: null,
        regrading: false,
        gradingMessage: result.status === 'submitted' ? state.gradingMessage : null,
        findings: restoring && result.status === 'open' ? draft.findings : state.findings,
        fixEdits: restoring && result.status === 'open' ? draft.fix : state.fixEdits,
      };
    }
    case 'grading-job':
      return { ...state, gradingMessage: action.message };
    case 'regrading':
      return { ...state, regrading: true, failure: null };
    case 'failed':
      return { ...state, failure: action.failure, stage: action.stage ?? state.stage, regrading: false };
    case 'add':
      return { ...state, findings: [...state.findings, newFinding(action.input)] };
    case 'update':
      return {
        ...state,
        findings: state.findings.map((finding) => (finding.id === action.id ? { ...finding, ...action.input } : finding)),
      };
    case 'remove':
      return { ...state, findings: state.findings.filter((finding) => finding.id !== action.id) };
    case 'move': {
      const index = state.findings.findIndex((finding) => finding.id === action.id);
      return index < 0 ? state : { ...state, findings: moveItem(state.findings, index, action.delta) };
    }
    case 'edit-fix': {
      const original = state.session?.drill.files.find((file) => file.path === action.path)?.text;
      if (original === undefined) return state;
      const { [action.path]: _replaced, ...rest } = state.fixEdits;
      return { ...state, fixEdits: action.text === original ? rest : { ...rest, [action.path]: action.text } };
    }
    case 'discard-fix':
      return { ...state, fixEdits: {} };
  }
}
