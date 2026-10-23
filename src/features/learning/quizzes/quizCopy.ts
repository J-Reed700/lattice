import type {
  LearningAssessmentFormDto,
  LearningAssessmentItemResultDto,
  LearningAssessmentSubmissionDto,
  LearningQuizAttemptSummaryDto,
  LearningQuizCoverageDto,
  LearningQuizScopeKind,
  LearningQuizStatusDto,
} from '@/lib/bindings';

/** How a verdict looks. Every tone is always paired with words and an icon. */
export type Tone = 'passed' | 'notPassed' | 'partial' | 'provisional' | 'pending' | 'warning' | 'override' | 'neutral';

export type Words = { label: string; detail?: string; tone: Tone };

/** "85 %" with a no-break space, so the sign never wraps away from its number. */
export function percent(score: number): string {
  return `${Math.round(score * 100)} %`;
}

/** 7 → "7", 6.75 → "6.8". */
export function points(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1).replace(/\.0$/, '');
}

export function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

export function shortDate(value: number | null | undefined): string {
  if (value == null) return 'date unavailable';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'date unavailable' : date.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
}

export const scopeKindLabel: Record<LearningQuizScopeKind, string> = {
  lesson: 'Lesson quiz',
  module: 'Module test',
  course: 'Course exam',
};

export const scopeHeadline: Record<LearningQuizScopeKind, string> = {
  lesson: 'Check what stuck',
  module: 'Show what you can do',
  course: 'Bring the course together',
};

export function formScopeKind(form: Pick<LearningAssessmentFormDto, 'scope' | 'purpose'>): LearningQuizScopeKind {
  if (form.scope) return form.scope.kind;
  if (form.purpose === 'module_test') return 'module';
  if (form.purpose === 'cumulative') return 'course';
  return 'lesson';
}

export function coverageWords(coverage: LearningQuizCoverageDto, kind: LearningQuizScopeKind): string {
  const where = kind === 'course' ? 'the course' : 'this module';
  return `Covers ${coverage.coveredLessons} of ${plural(coverage.totalLessons, 'lesson')} in ${where}`;
}

function attemptScore(attempt: LearningQuizAttemptSummaryDto): string {
  return attempt.overriddenItems > 0 ? `${percent(attempt.score)}, includes your override` : percent(attempt.score);
}

/** One quiz's state in words, for rows and callouts. Pending is never a fail. */
export function quizStatusWords(status: LearningQuizStatusDto): Words {
  const preparing = status.bankStatus === 'preparing' ? (status.bankMessage || 'Preparing more questions…') : undefined;
  if (status.activeFormId) return { label: 'Paused · resume or start fresh', detail: 'Your answers are saved.', tone: 'pending' };
  const best = status.best;
  if (status.verdict === 'pending') {
    const waiting = Math.max(status.latest?.pendingItems ?? 0, best?.pendingItems ?? 0);
    return { label: `Result pending · ${plural(waiting || 1, 'written answer')} grading`, tone: 'pending' };
  }
  if (status.verdict === 'passed' && best) return { label: `Passed · ${attemptScore(best)}`, detail: preparing ?? latestDetail(status), tone: 'passed' };
  if (status.verdict === 'not_passed' && best) return { label: `Not passed yet · best ${attemptScore(best)}`, detail: preparing ?? latestDetail(status), tone: 'notPassed' };
  if (status.bankStatus === 'preparing') return { label: 'Preparing questions…', detail: status.bankMessage || undefined, tone: 'pending' };
  if (status.bankStatus === 'not_prepared') return { label: 'Questions not prepared yet', detail: status.bankMessage || undefined, tone: 'neutral' };
  if (status.bankStatus === 'failed') return { label: "Couldn't prepare questions yet", detail: status.bankMessage || 'Try again when your model is free.', tone: 'warning' };
  return { label: 'Not taken', tone: 'neutral' };
}

function latestDetail(status: LearningQuizStatusDto): string | undefined {
  const { best, latest } = status;
  if (!latest || !best || latest.formId === best.formId) return status.attempts > 1 ? `${plural(status.attempts, 'attempt')}` : undefined;
  return `latest ${percent(latest.score)} · ${plural(status.attempts, 'attempt')}`;
}

/** Retakes prefer unseen questions; any repeat is said in words. */
export function freshnessWords(status: LearningQuizStatusDto): string | undefined {
  if (!status.attempts) return undefined;
  const repeats = Math.max(0, status.formSize - status.freshItems);
  if (repeats === 0) return 'A retake uses questions you have not seen.';
  if (status.freshItems === 0) return 'You have seen every question in this bank; a retake repeats them in a new order.';
  return `A retake uses ${plural(status.freshItems, 'fresh question')} and repeats ${plural(repeats, 'question')} you have seen, each labelled "Seen before".`;
}

export type ItemVerdict = Words & { lostPoints: boolean; pending: boolean; modelGraded: boolean };

export type GradingState = { active: boolean; stopped: boolean };

/** One item's grade in words. A pending item is "Grading…" or "Couldn't grade yet", never a fail. */
export function itemVerdict(result: LearningAssessmentItemResultDto, grading: GradingState): ItemVerdict {
  const lostPoints = result.pointsEarned < result.pointsPossible;
  const share = `${points(result.pointsEarned)} of ${plural(result.pointsPossible, 'point')}`;
  const base = { lostPoints, pending: false, modelGraded: result.gradeStatus !== 'deterministic' };
  if (result.learnerOverride) return { ...base, lostPoints: false, label: 'Overridden by you', detail: 'Counted as full credit', tone: 'override' };
  switch (result.gradeStatus) {
    case 'pending':
      if (grading.active) return { ...base, lostPoints: false, pending: true, label: 'Grading…', detail: 'Your model is reading this answer', tone: 'pending' };
      return { ...base, lostPoints: false, pending: true, label: grading.stopped ? 'Grading stopped' : "Couldn't grade yet", detail: result.feedback || undefined, tone: 'warning' };
    case 'deterministic':
      if (!lostPoints) return { ...base, label: 'Correct', tone: 'passed' };
      if (result.pointsEarned <= 0) return { ...base, label: 'Not correct', detail: result.detail || undefined, tone: 'notPassed' };
      return { ...base, label: 'Partly correct', detail: result.detail || share, tone: 'partial' };
    case 'provisional':
      return { ...base, label: 'Provisional', detail: share, tone: lostPoints ? (result.pointsEarned <= 0 ? 'notPassed' : 'partial') : 'provisional' };
    case 'uncertain':
      return { ...base, label: 'Uncertain', detail: share, tone: 'warning' };
    case 'needs_review':
      return { ...base, label: 'Needs review', detail: share, tone: 'warning' };
  }
}

export function pointTotals(results: LearningAssessmentItemResultDto[]) {
  return results.reduce((sum, result) => ({ earned: sum.earned + result.pointsEarned, possible: sum.possible + result.pointsPossible }), { earned: 0, possible: 0 });
}

/** The score over the answers graded so far; written answers still grading are left out. */
export function keyedTotals(submission: LearningAssessmentSubmissionDto) {
  const totals = pointTotals(submission.itemResults.filter((result) => result.gradeStatus !== 'pending'));
  return { ...totals, score: totals.possible > 0 ? totals.earned / totals.possible : 0 };
}

/** What the result shows as its score: while answers are grading, only the keyed part, said as such. */
export function shownScore(form: LearningAssessmentFormDto) {
  const submission = form.submission;
  if (!submission) return null;
  if (submission.passed === null && form.mode !== 'practice') {
    const keyed = keyedTotals(submission);
    return { score: keyed.score, line: `keyed answers ${percent(keyed.score)} · ${points(keyed.earned)} of ${plural(keyed.possible, 'point')} graded so far`, spoken: '' };
  }
  const totals = pointTotals(submission.itemResults);
  return { score: submission.score, line: `${points(totals.earned)} of ${plural(totals.possible, 'point')}`, spoken: percent(submission.score) };
}

/** Where the score sits against the pass mark, kindly and exactly. */
export function passMarkWords(form: LearningAssessmentFormDto): string | undefined {
  const submission = form.submission;
  if (!submission || submission.passed === null || form.mode === 'practice') return undefined;
  const gap = Math.round((submission.score - form.passingScore) * 100);
  if (submission.passed) return gap > 0 ? `${gap} % above the ${percent(form.passingScore)} pass mark.` : `Right on the ${percent(form.passingScore)} pass mark.`;
  return `${Math.max(1, -gap)} % short of the ${percent(form.passingScore)} pass mark. A retake uses fresh questions.`;
}

/** The result's headline: no pass or fail until every answer is graded. */
export function submissionHeadline(form: LearningAssessmentFormDto): Words {
  const submission = form.submission;
  if (!submission) return { label: 'Result not available yet', tone: 'neutral' };
  if (form.mode === 'practice') return { label: 'Practice round', detail: 'ungraded · nothing here counts toward your result', tone: 'neutral' };
  if (submission.passed === null) {
    return { label: 'Result pending', detail: `${plural(submission.pendingItems || 1, 'written answer')} still grading`, tone: 'pending' };
  }
  return submission.passed ? { label: 'Passed', tone: 'passed' } : { label: 'Not passed yet', tone: 'notPassed' };
}
