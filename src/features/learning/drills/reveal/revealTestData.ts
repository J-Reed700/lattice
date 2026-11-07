import type {
  CodeReviewFileDto,
  CodeReviewResultDto,
  CodeReviewRevealDto,
  IssueFix,
  RatedPatternDto,
  RevealedFindingDto,
  RevealedIssueDto,
  SuiteVerdict,
} from '@/lib/bindings';

export const MODEL = 'claude-haiku-5-5';
export const GRADED_AT = Date.UTC(2026, 10, 4, 15, 30);

export const STALE_BEFORE = '        recent = self.hits.get(key, [])';
export const STALE_AFTER = '        recent = [t for t in self.hits.get(key, []) if now - t < self.window]';
export const OPERATOR_BEFORE = '        if len(recent) > self.limit:';
export const OPERATOR_AFTER = '        if len(recent) >= self.limit:';
export const RECORD = '            self.hits[key] = recent';
export const LITERALS_BEFORE = '    return RateLimiter(100, 60, clock)';
export const LITERALS_AFTER = '    return RateLimiter(DEFAULT_LIMIT, DEFAULT_WINDOW, clock)';

const text = (lines: readonly string[]) => `${lines.join('\n')}\n`;

const cleanLimiter = [
  'class RateLimiter:',
  '    """Allows at most `limit` calls per key in a sliding window."""',
  '',
  '    def __init__(self, limit, window, clock):',
  '        self.limit = limit',
  '        self.window = window',
  '        self.clock = clock',
  '        self.hits = {}',
  '',
  '    def allow(self, key):',
  '        now = self.clock()',
  STALE_AFTER,
  OPERATOR_AFTER,
  RECORD,
  '            return False',
  '        recent.append(now)',
  '        self.hits[key] = recent',
  '        return True',
  '',
  '    def forget(self, key):',
  '        self.hits.pop(key, None)',
];

const cleanConfig = [
  'DEFAULT_LIMIT = 100',
  'DEFAULT_WINDOW = 60',
  '',
  '',
  'def default_limiter(clock):',
  '    from src.limiter import RateLimiter',
  '',
  LITERALS_AFTER,
];

/** The code the learner reviewed: the clean code with the three issues planted in it. */
export const REVIEWED_FILES: CodeReviewFileDto[] = [
  {
    path: 'src/limiter.py',
    text: text(cleanLimiter.map((line) => (line === STALE_AFTER ? STALE_BEFORE : line === OPERATOR_AFTER ? OPERATOR_BEFORE : line))),
  },
  { path: 'src/config.py', text: text(cleanConfig.map((line) => (line === LITERALS_AFTER ? LITERALS_BEFORE : line))) },
];

export const CLEAN_FILES: CodeReviewFileDto[] = [
  { path: 'src/limiter.py', text: text(cleanLimiter) },
  { path: 'src/config.py', text: text(cleanConfig) },
];

/** The key holds the issues in this order; the page shows them Critical, Major, Nit. */
export const MAGIC_NUMBER: RevealedIssueDto = {
  id: 'planted-1',
  pattern: 'readability.magic_number',
  patternName: 'Magic number',
  severity: 'nit',
  span: { path: 'src/config.py', start: 8, end: 8 },
  consequence: 'The default limit and window are written twice, so they can drift.',
  explanation: 'The constants are no longer used. Use them so the defaults change in one place.',
  fix: {
    path: 'src/config.py',
    lines: [
      { kind: 'removed', text: LITERALS_BEFORE },
      { kind: 'added', text: LITERALS_AFTER },
    ],
  },
  sourceQuote: null,
  outcome: 'missed',
  finding: null,
  reason: 'No comment mentions the literals.',
  alsoValid: false,
};

export const WRONG_OPERATOR: RevealedIssueDto = {
  id: 'planted-2',
  pattern: 'correctness.wrong_operator',
  patternName: 'Wrong operator or comparison',
  severity: 'major',
  span: { path: 'src/limiter.py', start: 13, end: 14 },
  consequence: 'A caller gets one call more than the limit.',
  explanation: 'The comparison lets one call too many through. It should refuse at the limit.',
  fix: {
    path: 'src/limiter.py',
    lines: [
      { kind: 'removed', text: OPERATOR_BEFORE },
      { kind: 'added', text: OPERATOR_AFTER },
      { kind: 'unchanged', text: RECORD },
    ],
  },
  sourceQuote: null,
  outcome: 'found',
  finding: 0,
  reason: 'Your comment names the operator.',
  alsoValid: false,
};

export const STALE_STATE: RevealedIssueDto = {
  id: 'planted-3',
  pattern: 'correctness.stale_state',
  patternName: 'Stale state',
  severity: 'critical',
  span: { path: 'src/limiter.py', start: 12, end: 12 },
  consequence: 'Hits never expire, so a caller who reaches the limit is refused for good.',
  explanation: 'The window is never applied to the recorded hits. Drop the ones older than the window first.',
  fix: {
    path: 'src/limiter.py',
    lines: [
      { kind: 'removed', text: STALE_BEFORE },
      { kind: 'added', text: STALE_AFTER },
    ],
  },
  sourceQuote: null,
  outcome: 'half_found',
  finding: 1,
  reason: 'Your comment is on the right line but is about speed, not about hits that never expire.',
  alsoValid: false,
};

/** Raised by a blind review, not planted: the key holds it after the planted issues. */
export const UNBOUNDED_GROWTH: RevealedIssueDto = {
  id: 'review-1',
  pattern: 'resources.unbounded_growth',
  patternName: 'Unbounded growth',
  severity: 'critical',
  span: { path: 'src/limiter.py', start: 21, end: 21 },
  consequence: 'Every key ever seen stays in memory.',
  explanation: 'The dictionary only grows. Drop keys whose window has passed.',
  fix: {
    path: 'src/limiter.py',
    lines: [
      { kind: 'removed', text: '        self.hits.pop(key, None)' },
      { kind: 'added', text: '        self.hits.clear()' },
    ],
  },
  sourceQuote: null,
  outcome: 'missed',
  finding: null,
  reason: 'No comment mentions the growth.',
  alsoValid: true,
};

export const ISSUES: RevealedIssueDto[] = [MAGIC_NUMBER, WRONG_OPERATOR, STALE_STATE];

export const FINDINGS: RevealedFindingDto[] = [
  {
    span: { path: 'src/limiter.py', start: 13, end: 13 },
    text: 'This lets one call more than the limit through.',
    severity: 'major',
    outcome: 'matches',
    issues: [1],
    reason: 'It is the changed operator.',
  },
  {
    span: { path: 'src/limiter.py', start: 12, end: 12 },
    text: 'This line looks slow.',
    severity: 'minor',
    outcome: 'matches',
    issues: [2],
    reason: 'Right line, wrong reason.',
  },
  {
    span: null,
    text: 'The dictionary grows without bound.',
    severity: null,
    outcome: 'valid_extra',
    issues: [],
    reason: 'The dictionary really does grow.',
  },
  {
    span: { path: 'src/limiter.py', start: 21, end: 21 },
    text: 'forget needs a lock.',
    severity: 'nit',
    outcome: 'false_positive',
    issues: [],
    reason: 'A single dictionary operation needs none here.',
  },
];

export const RATINGS: RatedPatternDto[] = [
  { pattern: 'readability.magic_number', patternName: 'Magic number', rating: 'again' },
  { pattern: 'correctness.wrong_operator', patternName: 'Wrong operator or comparison', rating: 'easy' },
  { pattern: 'correctness.stale_state', patternName: 'Stale state', rating: 'hard' },
];

export const SUITE_FAILED: SuiteVerdict = {
  status: 'failed',
  output: 'AssertionError: a call after the window must be allowed',
  ranBeside: [0, 2],
};

/** The learner fixed the operator and left the other two. */
export const FIXES: IssueFix[] = [
  {
    issue: 0,
    outcome: 'not_fixed',
    decidedBy: 'judge',
    reason: 'The literals are still written out.',
    output: null,
    fallback: 'There is no runtime here to run the test.',
    gradedBeside: [],
    suite: SUITE_FAILED,
  },
  {
    issue: 1,
    outcome: 'partly',
    decidedBy: 'suite',
    reason:
      'The test that shows this issue passes on your files. Your changes break what worked: the hidden suite fails on your files, even with the issues you left fixed.',
    output: 'AssertionError: a call after the window must be allowed',
    fallback: null,
    gradedBeside: [0, 2],
    suite: SUITE_FAILED,
  },
  {
    issue: 2,
    outcome: 'not_fixed',
    decidedBy: 'test',
    reason: 'The test that shows this issue still fails on your files.',
    output: 'AssertionError: an expired hit must not count',
    fallback: null,
    gradedBeside: [0, 1],
    suite: SUITE_FAILED,
  },
];

export function revealOf(patch: Partial<CodeReviewRevealDto> = {}): CodeReviewRevealDto {
  return {
    issues: ISSUES,
    findings: FINDINGS,
    score: {
      found: 1,
      halfFound: 1,
      missed: 1,
      validExtras: 1,
      falsePositives: 1,
      bySeverity: [
        { severity: 'critical', found: 0, halfFound: 1, total: 1 },
        { severity: 'major', found: 1, halfFound: 0, total: 1 },
        { severity: 'nit', found: 0, halfFound: 0, total: 1 },
      ],
      weightedRecall: 0.625,
      precision: 0.75,
      ledWithTheMostSevere: false,
    },
    ratings: RATINGS,
    fix: null,
    cleanFiles: CLEAN_FILES,
    source: null,
    ...patch,
  };
}

export function gradedResult(patch: Partial<CodeReviewResultDto> = {}): CodeReviewResultDto {
  return {
    attemptId: 'attempt-1',
    mode: 'practice',
    status: 'graded',
    startedAt: GRADED_AT - 600_000,
    submittedAt: GRADED_AT - 60_000,
    timeSpentMs: 90_000,
    drill: { title: 'A sliding-window rate limiter', material: { kind: 'code', language: 'python' }, files: REVIEWED_FILES },
    findings: FINDINGS.map(({ span, text: said, severity }) => ({ span, text: said, severity })),
    fixFiles: null,
    error: null,
    graderModel: MODEL,
    gradedAt: GRADED_AT,
    reveal: revealOf(),
    ...patch,
  };
}

export function ungradedResult(patch: Partial<CodeReviewResultDto> = {}): CodeReviewResultDto {
  return gradedResult({
    status: 'couldnt_grade',
    error: 'The grader did not answer: the model is down.',
    gradedAt: null,
    reveal: null,
    ...patch,
  });
}
