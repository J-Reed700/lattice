import {
  LEARNING_PROGRAMS_KEY,
  learningClaimReviewPrefix,
  learningCoursePlanKey,
  learningLessonEvidencePrefix,
  learningOutlineEvidencePrefix,
  learningPlanKey,
  learningProgramKey,
  learningQuizOverviewKey,
} from '@/features/learning/api/learningQueryKeys';
import { registerAffected } from '@/shared/edits/affected';
import { registerActionLabel } from '@/shared/edits/origins';

let registered = false;

/** Register Learning's query-invalidation groups and action-origin labels once. */
export function registerLearningAffected(): void {
  if (registered) return;
  registered = true;

  registerAffected('learning.program', async ({ container }, client) => {
    await client.invalidateQueries({ queryKey: learningProgramKey(container) });
  });

  registerAffected('learning.plan', async ({ container }, client) => {
    await Promise.all([
      client.invalidateQueries({ queryKey: learningPlanKey(container) }),
      client.invalidateQueries({ queryKey: learningCoursePlanKey(container) }),
      client.invalidateQueries({ queryKey: learningOutlineEvidencePrefix(container) }),
      client.invalidateQueries({ queryKey: LEARNING_PROGRAMS_KEY }),
    ]);
  });

  registerAffected('learning.claims', async ({ container }, client) => {
    await Promise.all([
      client.invalidateQueries({ queryKey: learningClaimReviewPrefix(container) }),
      client.invalidateQueries({ queryKey: learningLessonEvidencePrefix(container) }),
    ]);
  });

  registerAffected('learning.quizzes', async ({ container }, client) => {
    await client.invalidateQueries({ queryKey: learningQuizOverviewKey(container) });
  });

  registerAffected('learning.recall', async (_scope, client) => {
    await Promise.all([
      client.invalidateQueries({ queryKey: ['learning-recall-v2'] }),
      client.invalidateQueries({ queryKey: ['learning-memory'] }),
      client.invalidateQueries({ queryKey: ['learning-practice-due'] }),
    ]);
  });

  registerAffected('study.decks', async (_scope, client) => {
    await client.invalidateQueries({ queryKey: ['study'] });
  });

  registerActionLabel('claim.edit', 'Edited in the claim review');
  registerActionLabel('claim.remove', 'Removed in the claim review');
  registerActionLabel('claim.put_back', 'Put back in the claim review');
  registerActionLabel('claim.restore', 'Original restored in the claim review');
  registerActionLabel('teaching.remove_sentence', 'Sentence removed from a teaching note');
}
