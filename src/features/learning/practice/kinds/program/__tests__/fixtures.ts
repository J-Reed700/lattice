import type * as Wire from '@/lib/bindings';

import { practiceDebrief, practiceSession, STAGE_LABELS } from '../../../session/__tests__/fixtures';

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

export function programRun(cases: Wire.CaseViewDto[], overrides: Partial<Wire.ProgramRunDto> = {}): Wire.ProgramRunDto {
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

export function programWorkDto(overrides: Partial<Wire.ProgramWorkDto> = {}): Wire.ProgramWorkDto {
  return {
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
    draft: [{ path: 'solution.py', content: starter }],
    examples: [],
    runs: [],
    hintMarks: [],
    workedSolution: null,
    ...overrides,
  };
}

/** A Program session; `work` overrides only the parts of Program's work it names. */
export function programSession(
  overrides: Partial<Wire.PracticeSessionDto> = {},
  work: Partial<Wire.ProgramWorkDto> = {},
): Wire.PracticeSessionDto {
  const fullWork = programWorkDto(work);
  return practiceSession({
    kind: 'program',
    title: fullWork.problem.title,
    stages: STAGE_LABELS.map((item) =>
      item.stage === 'make' ? { ...item, label: 'Code' } : item.stage === 'check' ? { ...item, label: 'Test' } : item,
    ),
    coverage: { caught: 0, total: 3, uncaught: null },
    checks: fullWork.runs.map((run) => ({
      checkId: run.runId,
      scope: run.scope === 'input' ? 'own' : run.scope,
      status: run.status === 'passed' ? 'passed' : 'failed',
      passed: run.cases.filter((item) => item.status === 'passed').length,
      total: run.cases.length,
      at: run.createdAt,
    })),
    work: fullWork as unknown as Wire.JsonValue,
    ...overrides,
  });
}

export function programDebrief(overrides: Partial<Wire.PracticeDebriefDto> = {}): Wire.PracticeDebriefDto {
  const work: Wire.ProgramDebriefWorkDto = {
    strongAnswer: {
      approach: 'Remember each value and its position while scanning once.',
      complexity: 'O(n) time, O(n) space',
      reference: 'def find_pair(numbers, target):\n    seen = {}\n',
    },
    cases: [caseView({ caseId: 'c1', status: 'failed', category: 'an input with duplicates', visibility: 'hidden' })],
  };
  return practiceDebrief({ work: work as unknown as Wire.JsonValue, ...overrides });
}
