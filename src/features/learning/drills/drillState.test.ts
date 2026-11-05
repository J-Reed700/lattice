import { describe, expect, it } from 'vitest';

import type { CodeReviewResultDto } from '@/lib/bindings';
import { jobFixture } from '@/tests/fixtures/jobs';

import { DEFAULT_OPTIONS } from './drillOptions';
import { initialState, limitOf, reduce, type DrillAction, type DrillState, type Session } from './drillState';
import { EMPTY_DRAFT } from './reviewDraft';

const drill = {
  title: 'A rate limiter',
  language: 'python',
  files: [
    { path: 'a.py', text: 'a = 1\n' },
    { path: 'b.py', text: 'b = 1\n' },
  ],
};

const session: Session = { attemptId: 'attempt-1', mode: 'practice', startedAt: 1, drill, limitMs: null };

function result(overrides: Partial<CodeReviewResultDto> = {}): CodeReviewResultDto {
  return {
    attemptId: 'attempt-1',
    mode: 'practice',
    status: 'submitted',
    startedAt: 1,
    submittedAt: 2,
    timeSpentMs: 1,
    drill,
    findings: [],
    fixFiles: null,
    error: null,
    graderModel: null,
    gradedAt: null,
    reveal: null,
    ...overrides,
  };
}

function run(state: DrillState, ...actions: DrillAction[]): DrillState {
  return actions.reduce(reduce, state);
}

const choosing = initialState('choose', DEFAULT_OPTIONS);
const reviewing = run(choosing, { type: 'starting' }, { type: 'opened', session, draft: EMPTY_DRAFT });

describe('where the screen stands', () => {
  it('opens on the choices, or on the attempt the address names', () => {
    expect(choosing.stage).toBe('choose');
    expect(initialState('loading', DEFAULT_OPTIONS).stage).toBe('loading');
  });

  it('goes from the choices to a drill being asked for, and to a review once one opens', () => {
    expect(run(choosing, { type: 'starting' }).stage).toBe('starting');
    expect(reviewing.stage).toBe('reviewing');
    expect(reviewing.session).toEqual(session);
  });

  it('follows a hand-in to its grade: waiting, could not be graded, then graded', () => {
    expect(run(reviewing, { type: 'submitting' }).stage).toBe('submitting');
    expect(run(reviewing, { type: 'result', result: result({ status: 'submitted' }), draft: EMPTY_DRAFT }).stage).toBe('grading');
    expect(run(reviewing, { type: 'result', result: result({ status: 'couldnt_grade', error: 'No answer.' }), draft: EMPTY_DRAFT }).stage).toBe('grading');
    expect(run(reviewing, { type: 'result', result: result({ status: 'graded' }), draft: EMPTY_DRAFT }).stage).toBe('revealed');
  });

  it('reopens an attempt from what the backend says of it, with the draft only while it is open', () => {
    const draft = { findings: [{ id: 'f1', span: null, text: 'Kept.', severity: null }], fix: { 'a.py': 'a = 2\n' }, minutes: 15 };
    const open = run(initialState('loading', DEFAULT_OPTIONS), { type: 'result', result: result({ status: 'open', mode: 'interview' }), draft });
    expect(open.stage).toBe('reviewing');
    expect(open.findings).toEqual(draft.findings);
    expect(open.fixEdits).toEqual(draft.fix);
    expect(open.session?.limitMs).toBe(15 * 60_000);

    const handedIn = run(initialState('loading', DEFAULT_OPTIONS), { type: 'result', result: result({ status: 'submitted' }), draft });
    expect(handedIn.stage).toBe('grading');
    expect(handedIn.findings).toEqual([]);
    expect(handedIn.result?.status).toBe('submitted');
  });

  it('keeps the time limit of an interview, and none for practice', () => {
    expect(limitOf('interview', 15)).toBe(900_000);
    expect(limitOf('interview', null)).toBe(600_000);
    expect(limitOf('practice', 15)).toBeNull();
  });

  it('goes back to the choices with the options and nothing else', () => {
    const back = run(reviewing, { type: 'options', patch: { language: 'rust' } }, { type: 'reset' });
    expect(back.stage).toBe('choose');
    expect(back.options.language).toBe('rust');
    expect(back.session).toBeNull();
    expect(back.findings).toEqual([]);
  });

  it('keeps a failure with the stage it happened in, or puts the screen back where the learner can ask again', () => {
    const failure = { message: 'Finding 1 has no text.', retry: 'submit' as const };
    expect(run(reviewing, { type: 'submitting' }, { type: 'failed', failure, stage: 'reviewing' })).toMatchObject({ stage: 'reviewing', failure });
    expect(run(choosing, { type: 'starting' }, { type: 'failed', failure: { message: 'No model.', retry: 'start' } })).toMatchObject({
      stage: 'starting',
      failure: { message: 'No model.' },
    });
    expect(run(reviewing, { type: 'failed', failure }, { type: 'submitting' }).failure).toBeNull();
  });
});

describe('waiting for a build', () => {
  const asking = run(choosing, { type: 'starting' });

  it('shows the progress of the build it waits for, and no other', () => {
    const building = run(asking, { type: 'building', jobId: 'job-1' });
    const progressed = run(building, { type: 'job', job: jobFixture({ id: 'job-1', progressMessage: 'Planting the defects (3 of 6)', progressCurrent: 2, progressTotal: 6 }) });
    expect(progressed.building).toMatchObject({ jobId: 'job-1', message: 'Planting the defects (3 of 6)', current: 2, total: 6 });
    expect(run(building, { type: 'job', job: jobFixture({ id: 'job-2', progressMessage: 'Elsewhere' }) })).toBe(building);
  });

  it('is told of the same build twice without losing its progress', () => {
    const progressed = run(asking, { type: 'building', jobId: 'job-1' }, { type: 'job', job: jobFixture({ id: 'job-1', progressMessage: 'Reviewing the code (2 of 6)' }) });
    expect(run(progressed, { type: 'building', jobId: 'job-1' })).toBe(progressed);
    expect(run(progressed, { type: 'building', jobId: 'job-2' }).building?.message).toBe('');
  });
});

describe('the review being written', () => {
  const add = (state: DrillState, text: string) => run(state, { type: 'add', input: { span: null, text, severity: null } });
  const texts = (state: DrillState) => state.findings.map((finding) => finding.text);

  it('adds comments in order, edits them in place, moves them and deletes them', () => {
    let state = add(add(add(reviewing, 'first'), 'second'), 'third');
    expect(texts(state)).toEqual(['first', 'second', 'third']);
    const [first, , third] = state.findings;

    state = run(state, { type: 'move', id: third.id, delta: -1 });
    expect(texts(state)).toEqual(['first', 'third', 'second']);

    state = run(state, { type: 'update', id: first.id, input: { span: { path: 'a.py', start: 1, end: 1 }, text: 'first, edited', severity: 'major' } });
    expect(state.findings[0]).toMatchObject({ id: first.id, text: 'first, edited', severity: 'major', span: { path: 'a.py', start: 1, end: 1 } });

    state = run(state, { type: 'remove', id: third.id });
    expect(texts(state)).toEqual(['first, edited', 'second']);
  });

  it('ignores a move of a comment that is not there', () => {
    const state = add(reviewing, 'only');
    expect(run(state, { type: 'move', id: 'nobody', delta: 1 })).toBe(state);
  });

  it('keeps the text of a file only while it differs from the drill, and only for the drill files', () => {
    let state = run(reviewing, { type: 'edit-fix', path: 'a.py', text: 'a = 2\n' });
    expect(state.fixEdits).toEqual({ 'a.py': 'a = 2\n' });
    state = run(state, { type: 'edit-fix', path: 'a.py', text: 'a = 1\n' });
    expect(state.fixEdits).toEqual({});
    expect(run(state, { type: 'edit-fix', path: 'elsewhere.py', text: 'x' }).fixEdits).toEqual({});

    state = run(state, { type: 'edit-fix', path: 'a.py', text: 'a = 3\n' }, { type: 'edit-fix', path: 'b.py', text: 'b = 3\n' });
    expect(Object.keys(state.fixEdits).sort()).toEqual(['a.py', 'b.py']);
    expect(run(state, { type: 'discard-fix' }).fixEdits).toEqual({});
  });
});
