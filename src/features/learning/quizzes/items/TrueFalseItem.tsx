import { ChoiceGroup, ChoiceReview } from '@/features/learning/quizzes/items/ChoiceItem';
import type { EditorProps, ReviewProps } from '@/features/learning/quizzes/items/itemTypes';

const fixed = ['True', 'False'];
const optionsOf = (options: string[]) => (options.length === 2 ? options : fixed);

/** A statement to judge: two choices, the statement is the prompt. */
export function TrueFalseEditor({ item, draft, disabled, promptId, onChange }: EditorProps) {
  return <div className="max-w-md">
    <ChoiceGroup options={optionsOf(item.options)} selected={draft.selectedIndices[0]} disabled={disabled} promptId={promptId}
      onChange={(index) => onChange({ selectedIndices: [index] })} />
  </div>;
}

export function TrueFalseReview(props: ReviewProps) {
  return <ChoiceReview {...props} item={{ ...props.item, options: optionsOf(props.item.options) }} />;
}
