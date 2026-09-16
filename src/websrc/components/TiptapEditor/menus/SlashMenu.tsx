import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';

import { createPortal } from 'react-dom';

import { SLASH_ITEMS, filterSlashItems } from './slashItems';
import { SLASH_MENU_PLUGIN_KEY } from '../extensions/slashMenu';


import type { SlashKey, SlashMenuState } from '../extensions/slashMenu';
import type { Editor } from '@tiptap/react';

export interface SlashHandlers {
  onStateChange: (_state: SlashMenuState | null) => void;
  onKeyDown: (_key: SlashKey) => boolean;
}

interface SlashMenuProps {
  editor: Editor;
  /** Filled in by this component; the extension reads it on every keystroke. */
  handlersRef: React.MutableRefObject<SlashHandlers>;
}

const VIEWPORT_MARGIN = 8;
const MENU_OFFSET = 6;

/**
 * The `/` block menu.
 *
 * Positioned from `coordsAtPos` into a body portal rather than through Radix:
 * a popover would take focus, and the caret has to stay in the editor for the
 * query to keep typing.
 */
export function SlashMenu({ editor, handlersRef }: SlashMenuProps) {
  const [state, setState] = useState<SlashMenuState | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const menuRef = useRef<HTMLDivElement>(null);
  const stateRef = useRef<SlashMenuState | null>(null);
  const activeIndexRef = useRef(0);

  const items = useMemo(
    () => (state ? filterSlashItems(SLASH_ITEMS, state.query) : []),
    [state],
  );
  const itemsRef = useRef(items);
  itemsRef.current = items;
  stateRef.current = state;
  activeIndexRef.current = activeIndex;

  useEffect(() => {
    setActiveIndex(0);
  }, [state?.query]);

  useLayoutEffect(() => {
    handlersRef.current = {
      onStateChange: (next) => setState(next),
      onKeyDown: (key) => {
        const current = stateRef.current;
        const list = itemsRef.current;
        if (!current || list.length === 0) return false;

        if (key === 'Escape') {
          const { view } = editor;
          view.dispatch(view.state.tr.setMeta(SLASH_MENU_PLUGIN_KEY, 'dismiss'));
          setState(null);
          return true;
        }
        if (key === 'ArrowDown') {
          setActiveIndex((index) => (index + 1) % list.length);
          return true;
        }
        if (key === 'ArrowUp') {
          setActiveIndex((index) => (index - 1 + list.length) % list.length);
          return true;
        }
        // Enter and Tab both commit.
        const item = list[activeIndexRef.current] ?? list[0];
        if (!item) return false;
        item.run(editor, { from: current.from, to: current.to });
        setState(null);
        return true;
      },
    };
  }, [editor, handlersRef]);

  // Clamp into the viewport once the popup has a measurable size.
  useLayoutEffect(() => {
    const node = menuRef.current;
    if (!node || !state) return;
    const coords = editor.view.coordsAtPos(state.from);
    const rect = node.getBoundingClientRect();

    let left = coords.left;
    let top = coords.bottom + MENU_OFFSET;
    if (left + rect.width > window.innerWidth - VIEWPORT_MARGIN) {
      left = Math.max(VIEWPORT_MARGIN, window.innerWidth - rect.width - VIEWPORT_MARGIN);
    }
    if (top + rect.height > window.innerHeight - VIEWPORT_MARGIN) {
      top = Math.max(VIEWPORT_MARGIN, coords.top - rect.height - MENU_OFFSET);
    }
    node.style.left = `${left}px`;
    node.style.top = `${top}px`;
  }, [editor, state, items.length]);

  if (!state || items.length === 0) return null;
  if (typeof document === 'undefined') return null;

  return createPortal(
    <div
      ref={menuRef}
      role="listbox"
      aria-label="Insert block"
      className="tiptap-slash-menu"
      style={{ position: 'fixed', left: 0, top: 0 }}
    >
      {items.map((item, index) => (
        <button
          key={item.id}
          type="button"
          role="option"
          aria-selected={index === activeIndex}
          onMouseEnter={() => setActiveIndex(index)}
          onMouseDown={(event) => {
            event.preventDefault();
            item.run(editor, { from: state.from, to: state.to });
            setState(null);
          }}
          className={`tiptap-slash-item ${index === activeIndex ? 'is-active' : ''}`}
        >
          <item.icon className="h-4 w-4 shrink-0 text-text-muted" strokeWidth={1.75} />
          <span className="truncate">{item.label}</span>
        </button>
      ))}
    </div>,
    document.body,
  );
}
