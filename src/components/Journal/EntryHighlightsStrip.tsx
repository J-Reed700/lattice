import { useEffect, useRef, useState } from 'react';

import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { ChevronDown, ChevronRight, Highlighter, Pin, PinOff } from 'lucide-react';

import type { NoteHighlight } from '@/types/api/dailyNotes';

interface EntryHighlightsStripProps {
  highlights: NoteHighlight[];
  pinnedIds: Set<string>;
  onAddHighlight: (text: string) => void;
  onRemoveHighlight: (highlightId: string) => void;
  onTogglePinned: (highlightId: string) => void;
  editorContainerRef: React.RefObject<HTMLElement | null>;
  /**
   * Render the floating "Highlight" button over a selection. Off when the
   * editor's own selection toolbar carries the verb, so one selection never
   * grows two toolbars.
   */
  showFloatingToolbar?: boolean;
}

export const HIGHLIGHT_CHAR_LIMIT = 8000;

function formatWhen(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

/**
 * Trailing collapsible highlights region plus the floating selection
 * mini-toolbar. The mini-toolbar is co-located so the highlight flow stays
 * local to this component.
 * Spec §5.4.
 */
export function EntryHighlightsStrip({
  highlights,
  pinnedIds,
  onAddHighlight,
  onRemoveHighlight,
  onTogglePinned,
  editorContainerRef,
  showFloatingToolbar = true,
}: EntryHighlightsStripProps) {
  const prefersReducedMotion = useReducedMotion();
  const [isExpanded, setIsExpanded] = useState(false);
  const [toolbarState, setToolbarState] = useState<
    | { top: number; left: number; text: string }
    | null
  >(null);
  const toolbarRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!showFloatingToolbar) return;
    const container = editorContainerRef.current;
    if (!container) return;

    const updateToolbar = () => {
      const selection = window.getSelection();
      if (!selection || selection.isCollapsed || selection.rangeCount === 0) {
        setToolbarState(null);
        return;
      }
      const range = selection.getRangeAt(0);
      const selectedText = selection.toString().trim();
      if (!selectedText) {
        setToolbarState(null);
        return;
      }
      // Require selection to be within the editor container
      if (!container.contains(range.commonAncestorContainer)) {
        setToolbarState(null);
        return;
      }
      const rect = range.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) {
        setToolbarState(null);
        return;
      }
      setToolbarState({
        top: window.scrollY + rect.top - 36,
        left: window.scrollX + rect.left + rect.width / 2,
        text: selectedText,
      });
    };

    const onSelectionChange = () => {
      requestAnimationFrame(updateToolbar);
    };
    const onMouseDown = (event: MouseEvent) => {
      if (toolbarRef.current?.contains(event.target as Node)) {
        return;
      }
      // Let the default selection handling proceed; selectionchange fires after.
    };

    document.addEventListener('selectionchange', onSelectionChange);
    document.addEventListener('mousedown', onMouseDown, true);
    return () => {
      document.removeEventListener('selectionchange', onSelectionChange);
      document.removeEventListener('mousedown', onMouseDown, true);
    };
  }, [editorContainerRef, showFloatingToolbar]);

  const handleAdd = () => {
    if (!toolbarState) return;
    onAddHighlight(toolbarState.text.slice(0, HIGHLIGHT_CHAR_LIMIT));
    setToolbarState(null);
    window.getSelection()?.removeAllRanges();
  };

  // Order: pinned first, then newest
  const ordered = [...highlights].sort((a, b) => {
    const aPinned = pinnedIds.has(a.id);
    const bPinned = pinnedIds.has(b.id);
    if (aPinned !== bPinned) return aPinned ? -1 : 1;
    return new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime();
  });

  if (highlights.length === 0) {
    // Still render the mini-toolbar portal
    return (
      <FloatingToolbar
        state={toolbarState}
        onAdd={handleAdd}
        toolbarRef={toolbarRef}
        prefersReducedMotion={prefersReducedMotion}
      />
    );
  }

  return (
    <>
      <section className="mt-12">
        <button
          type="button"
          onClick={() => setIsExpanded((v) => !v)}
          className="group flex w-full items-center justify-between text-sm text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
          aria-expanded={isExpanded}
        >
          <span>
            {highlights.length} highlight{highlights.length === 1 ? '' : 's'} from this entry
          </span>
          {isExpanded ? (
            <ChevronDown className="h-4 w-4" strokeWidth={1.75} />
          ) : (
            <ChevronRight className="h-4 w-4" strokeWidth={1.75} />
          )}
        </button>
        <AnimatePresence initial={false}>
          {isExpanded && (
            <motion.div
              key="highlights-body"
              initial={prefersReducedMotion ? undefined : { height: 0, opacity: 0 }}
              animate={prefersReducedMotion ? undefined : { height: 'auto', opacity: 1 }}
              exit={prefersReducedMotion ? undefined : { height: 0, opacity: 0 }}
              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
              className="overflow-hidden"
            >
              <ul className="mt-4 space-y-5">
                {ordered.map((highlight) => {
                  const isPinned = pinnedIds.has(highlight.id);
                  return (
                    <li key={highlight.id}>
                      <blockquote className="border-l-[3px] border-[hsl(var(--border-strong))] pl-4 italic font-serif text-[hsl(var(--text-secondary))] text-base leading-relaxed">
                        {highlight.text}
                      </blockquote>
                      <div className="mt-2 flex items-center gap-3 pl-4 text-xs text-[hsl(var(--text-muted))]">
                        <span>{formatWhen(highlight.createdAt)}</span>
                        <button
                          type="button"
                          onClick={() => onTogglePinned(highlight.id)}
                          className="inline-flex items-center gap-1 text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
                        >
                          {isPinned ? (
                            <>
                              <PinOff className="h-3 w-3" strokeWidth={1.75} />
                              Unpin
                            </>
                          ) : (
                            <>
                              <Pin className="h-3 w-3" strokeWidth={1.75} />
                              Pin
                            </>
                          )}
                        </button>
                        <button
                          type="button"
                          onClick={() => onRemoveHighlight(highlight.id)}
                          className="text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--danger-fg))] transition-colors duration-fast"
                        >
                          Remove
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            </motion.div>
          )}
        </AnimatePresence>
      </section>
      <FloatingToolbar
        state={toolbarState}
        onAdd={handleAdd}
        toolbarRef={toolbarRef}
        prefersReducedMotion={prefersReducedMotion}
      />
    </>
  );
}

interface FloatingToolbarProps {
  state: { top: number; left: number; text: string } | null;
  onAdd: () => void;
  toolbarRef: React.MutableRefObject<HTMLDivElement | null>;
  prefersReducedMotion: boolean | null;
}

function FloatingToolbar({ state, onAdd, toolbarRef, prefersReducedMotion }: FloatingToolbarProps) {
  return (
    <AnimatePresence>
      {state && (
        <motion.div
          ref={(node) => {
            toolbarRef.current = node;
          }}
          initial={prefersReducedMotion ? undefined : { opacity: 0, y: 4 }}
          animate={prefersReducedMotion ? undefined : { opacity: 1, y: 0 }}
          exit={prefersReducedMotion ? undefined : { opacity: 0, y: 4 }}
          transition={{ duration: 0.12, ease: [0.22, 1, 0.36, 1] }}
          style={{
            position: 'absolute',
            top: state.top,
            left: state.left,
            transform: 'translateX(-50%)',
            zIndex: 60,
          }}
          className="pointer-events-auto rounded-sm border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2 py-1 shadow-md"
        >
          <button
            type="button"
            onMouseDown={(e) => {
              // Prevent the selection from collapsing before our handler runs.
              e.preventDefault();
            }}
            onClick={onAdd}
            className="inline-flex items-center gap-1 text-xs text-[hsl(var(--text-primary))] hover:text-[hsl(var(--accent))] transition-colors duration-fast"
          >
            <Highlighter className="h-3 w-3" strokeWidth={1.75} />
            Highlight
          </button>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
