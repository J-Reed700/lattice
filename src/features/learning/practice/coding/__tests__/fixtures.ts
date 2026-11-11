import type * as Wire from '@/lib/bindings';

const starter = 'def find_pair(numbers, target):\n    pass\n';

export function caseView(overrides: Partial<Wire.CaseViewDto> = {}): Wire.CaseViewDto {
  return {
    caseId: 'case-1',
    category: 'a pair at the ends',
    visibility: 'example',
    mine: false,
    scale: false,
    status: 'passed',
    args: ['[1, 2, 3]', '4'],
    expected: '[0, 2]',
    actual: '[0, 2]',
    printed: null,
    error: null,
    elapsedMs: 1,
    ...overrides,
  };
}

export function codingRun(cases: Wire.CaseViewDto[], overrides: Partial<Wire.CodingRunDto> = {}): Wire.CodingRunDto {
  return {
    runId: 'run-1',
    scope: 'all',
    status: 'failed',
    cases,
    stdout: '',
    stderr: '',
    durationMs: 12,
    referenceActual: null,
    createdAt: 1_700_000_000_000,
    ...overrides,
  };
}

export function codingSession(overrides: Partial<Wire.CodingSessionDto> = {}): Wire.CodingSessionDto {
  return {
    sessionId: 'session-1',
    programId: 'program-1',
    activityId: 'activity-1',
    lessonId: null,
    task: 'build',
    setting: 'coached',
    settingsUsed: ['coached'],
    stage: 'understand',
    stages: ['understand', 'examples', 'plan', 'make', 'check', 'review'],
    problem: {
      title: 'Pair with a sum',
      statement: 'Return the positions of two numbers that add up to the target.',
      signature: {
        function: 'find_pair',
        params: [
          { name: 'numbers', annotation: 'list[int]' },
          { name: 'target', annotation: 'int' },
        ],
        returns: 'list[int]',
      },
      constraints: ['2 <= len(numbers) <= 10000'],
      difficulty: 'easy',
      language: 'python',
      examples: [caseView()],
      starter,
    },
    clock: { activeMs: 60_000, openSince: null, budgetSeconds: null },
    examples: [],
    coverage: { caught: 0, total: 3, uncaught: null },
    runs: [],
    hints: [],
    nextRung: 'nudge',
    turns: [],
    cuePending: false,
    draft: [{ path: 'solution.py', content: starter }],
    submitted: false,
    solutionAvailable: false,
    workedSolution: null,
    debrief: null,
    finished: false,
    revision: 4,
    ...overrides,
  };
}

export function codingDebrief(overrides: Partial<Wire.CodingDebriefDto> = {}): Wire.CodingDebriefDto {
  return {
    scorecard: {
      rows: [
        {
          criterionId: 'problem_solving',
          title: 'Problem solving',
          status: 'observed',
          points: 3,
          maxPoints: 4,
          evidence: [{ kind: 'quote', turnId: 'turn-1', text: 'a dict remembers what I have seen' }],
          feedback: 'You chose a sound approach.',
          nextStep: 'State the cost before coding.',
        },
        {
          criterionId: 'testing',
          title: 'Testing',
          status: 'insufficient_evidence',
          points: null,
          maxPoints: 4,
          evidence: [],
          feedback: 'There was little to go on.',
          nextStep: 'Write examples first.',
        },
      ],
    },
    facts: {
      stageMs: [
        { stage: 'understand', activeMs: 90_000 },
        { stage: 'make', activeMs: 300_000 },
      ],
      runs: 4,
      submits: 1,
      hints: [{ rung: 'nudge', count: 1 }],
      settings: ['coached', 'mock'],
    },
    strongAnswer: {
      approach: 'Remember each value and its position while scanning once.',
      complexity: 'O(n) time, O(n) space',
      reference: 'def find_pair(numbers, target):\n    seen = {}\n',
    },
    patterns: [
      {
        patternId: 'hash-map-lookup',
        name: 'Hash map lookup',
        family: 'lookup',
        cue: 'pairs that sum to a target: remember what you have seen',
        rating: 'good',
        dueAt: Date.UTC(2026, 10, 20, 12),
      },
    ],
    cases: [caseView({ caseId: 'c1', status: 'failed', category: 'an input with duplicates', visibility: 'hidden' })],
    misreadings: [
      { description: 'Uses the same element twice', caught: true },
      { description: 'Returns values instead of positions', caught: false },
    ],
    model: 'a model',
    ...overrides,
  };
}
