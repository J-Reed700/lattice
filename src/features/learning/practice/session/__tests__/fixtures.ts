import type * as Wire from '@/lib/bindings';

export const FAKE_KIND = 'fake';

export const STAGE_LABELS: Wire.StageLabelDto[] = [
  { stage: 'understand', label: 'Understand' },
  { stage: 'examples', label: 'Examples' },
  { stage: 'plan', label: 'Plan' },
  { stage: 'make', label: 'Make it' },
  { stage: 'check', label: 'Test' },
  { stage: 'review', label: 'Review' },
];

/** A session of the fake kind: its work is a brief and a starting text, both plain strings. */
export function practiceSession(overrides: Partial<Wire.PracticeSessionDto> = {}): Wire.PracticeSessionDto {
  return {
    sessionId: 'session-1',
    programId: 'program-1',
    activityId: 'activity-1',
    lessonId: null,
    kind: FAKE_KIND,
    title: 'Reverse a word',
    difficulty: 'easy',
    form: 'build',
    setting: 'coached',
    settingsUsed: ['coached'],
    stage: 'understand',
    stages: STAGE_LABELS,
    clock: { activeMs: 60_000, openSince: null, budgetSeconds: null },
    checks: [],
    hints: [],
    nextRung: 'nudge',
    turns: [],
    cuePending: false,
    coverage: null,
    submitted: false,
    solutionAvailable: false,
    solutionShown: false,
    finished: false,
    revision: 4,
    work: { brief: 'Give the word back in reverse.', start: 'start' },
    debrief: null,
    ...overrides,
  };
}

export function practiceCheck(overrides: Partial<Wire.PracticeCheckDto> = {}): Wire.PracticeCheckDto {
  return {
    checkId: 'check-1',
    scope: 'all',
    status: 'failed',
    passed: 1,
    total: 3,
    at: 1_700_000_000_000,
    ...overrides,
  };
}

export function practiceHint(overrides: Partial<Wire.PracticeHintDto> = {}): Wire.PracticeHintDto {
  return {
    ordinal: 0,
    rung: 'nudge',
    text: 'What do you need to remember about earlier letters?',
    reason: 'It points at what to look at.',
    stage: 'make',
    checkId: null,
    inRole: false,
    at: 1,
    ...overrides,
  };
}

export function practiceDebrief(overrides: Partial<Wire.PracticeDebriefDto> = {}): Wire.PracticeDebriefDto {
  return {
    scorecard: {
      rows: [
        {
          criterionId: 'problem_solving',
          title: 'Problem solving',
          status: 'observed',
          points: 3,
          maxPoints: 4,
          evidence: [{ kind: 'quote', turnId: 'turn-1', text: 'a stack remembers what I have seen' }],
          feedback: 'You chose a sound approach.',
          nextStep: 'State the cost before you start.',
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
      checks: 4,
      submits: 1,
      hints: [{ rung: 'nudge', count: 1 }],
      settings: ['coached', 'mock'],
    },
    misreadings: [
      { description: 'Reverses the words instead of the letters', caught: true },
      { description: 'Drops the last letter', caught: false },
    ],
    techniques: [
      {
        techniqueId: 'two-pointers',
        name: 'Two pointers',
        family: 'scanning',
        cue: 'something to swap from both ends: walk two positions inward',
        rating: 'good',
        dueAt: Date.UTC(2026, 10, 20, 12),
      },
    ],
    model: 'a model',
    work: { note: 'Kind debrief text.' },
    ...overrides,
  };
}
