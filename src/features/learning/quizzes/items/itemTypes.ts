import type { ComponentType } from 'react';

import type { FormItem, ItemDraft } from '@/features/learning/quizzes/items/itemResponse';
import type { LearningAssessmentItemResultDto, LearningKeyRevealDto } from '@/lib/bindings';

export type EditorProps = {
  item: FormItem;
  draft: ItemDraft;
  disabled: boolean;
  onChange: (patch: Partial<ItemDraft>) => void;
  /** The id of the element that holds the question, for aria-labelledby. */
  promptId: string;
};

export type ReviewProps = {
  item: FormItem;
  draft: ItemDraft;
  reveal: LearningKeyRevealDto;
  result: LearningAssessmentItemResultDto | null;
};

export type ItemRenderer = {
  /** The format in words, for "QUESTION 6 · MATCHING". */
  label: string;
  Editor: ComponentType<EditorProps>;
  Review: ComponentType<ReviewProps>;
  /** Cloze draws its prompt itself, with the blanks inline. */
  ownsPrompt?: boolean;
};

export const optionLetter = (index: number) => String.fromCharCode(65 + index);
