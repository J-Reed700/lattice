/**
 * Reading surface: locating, highlighting, and capturing passages.
 *
 * Everything a viewer needs to land on a cited passage and let the reader turn
 * a selection into a reference, a journal line, or a new conversation.
 */

export { PassageHighlighter } from './components/PassageHighlighter';
export type { PassageHighlighterProps } from './components/PassageHighlighter';
export { SelectionToolbar } from './components/SelectionToolbar';
export type { SelectionToolbarProps } from './components/SelectionToolbar';
export { useTextSelection } from './hooks/useTextSelection';
export type { TextSelectionState } from './hooks/useTextSelection';
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
} from '@/shared/sources/passageLocator';
export {
  buildPassageTextRenderer,
  escapeHtml,
  findPassagePage,
} from './model/pdfPassageSearch';
