import { useLayoutEffect, useRef } from 'react';

import './composer.css';

import type { CaretPoint } from './caretCoordinates';
import type { LucideIcon } from 'lucide-react';

/**
 * The list that opens on `/` and `@`, anchored to the caret inside the
 * composer's textarea.
 *
 * It is a command surface, not a datalist: every row says what it does and
 * where it stands, and the keys that drive it are printed along the bottom.
 * The component draws and reports clicks; the keyboard and the trigger belong
 * to `useComposerSuggest`, so the same rows serve both commands and documents.
 */

/** How wide the panel draws, so a caret near the right edge can be clamped. */
export const SUGGEST_PANEL_WIDTH = 340;

export interface SuggestItem {
  /** A command id, or a document id. */
  id: string;
  /** The row's name: `/deep`, or a file name. */
  label: string;
  /** One line about what accepting it does. */
  description: string;
  /** Where this switch stands now, when it is a switch. */
  state?: string | null;
  icon?: LucideIcon;
}

export interface ComposerSuggestProps {
  items: SuggestItem[];
  activeIndex: number;
  /** One line above the list saying what is being picked. */
  heading: string;
  /** What to say when nothing matches — or while a search is still running. */
  emptyLabel: string;
  point: CaretPoint;
  listId: string;
  onSelect: (_index: number) => void;
  onHover: (_index: number) => void;
}

export function ComposerSuggest({
  items,
  activeIndex,
  heading,
  emptyLabel,
  point,
  listId,
  onSelect,
  onHover,
}: ComposerSuggestProps) {
  const activeRowRef = useRef<HTMLButtonElement | null>(null);

  // Arrowing past the bottom of a scrolled list should not lose the lit row.
  useLayoutEffect(() => {
    activeRowRef.current?.scrollIntoView?.({ block: 'nearest' });
  }, [activeIndex]);

  return (
    <div
      className="composer-suggest-anchor"
      style={{ left: `${point.left}px`, top: `${point.top}px` }}
    >
      <div
        className="composer-suggest-panel surface-pop rounded-xl bg-surface-overlay shadow-lg"
        // The textarea keeps the caret and the keys; the panel is what the
        // caret's owner points at through aria-activedescendant.
        onMouseDown={(event) => event.preventDefault()}
      >
        <p className="px-2 pb-1.5 pt-1 text-xxs uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
          {heading}
        </p>

        <div className="composer-suggest-rows" role="listbox" id={listId} aria-label={heading}>
          {items.length === 0 ? (
            <p className="px-2 py-2 text-ui text-[hsl(var(--text-muted))]">{emptyLabel}</p>
          ) : (
            items.map((item, index) => {
              const Icon = item.icon;
              const isActive = index === activeIndex;
              return (
                <button
                  key={item.id}
                  ref={isActive ? activeRowRef : undefined}
                  type="button"
                  role="option"
                  id={`${listId}-${index}`}
                  aria-selected={isActive}
                  data-active={isActive}
                  onClick={() => onSelect(index)}
                  onMouseMove={() => onHover(index)}
                  className="composer-suggest-row flex w-full items-start gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors duration-fast"
                >
                  {Icon && (
                    <Icon
                      className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[hsl(var(--text-tertiary))]"
                      strokeWidth={1.7}
                      aria-hidden="true"
                    />
                  )}
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-ui text-[hsl(var(--text-primary))]">
                      {item.label}
                    </span>
                    <span className="block truncate text-xs leading-snug text-[hsl(var(--text-muted))]">
                      {item.description}
                    </span>
                  </span>
                  {item.state ? (
                    <span className="mt-0.5 shrink-0 text-xs text-[hsl(var(--text-tertiary))]">
                      {item.state}
                    </span>
                  ) : null}
                </button>
              );
            })
          )}
        </div>

        <div className="composer-suggest-hints text-xs text-[hsl(var(--text-muted))]">
          <span className="inline-flex items-center gap-1">
            <kbd className="kbd">↑</kbd>
            <kbd className="kbd">↓</kbd>
            move
          </span>
          <span className="inline-flex items-center gap-1">
            <kbd className="kbd">↵</kbd>
            accept
          </span>
          <span className="inline-flex items-center gap-1">
            <kbd className="kbd">esc</kbd>
            close
          </span>
        </div>
      </div>
    </div>
  );
}
