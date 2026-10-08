import type { LearningGenerationPhase } from '@/lib/bindings';

export const PREPARATION_PHASES: Record<LearningGenerationPhase, { title: string; detail: string; next: string; group: number }> = {
  references: { title: 'Preparing reference material', detail: 'Loading saved sources and building the searchable passages used to check this lesson.', next: 'Write or restore the lesson draft.', group: 0 },
  writing: { title: 'Building the lesson draft', detail: 'Writing explanations, examples, practice activities and assessments using the saved references.', next: 'Review the teaching and independently check assessment answers.', group: 1 },
  review: { title: 'Reviewing teaching and assessments', detail: 'Checking the explanations, citations and answer keys. Problems found here may send a section back for revision.', next: 'Identify factual claims and check that every section is covered.', group: 2 },
  inventory: { title: 'Identifying factual claims', detail: 'Extracting the claims made by explanations, examples and assessments so each can be checked against evidence.', next: 'Audit the inventory for missed claims and lost context.', group: 3 },
  coverage: { title: 'Checking every section is covered', detail: 'Comparing the claim inventory with the original lesson. Missing claims and important conditions must be accounted for.', next: 'Run supported examples, then check claims against references.', group: 3 },
  examples: { title: 'Checking worked examples', detail: 'Running examples where a supported runtime is available and recording what could and could not be executed.', next: 'Check factual claims against the current reference collection.', group: 3 },
  evidence: { title: 'Verifying factual claims', detail: 'Comparing each claim with saved evidence and checking for unresolved questions. Each completed comparison is saved independently.', next: 'Research or repair unresolved claims. Publish only when all required checks pass.', group: 4 },
  research: { title: 'Researching unresolved claims', detail: 'Searching for relevant, authoritative references, saving complete pages and checking whether they resolve the evidence gaps.', next: 'Review newly retrieved evidence and reuse unchanged checks; revise affected sections if needed.', group: 4 },
  repair: { title: 'Repairing the draft', detail: 'Revising affected sections using the recorded problems and evidence. Each repaired section is saved before work continues.', next: 'Review and verify the revised lesson. Unchanged checks can be reused.', group: 4 },
  publishing: { title: 'Saving the verified lesson', detail: 'Confirming the lesson and sources still match the verification report, then making the finished material available.', next: 'The prepared lesson opens for study.', group: 5 },
};

export function preparationDuration(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  if (total >= 3600) return `${Math.floor(total / 3600)}h ${Math.floor(total % 3600 / 60)}m`;
  if (total >= 60) return `${Math.floor(total / 60)}m ${total % 60}s`;
  return `${total}s`;
}
