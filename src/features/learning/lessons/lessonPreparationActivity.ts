import { heldLessonIds } from '@/features/learning/lessons/review/claimReviewPresentation';
import type { LearningGenerationActivity, LearningGenerationJob, LearningGenerationPhase, LearningStatementPlan } from '@/lib/bindings';

export type PreparationPhase = { title: string; detail: string; next: string; group: number };

export const PREPARATION_PHASES: Record<LearningGenerationPhase, PreparationPhase> = {
  references: { title: 'Preparing reference material', detail: 'Loading saved sources and building the searchable passages used to check this lesson.', next: 'Write or restore the lesson draft.', group: 0 },
  writing: { title: 'Building the lesson draft', detail: 'Writing explanations, examples, practice activities and assessments using the saved references.', next: 'Review the teaching and independently check assessment answers.', group: 1 },
  review: { title: 'Reviewing teaching and assessments', detail: 'Checking the explanations, citations and answer keys. Problems found here may send a section back for revision.', next: 'Identify factual statements and check that every section is covered.', group: 2 },
  inventory: { title: 'Identifying factual statements', detail: 'Extracting the statements made by explanations, examples and assessments so each can be checked against evidence.', next: 'Audit the inventory for missed statements and lost context.', group: 3 },
  coverage: { title: 'Checking every section is covered', detail: 'Comparing the statement inventory with the original lesson. Missing statements and important conditions must be accounted for.', next: 'Run supported examples, then check statements against references.', group: 3 },
  examples: { title: 'Checking worked examples', detail: 'Running examples where a supported runtime is available and recording what could and could not be executed.', next: 'Check factual statements against the current reference collection.', group: 3 },
  evidence: { title: 'Verifying factual statements', detail: 'Comparing each statement with saved evidence and checking for unresolved questions. Each completed comparison is saved independently.', next: 'Research or repair unresolved statements. Publish only when all required checks pass.', group: 4 },
  research: { title: 'Researching unresolved statements', detail: 'Searching for relevant, authoritative references, saving complete pages and checking whether they resolve the evidence gaps.', next: 'Review newly retrieved evidence and reuse unchanged checks; revise affected sections if needed.', group: 4 },
  repair: { title: 'Repairing the draft', detail: 'Revising affected sections using the recorded problems and evidence. Each repaired section is saved before work continues.', next: 'Review and verify the revised lesson. Unchanged checks can be reused.', group: 4 },
  questions: { title: 'Writing quiz questions', detail: 'Writing practice questions from the verified lesson. Each question and its answer key is checked before it is kept; a question that fails a check is left out.', next: 'Save the lesson and its quiz questions.', group: 5 },
  publishing: { title: 'Saving the verified lesson', detail: 'Confirming the lesson and sources still match the verification report, then making the finished material available.', next: 'The prepared lesson opens for study.', group: 5 },
};

/**
 * The step a job shows: its phase, or, while a repaired draft goes back
 * through an earlier check, that check at the furthest step reached.
 */
export function shownPhase(activity: LearningGenerationActivity): PreparationPhase {
  const current = PREPARATION_PHASES[activity.phase];
  const furthest = PREPARATION_PHASES[activity.reached ?? activity.phase].group;
  if (furthest <= current.group) return current;
  return {
    title: 'Checking the repaired draft again',
    detail: `${current.title}, now on the repaired draft. Unchanged checks can be reused.`,
    next: current.next,
    group: furthest,
  };
}

/** "73 statements: 36 to check against their sources, 29 to check for consistency, 8 common knowledge". */
export function statementsLine(plan: LearningStatementPlan): string {
  const total = plan.sourced + plan.reasoning + plan.common;
  return `${total} ${total === 1 ? 'statement' : 'statements'}: ${plan.sourced} to check against their sources, ${plan.reasoning} to check for consistency, ${plan.common} common knowledge`;
}

/**
 * "112 calls so far, at most 278". The most is known once the statements are
 * sorted, and only comes down; before that the calls so far stand alone, and
 * before the first call there is nothing to say.
 */
export function callsLine(activity: LearningGenerationActivity): string | null {
  const calls = activity.modelCalls ?? 0;
  const most = activity.modelCallsCeiling ?? null;
  if (calls === 0 && most == null) return null;
  const made = `${calls} ${calls === 1 ? 'call' : 'calls'} so far`;
  return most == null ? made : `${made}, at most ${most}`;
}

/**
 * "Preparing · 40 of 44 checked" while the claims are checked, and
 * "Preparing · Repairing the draft" in any other phase: every claim can have
 * had its check while the job goes on repairing and reviewing the lesson.
 */
export function preparingLine(activity: LearningGenerationActivity | null | undefined): string {
  if (!activity) return 'Preparing';
  if (activity.phase === 'evidence' && activity.checksTotal > 0) {
    return `Preparing · ${activity.checksCompleted} of ${activity.checksTotal} checked`;
  }
  return `Preparing · ${shownPhase(activity).title}`;
}

export function preparationDuration(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  if (total >= 3600) return `${Math.floor(total / 3600)}h ${Math.floor(total % 3600 / 60)}m`;
  if (total >= 60) return `${Math.floor(total / 60)}m ${total % 60}s`;
  return `${total}s`;
}

/** What a lesson preparation job is doing, as both the full panel and the compact line show it. */
export type PreparationState = {
  /** Running, waiting for a slot, or about to start. */
  active: boolean;
  /** Paused by the user; resumes only on Resume. */
  paused: boolean;
  queued: boolean;
  /** Waiting for the model or network, and retrying by itself. */
  reconnecting: boolean;
  phase?: PreparationPhase;
  /** Lessons held for the user's review: not a failure. */
  held: string[];
};

export function preparationState(job: LearningGenerationJob | undefined, pending: boolean): PreparationState {
  const queued = job?.status === 'pending';
  return {
    active: job ? ['pending', 'running'].includes(job.status) : pending,
    paused: job?.status === 'cancelled',
    queued,
    reconnecting: queued && Boolean(job?.error),
    phase: job?.activity ? shownPhase(job.activity) : undefined,
    held: heldLessonIds(job),
  };
}

/**
 * One line for the job, for tabs other than Lessons: "Preparing lesson:
 * <title> · <current step>", "Lesson preparation failed: <title>". Null when
 * there is nothing to show: no job, nothing pending and no error.
 */
export function preparationSummary(job: LearningGenerationJob | undefined, state: PreparationState, error?: string, queuedMore = 0): string | null {
  const title = job?.activity?.lessonTitle;
  const named = (label: string) => (title ? `${label}: ${title}` : label);
  if (state.held.length > 0) return named('Lesson waiting for your review');
  if (state.reconnecting) return named('Lesson preparation will resume automatically');
  if (state.active) return `${named('Preparing lesson')} · ${state.queued ? 'Queued' : state.phase?.title ?? 'Starting'}${queuedMore > 0 ? ` · ${queuedMore} more queued` : ''}`;
  if (state.paused) return named('Lesson preparation paused');
  if (job && job.status !== 'completed') return named(`Lesson preparation ${job.status}`);
  if (error) return 'Lesson preparation failed';
  return null;
}
