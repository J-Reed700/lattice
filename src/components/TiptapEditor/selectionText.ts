import { withoutMarks } from '@/utils/replyPieces';

import { selectedTextWithFormulas } from './extensions/math';

/** What a selection of drawn text reads as: each formula as its source, and no citation mark. */
export function selectionText(selection: Selection): string {
  return withoutMarks(selectedTextWithFormulas(selection.getRangeAt(0)) ?? selection.toString()).trim();
}
