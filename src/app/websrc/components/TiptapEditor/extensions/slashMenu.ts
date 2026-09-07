import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';

export type SlashKey = 'ArrowUp' | 'ArrowDown' | 'Enter' | 'Tab' | 'Escape';

export interface SlashMenuState {
  query: string;
  /** Document position of the `/` itself. */
  from: number;
  /** Document position of the caret. */
  to: number;
}

export interface SlashMenuOptions {
  /** Called on every editor update with the current trigger state, or null when inactive. */
  onStateChange: (_state: SlashMenuState | null) => void;
  /** Return true to swallow the key. Called only while the menu is open. */
  onKeyDown: (_key: SlashKey) => boolean;
}

interface SlashPluginState {
  active: SlashMenuState | null;
  /** Text block start position the user dismissed the menu in, if any. */
  dismissedIn: number | null;
}

export const SLASH_MENU_PLUGIN_KEY = new PluginKey<SlashPluginState>('slashMenu');

/** The `/` in a text block, with up to 24 non-space characters after it. */
const TRIGGER = /(?:^|\s)\/([^\s/]{0,24})$/;

const EMPTY: SlashPluginState = { active: null, dismissedIn: null };

/**
 * Publishes "the user is typing a `/` command" as plugin state.
 *
 * Hand-rolled rather than built on `@tiptap/suggestion`, which is not installed.
 * The extension stays free of React: it reports state and delegates the four
 * navigation keys, and the component decides what to render.
 */
export const SlashMenu = Extension.create<SlashMenuOptions>({
  name: 'slashMenu',

  addOptions() {
    return {
      onStateChange: () => {},
      onKeyDown: () => false,
    };
  },

  addProseMirrorPlugins() {
    const options = this.options;

    return [
      new Plugin<SlashPluginState>({
        key: SLASH_MENU_PLUGIN_KEY,
        state: {
          init: () => EMPTY,
          apply(tr, previous, _oldState, next) {
            const selection = next.selection;
            const parent = selection.$from.parent;
            const blockStart = selection.$from.start();

            if (tr.getMeta(SLASH_MENU_PLUGIN_KEY) === 'dismiss') {
              return { active: null, dismissedIn: blockStart };
            }

            if (!selection.empty || !parent.isTextblock || parent.type.name === 'codeBlock') {
              return EMPTY;
            }

            const cursor = selection.from;
            const textBefore = next.doc.textBetween(blockStart, cursor, '\n', '￼');
            const match = TRIGGER.exec(textBefore);
            if (!match) return EMPTY;

            // Escape suppresses the menu until the caret leaves the block.
            if (previous.dismissedIn === blockStart) {
              return { active: null, dismissedIn: blockStart };
            }

            const query = match[1];
            return {
              active: { query, from: cursor - query.length - 1, to: cursor },
              dismissedIn: null,
            };
          },
        },
        view() {
          let last: SlashMenuState | null = null;
          return {
            update(view) {
              const state = SLASH_MENU_PLUGIN_KEY.getState(view.state)?.active ?? null;
              if (state === last) return;
              if (
                state &&
                state.query === last?.query &&
                state.from === last.from &&
                state.to === last.to
              ) {
                return;
              }
              last = state;
              options.onStateChange(state);
            },
            destroy() {
              last = null;
              options.onStateChange(null);
            },
          };
        },
      }),
    ];
  },

  addKeyboardShortcuts() {
    const handle = (key: SlashKey) => () => this.options.onKeyDown(key);
    return {
      ArrowUp: handle('ArrowUp'),
      ArrowDown: handle('ArrowDown'),
      Enter: handle('Enter'),
      Tab: handle('Tab'),
      Escape: handle('Escape'),
    };
  },
});
