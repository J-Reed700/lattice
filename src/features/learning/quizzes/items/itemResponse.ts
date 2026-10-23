import type { LearningAssessmentFormItemDto, LearningAssessmentResponse } from '@/lib/bindings';

/** The learner's working answer to one item, whatever its format. */
export type ItemDraft = {
  selectedIndices: number[];
  textResponse: string;
  blankResponses: string[];
  matches: (number | null)[];
  orderedValues: string[];
};

export type FormItem = LearningAssessmentFormItemDto;

export function draftOf(item: FormItem): ItemDraft {
  const blanks = Math.max(item.blankCount, item.blankResponses.length);
  return {
    selectedIndices: [...item.selectedIndices],
    textResponse: item.textResponse ?? '',
    blankResponses: Array.from({ length: blanks }, (_, index) => item.blankResponses[index] ?? ''),
    matches: item.leftItems.map((_, index) => item.matches[index] ?? null),
    orderedValues: [...item.orderedValues],
  };
}

/** The order an ordering item shows: the learner's, else the order it was given in. */
export function shownOrder(item: FormItem, draft: ItemDraft): string[] {
  return draft.orderedValues.length ? draft.orderedValues : item.options;
}

/** Whether the item counts as answered for the rail and the submit check. */
export function isAnswered(item: FormItem, draft: ItemDraft): boolean {
  switch (item.format) {
    case 'single_choice':
    case 'true_false':
      return draft.selectedIndices.length === 1;
    case 'multi_select':
      return draft.selectedIndices.length > 0;
    case 'short_answer':
    case 'explanation':
      return draft.textResponse.trim().length > 0;
    case 'cloze':
      return draft.blankResponses.length > 0 && draft.blankResponses.every((blank) => blank.trim().length > 0);
    case 'matching':
      return draft.matches.length > 0 && draft.matches.every((match) => match !== null);
    case 'ordering':
      return draft.orderedValues.length > 0;
    case 'artifact':
      return false;
  }
}

/** The saved response for one item. Only the fields its format uses are filled. */
export function responseOf(item: FormItem, draft: ItemDraft): LearningAssessmentResponse {
  const single = item.format === 'single_choice' || item.format === 'true_false';
  const choice = single || item.format === 'multi_select';
  const written = item.format === 'short_answer' || item.format === 'explanation';
  return {
    itemId: item.id,
    selectedIndex: single ? (draft.selectedIndices[0] ?? null) : null,
    selectedIndices: choice ? [...draft.selectedIndices].sort((a, b) => a - b) : [],
    text: written ? draft.textResponse : null,
    blankResponses: item.format === 'cloze' ? draft.blankResponses : [],
    matches: item.format === 'matching' ? draft.matches : [],
    orderedValues: item.format === 'ordering' ? draft.orderedValues : [],
    artifactJson: null,
  } as LearningAssessmentResponse;
}

/** Equal fingerprints mean nothing new to save. */
export function fingerprintOf(item: FormItem, draft: ItemDraft): string {
  const { itemId: _itemId, ...fields } = responseOf(item, draft);
  return JSON.stringify(fields);
}
