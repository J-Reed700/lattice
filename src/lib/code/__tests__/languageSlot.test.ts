import { language } from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it } from 'vitest';

import { loadLanguage, resolveLanguage } from '../languages';
import { fillLanguageSlot, languageSlot } from '../languageSlot';

const views: EditorView[] = [];
function viewFor(path: string) {
  const view = new EditorView({
    parent: document.body,
    state: EditorState.create({ doc: 'x = 1\n', extensions: languageSlot(resolveLanguage(path)) }),
  });
  views.push(view);
  return view;
}

afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
});

describe('language slot', () => {
  it('starts plain and swaps the grammar in once it loads, keeping the view and its state', async () => {
    const view = viewFor('script.lua');
    view.dispatch({ selection: { anchor: 3 } });
    expect(view.state.facet(language)).toBeNull();

    fillLanguageSlot(view, resolveLanguage('script.lua'), () => true);
    await loadLanguage(resolveLanguage('script.lua')!);
    await Promise.resolve();

    expect(view.state.facet(language)?.name).toBe('lua');
    expect(view.state.selection.main.head).toBe(3);
  });

  it('leaves the view alone once it has moved on', async () => {
    const view = viewFor('notes.r');
    fillLanguageSlot(view, resolveLanguage('notes.r'), () => false);
    await loadLanguage(resolveLanguage('notes.r')!);
    await Promise.resolve();

    expect(view.state.facet(language)).toBeNull();
  });

  it('starts colored when the grammar loaded before, and plain text has none to load', async () => {
    await loadLanguage(resolveLanguage('other.lua')!);
    expect(viewFor('other.lua').state.facet(language)?.name).toBe('lua');
    const plain = viewFor('notes.txt');
    fillLanguageSlot(plain, null, () => true);
    expect(plain.state.facet(language)).toBeNull();
  });
});
