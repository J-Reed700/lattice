import { ChoiceEditor, ChoiceReview } from '@/features/learning/quizzes/items/ChoiceItem';
import { ClozeEditor, ClozeReview } from '@/features/learning/quizzes/items/ClozeItem';
import { ExplanationEditor, ExplanationReview } from '@/features/learning/quizzes/items/ExplanationItem';
import type { ItemRenderer } from '@/features/learning/quizzes/items/itemTypes';
import { MatchingEditor, MatchingReview } from '@/features/learning/quizzes/items/MatchingItem';
import { MultiSelectEditor, MultiSelectReview } from '@/features/learning/quizzes/items/MultiSelectItem';
import { OrderingEditor, OrderingReview } from '@/features/learning/quizzes/items/OrderingItem';
import { ArtifactNotice } from '@/features/learning/quizzes/items/ReviewParts';
import { ShortAnswerEditor, ShortAnswerReview } from '@/features/learning/quizzes/items/ShortAnswerItem';
import { TrueFalseEditor, TrueFalseReview } from '@/features/learning/quizzes/items/TrueFalseItem';
import type { LearningItemFormat } from '@/lib/bindings';

/** Format → its editor and review. The only switch on format in the quiz screens. */
export const itemRegistry: Record<LearningItemFormat, ItemRenderer> = {
  single_choice: { label: 'Single choice', Editor: ChoiceEditor, Review: ChoiceReview },
  true_false: { label: 'True or false', Editor: TrueFalseEditor, Review: TrueFalseReview },
  multi_select: { label: 'Choose all that apply', Editor: MultiSelectEditor, Review: MultiSelectReview },
  short_answer: { label: 'Short answer', Editor: ShortAnswerEditor, Review: ShortAnswerReview },
  cloze: { label: 'Fill the blanks', Editor: ClozeEditor, Review: ClozeReview, ownsPrompt: true },
  matching: { label: 'Matching', Editor: MatchingEditor, Review: MatchingReview },
  ordering: { label: 'Ordering', Editor: OrderingEditor, Review: OrderingReview },
  explanation: { label: 'Written explanation', Editor: ExplanationEditor, Review: ExplanationReview },
  artifact: { label: 'Practical task', Editor: ArtifactNotice, Review: ArtifactNotice },
};
