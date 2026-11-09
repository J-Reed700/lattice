import { SaveAsReviewCard } from '@/features/learning/practice/shared/SaveAsReviewCard';

export interface MisconceptionCardProps {
  programId: string;
  idea: string;
}

/** An accepted misconception, offered as a recall card the learner corrects in their own words. */
export function MisconceptionCard({ programId, idea }: MisconceptionCardProps) {
  return (
    <SaveAsReviewCard
      programId={programId}
      front={`What is wrong with this idea, and what is right instead? “${idea}”`}
      back=""
      sourceLabel="Drafted from a misconception found in practice"
    />
  );
}
