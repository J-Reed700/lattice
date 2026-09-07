import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { Bookmark, MessageSquare, NotebookPen } from 'lucide-react';
import { createPortal } from 'react-dom';

import type { TextSelectionState } from './useTextSelection';

/**
 * The three capture verbs that appear over a text selection (BRIEF rank 6).
 *
 * Rendered through a portal with `position: fixed` and the raw viewport rect,
 * so it stays put whatever scroll container it happens to be over. Every button
 * cancels `mousedown` — without that the selection collapses before the click
 * handler ever sees it.
 *
 * All three verbs are always live. There is no state in which one of them is
 * unavailable, so there is no disabled variant to explain (CLAUDE.md: no
 * control that changes nothing).
 */

export interface SelectionToolbarProps {
  selection: TextSelectionState;
  onReference: (_text: string) => void | Promise<void>;
  onAddToJournal: (_text: string) => void | Promise<void>;
  onAskAbout: (_text: string) => void;
}

const BUTTON_CLASS =
  'inline-flex items-center gap-1 text-xs text-[hsl(var(--text-primary))] transition-colors duration-fast hover:text-[hsl(var(--accent))]';

export function SelectionToolbar({
  selection,
  onReference,
  onAddToJournal,
  onAskAbout,
}: SelectionToolbarProps) {
  const prefersReducedMotion = useReducedMotion();

  if (typeof document === 'undefined') return null;

  const visible = Boolean(selection.text && selection.rect);
  const rect = selection.rect;

  return createPortal(
    <AnimatePresence>
      {visible && rect && (
        <motion.div
          initial={prefersReducedMotion ? undefined : { opacity: 0, y: 4 }}
          animate={prefersReducedMotion ? undefined : { opacity: 1, y: 0 }}
          exit={prefersReducedMotion ? undefined : { opacity: 0, y: 4 }}
          transition={{ duration: 0.12, ease: [0.22, 1, 0.36, 1] }}
          style={{
            position: 'fixed',
            top: rect.top - 40,
            left: rect.left + rect.width / 2,
            transform: 'translateX(-50%)',
            zIndex: 60,
          }}
          className="pointer-events-auto flex items-center gap-3 rounded-sm border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2 py-1 shadow-md"
        >
          <button
            type="button"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => void onReference(selection.text)}
            className={BUTTON_CLASS}
          >
            <Bookmark className="h-3 w-3" strokeWidth={1.75} />
            Reference
          </button>
          <button
            type="button"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => void onAddToJournal(selection.text)}
            className={BUTTON_CLASS}
          >
            <NotebookPen className="h-3 w-3" strokeWidth={1.75} />
            Add to journal
          </button>
          <button
            type="button"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => onAskAbout(selection.text)}
            className={BUTTON_CLASS}
          >
            <MessageSquare className="h-3 w-3" strokeWidth={1.75} />
            Ask about this
          </button>
        </motion.div>
      )}
    </AnimatePresence>,
    document.body
  );
}
