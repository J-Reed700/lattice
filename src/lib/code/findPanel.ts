import {
  closeSearchPanel,
  findNext,
  findPrevious,
  getSearchQuery,
  search,
  SearchQuery,
  searchKeymap,
  setSearchQuery,
} from '@codemirror/search';
import { EditorSelection, type Extension, type SelectionRange } from '@codemirror/state';
import { EditorView, keymap, runScopeHandlers } from '@codemirror/view';

import { icon, icons, type IconNode } from './icons';

import type { Panel, ViewUpdate } from '@codemirror/view';

function button(label: string, glyph: IconNode, onClick: () => void): HTMLButtonElement {
  const element = document.createElement('button');
  element.type = 'button';
  element.className = 'cm-find-button';
  element.title = label;
  element.setAttribute('aria-label', label);
  element.append(icon(glyph));
  // Keep focus in the field, so Enter still steps through matches.
  element.addEventListener('mousedown', (event) => event.preventDefault());
  element.addEventListener('click', onClick);
  return element;
}

// Counting stops here; past it the bar says "9,999+".
const COUNT_LIMIT = 9_999;

type Toggle = 'caseSensitive' | 'wholeWord' | 'regexp';

/** Scrolls a match into view with a few lines of context around it. */
const revealMatch = (range: SelectionRange) => EditorView.scrollIntoView(range, { y: 'nearest', yMargin: 64 });

/**
 * The find bar: one field, a match count, three toggles and the steppers.
 * State lives in CodeMirror's search query; the bar only mirrors it.
 */
class FindBar implements Panel {
  readonly dom: HTMLElement;
  readonly top = true;
  private readonly field: HTMLElement;
  private readonly input: HTMLInputElement;
  private readonly count: HTMLElement;
  private readonly toggles: Record<Toggle, HTMLButtonElement>;
  private readonly steppers: HTMLButtonElement[];
  private query: SearchQuery;

  constructor(private readonly view: EditorView) {
    this.query = getSearchQuery(view.state);

    this.input = document.createElement('input');
    this.input.className = 'cm-find-input';
    this.input.type = 'text';
    this.input.placeholder = 'Find in file';
    this.input.spellcheck = false;
    this.input.value = this.query.search;
    this.input.setAttribute('aria-label', 'Find in file');
    // openSearchPanel focuses and selects the element marked main-field.
    this.input.setAttribute('main-field', 'true');
    this.input.addEventListener('input', () => this.commit({ search: this.input.value }));

    this.count = document.createElement('span');
    this.count.className = 'cm-find-count';
    this.count.setAttribute('aria-live', 'polite');

    this.field = document.createElement('label');
    this.field.className = 'cm-find-field';
    this.field.append(icon(icons.search, 13), this.input, this.count);

    const toggle = (key: Toggle, label: string, glyph: IconNode) =>
      button(label, glyph, () => this.commit({ [key]: !this.query[key] }));
    this.toggles = {
      caseSensitive: toggle('caseSensitive', 'Match case', icons.caseSensitive),
      wholeWord: toggle('wholeWord', 'Match whole word', icons.wholeWord),
      regexp: toggle('regexp', 'Use regular expression', icons.regexp),
    };
    this.steppers = [
      button('Previous match (⇧Enter)', icons.chevronUp, () => this.step(findPrevious)),
      button('Next match (Enter)', icons.chevronDown, () => this.step(findNext)),
    ];
    const rule = document.createElement('span');
    rule.className = 'cm-find-rule';

    this.dom = document.createElement('div');
    this.dom.className = 'cm-find';
    this.dom.setAttribute('role', 'search');
    this.dom.addEventListener('keydown', (event) => this.keydown(event));
    this.dom.append(
      this.field,
      ...Object.values(this.toggles),
      rule,
      ...this.steppers,
      button('Close (Esc)', icons.close, () => closeSearchPanel(view)),
    );
    this.render();
  }

  mount() {
    this.input.focus();
    this.input.select();
  }

  update(update: ViewUpdate) {
    let queryChanged = false;
    for (const transaction of update.transactions) {
      for (const effect of transaction.effects) {
        if (effect.is(setSearchQuery) && !effect.value.eq(this.query)) {
          // Opening the bar over selected text searches for that text.
          this.query = effect.value;
          this.input.value = effect.value.search;
          queryChanged = true;
        }
      }
    }
    if (queryChanged || update.selectionSet || update.docChanged) this.render();
  }

  private commit(change: Partial<Pick<SearchQuery, 'search' | Toggle>>) {
    const query = new SearchQuery({
      search: this.query.search,
      caseSensitive: this.query.caseSensitive,
      wholeWord: this.query.wholeWord,
      regexp: this.query.regexp,
      ...change,
    });
    if (query.eq(this.query)) return;
    this.query = query;
    this.view.dispatch({ effects: setSearchQuery.of(query) });
    // Find as you type: the first match at or after where the reader is,
    // else the first one. (findNext would select the field's text and eat
    // the next key.)
    if (query.search && query.valid) {
      const { state } = this.view;
      const after = query.getCursor(state, state.selection.main.from).next();
      const match = after.done ? query.getCursor(state).next() : after;
      if (!match.done) {
        const range = EditorSelection.range(match.value.from, match.value.to);
        this.view.dispatch({ selection: EditorSelection.create([range]), effects: revealMatch(range) });
      }
    }
    this.render();
  }

  private keydown(event: KeyboardEvent) {
    if (runScopeHandlers(this.view, event, 'search-panel')) {
      event.preventDefault();
    } else if (event.key === 'Enter' && event.target === this.input) {
      event.preventDefault();
      this.step(event.shiftKey ? findPrevious : findNext);
    }
  }

  /** Moves to a match, leaving the caret at the end of the field rather than selecting it. */
  private step(command: typeof findNext) {
    command(this.view);
    const end = this.input.value.length;
    if (this.input === this.input.ownerDocument.activeElement) this.input.setSelectionRange(end, end);
  }

  private render() {
    for (const [key, element] of Object.entries(this.toggles) as [Toggle, HTMLButtonElement][]) {
      element.setAttribute('aria-pressed', String(this.query[key]));
    }
    const { search: text, valid } = this.query;
    this.field.toggleAttribute('data-invalid', Boolean(text) && !valid);
    let label = '';
    let total = 0;
    if (text && !valid) {
      label = 'Invalid';
    } else if (text) {
      const { from, to } = this.view.state.selection.main;
      let current = 0;
      const cursor = this.query.getCursor(this.view.state);
      for (let match = cursor.next(); !match.done && total < COUNT_LIMIT; match = cursor.next()) {
        total += 1;
        if (match.value.from === from && match.value.to === to) current = total;
      }
      const shown = total >= COUNT_LIMIT ? `${COUNT_LIMIT.toLocaleString()}+` : total.toLocaleString();
      label = total === 0 ? 'No results' : current ? `${current.toLocaleString()} of ${shown}` : `${shown} found`;
    }
    this.count.textContent = label;
    this.count.toggleAttribute('data-empty', Boolean(text) && total === 0);
    for (const stepper of this.steppers) stepper.disabled = total === 0;
  }
}

const findKeys = new Set(['Mod-f', 'F3', 'Mod-g', 'Escape']);

/**
 * In-file search for a read-only view: ⌘F / Ctrl+F opens an app-styled find
 * bar over the code, Enter and ⌘G step through matches, Esc closes it.
 */
export function findPanel(): Extension {
  return [
    search({
      top: true,
      createPanel: (view) => new FindBar(view),
      scrollToMatch: revealMatch,
    }),
    // Search's own keys, minus the ones that edit or multi-select.
    keymap.of(searchKeymap.filter((binding) => binding.key && findKeys.has(binding.key))),
  ];
}
