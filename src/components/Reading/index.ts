/**
 * Reading surface: locating, highlighting, and capturing passages.
 *
 * Everything a viewer needs to land on a cited passage and let the reader turn
 * a selection into a reference, a journal line, or a new conversation.
 */

export { PassageHighlighter } from './PassageHighlighter';
export type { PassageHighlighterProps } from './PassageHighlighter';
export { SelectionToolbar } from './SelectionToolbar';
export type { SelectionToolbarProps } from './SelectionToolbar';
export { useTextSelection } from './useTextSelection';
export type { TextSelectionState } from './useTextSelection';
export {
  approximateScrollRatio,
  buildNeedles,
  formatSourceLocation,
  isTimestampSection,
  locatorFromSource,
  normalizeForMatch,
  recallLocation,
  rememberLocation,
  timestampSectionStartSeconds,
} from './passageLocator';
export {
  buildPassageTextRenderer,
  escapeHtml,
  findPassagePage,
} from './pdfPassageSearch';
