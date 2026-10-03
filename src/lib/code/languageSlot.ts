import { Compartment, type Extension } from '@codemirror/state';

import { logger } from '@/utils/logger';

import { loadedLanguage, loadLanguage, type CodeLanguage } from './languages';

import type { EditorView } from '@codemirror/view';

// A compartment is only a key; each editor state keeps its own contents.
const slot = new Compartment();

/**
 * The language part of a code view's extensions: the grammar at once if an
 * earlier file already loaded it, else nothing until `fillLanguageSlot`.
 */
export function languageSlot(language: CodeLanguage | null): Extension {
  return slot.of((language && loadedLanguage(language)) ?? []);
}

/**
 * Loads the grammar and swaps it into the open view, so the text shows at
 * once and the colors arrive without a remount: scroll, selection and folds
 * stay put. Skips the swap once `stillWanted` says the view moved on.
 */
export function fillLanguageSlot(view: EditorView, language: CodeLanguage | null, stillWanted: () => boolean): void {
  if (!language) return;
  const loaded = loadedLanguage(language);
  if (loaded && slot.get(view.state) === loaded) return;
  loadLanguage(language).then(
    (support) => {
      if (stillWanted() && slot.get(view.state) !== support) view.dispatch({ effects: slot.reconfigure(support) });
    },
    (error: unknown) => logger.warn(`Could not load the ${language.label} grammar; showing plain text`, { error }),
  );
}
