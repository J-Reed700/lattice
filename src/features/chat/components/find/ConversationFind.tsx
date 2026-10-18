import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from 'react';

import { ChevronDown, ChevronUp, Search, X } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { isMacOS, matchesShortcut } from '@/config/shortcuts';
import type { ConversationMessage, OptimisticMessage } from '@/types/conversation';
import { findMessageElement, scrollToMessage } from '@/utils/chatMessageNavigation';
import { setHighlightRanges } from '@/utils/customHighlights';
import { formatChatTimestamp } from '@/utils/dateUtils';
import { flattenRenderedText, renderedRange } from '@/utils/renderedText';

import { findInMessages, findPattern, MAX_FIND_RESULTS, occurrenceAt, occurrenceRows, type RenderedCount } from './findInMessages';

import './conversationFind.css';

const MIN_QUERY = 2;
const DEBOUNCE_MS = 250;
const HIGHLIGHT = 'conversation-find';
const CURRENT_HIGHLIGHT = 'conversation-find-current';
const NO_COUNTS: ReadonlyMap<string, RenderedCount> = new Map();

interface RenderedCounts { query: string; counts: ReadonlyMap<string, RenderedCount> }

/** Every occurrence of `pattern` in a message's rendered body, in reading order. */
function renderedOccurrences(article: HTMLElement, pattern: RegExp): (Range | null)[] {
  const found: (Range | null)[] = [];
  article.querySelectorAll<HTMLElement>('.tiptap-viewer').forEach(viewer => {
    const flat = flattenRenderedText(viewer, true);
    for (const match of flat.text.matchAll(pattern)) found.push(renderedRange(flat, match.index ?? 0, match[0].length));
  });
  return found;
}

/**
 * Centre a range in every box that scrolls around it: sideways in a code block
 * only when it is out of sight there, then down the transcript. A range has no
 * `scrollIntoView`, and its parent element can be a whole code block.
 */
function centreRange(range: Range) {
  const start = range.startContainer;
  for (let box = start instanceof Element ? start : start.parentElement; box; box = box.parentElement) {
    const style = getComputedStyle(box);
    const across = /auto|scroll/.test(style.overflowX) && box.scrollWidth > box.clientWidth;
    const down = /auto|scroll/.test(style.overflowY) && box.scrollHeight > box.clientHeight;
    if (!across && !down) continue;
    const target = range.getBoundingClientRect();
    const frame = box.getBoundingClientRect();
    const hidden = target.left < frame.left || target.right > frame.right;
    box.scrollBy({
      left: across && hidden ? target.left + target.width / 2 - (frame.left + frame.width / 2) : 0,
      top: down ? target.top + target.height / 2 - (frame.top + frame.height / 2) : 0,
    });
  }
}

/** Centre occurrence `ordinal` of a rendered message (its last when fewer render), or the message when none does. */
function revealOccurrence(article: HTMLElement, pattern: RegExp, ordinal: number) {
  const found = renderedOccurrences(article, pattern);
  const range = found[Math.min(ordinal, found.length - 1)];
  if (range) centreRange(range);
  else article.scrollIntoView({ block: 'center' });
}

/** Fold counts taken from the screen into the cache, keeping the old object when nothing changed. */
function mergeCounts(previous: RenderedCounts, query: string, counted: ReadonlyMap<string, RenderedCount>): RenderedCounts {
  const base = previous.query === query ? previous.counts : NO_COUNTS;
  const changed = base !== previous.counts || Array.from(counted).some(([id, seen]) => {
    const known = base.get(id);
    return known?.content !== seen.content || known.count !== seen.count;
  });
  return changed ? { query, counts: new Map([...base, ...counted]) } : previous;
}

/**
 * Find in the open conversation, the way a browser or editor finds: the counter
 * counts every occurrence, Enter and Shift+Enter step through them one by one
 * (several within one message too) and wrap at the ends, and the current one is
 * marked apart from the rest and centred. Beneath the box the matching messages
 * list one row each, like the Explorer's folder search. The thread is loaded
 * whole (only its rendering is virtualised), so the search runs here and reaches
 * the oldest message; an occurrence in a message off screen is reached by
 * scrolling to the message, then to the occurrence once it renders.
 * Cmd/Ctrl+F opens it while focus, or the last click, is inside `scopeRef`.
 */
export function ConversationFind({ messages, scopeRef }: {
  messages: readonly (ConversationMessage | OptimisticMessage)[];
  scopeRef: RefObject<HTMLElement | null>;
}) {
  const [open, setOpen] = useState(false);
  const [input, setInput] = useState('');
  const [query, setQuery] = useState('');
  const [listOpen, setListOpen] = useState(true);
  const [current, setCurrent] = useState<{ query: string; messageId: string; ordinal: number; forward: boolean } | null>(null);
  const [rendered, setRendered] = useState<RenderedCounts>({ query: '', counts: NO_COUNTS });
  const [focusRequest, setFocusRequest] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const toggleRef = useRef<HTMLButtonElement>(null);
  const cancelJump = useRef<(() => void) | null>(null);
  const restoreFocus = useRef(false);

  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(input.trim()), DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [input]);

  const active = open && query.length >= MIN_QUERY;
  const persisted = useMemo(() => messages.filter((message): message is ConversationMessage => 'id' in message), [messages]);
  const results = useMemo(() => (active ? findInMessages(persisted, query) : null), [active, persisted, query]);
  const sources = useMemo(() => new Map(results?.matches.map(match => [match.message.id, match.message.content])), [results]);
  const seen = rendered.query === query ? rendered.counts : NO_COUNTS;
  const { rows, total } = useMemo(() => occurrenceRows(results?.matches ?? [], seen), [results, seen]);
  const currentRow = current?.query === query ? rows.find(row => row.message.id === current.messageId) : undefined;
  const currentId = currentRow?.message.id ?? null;
  const currentOrdinal = currentRow && current ? current.ordinal : -1;
  const index = currentRow ? currentRow.start + Math.min(currentOrdinal, currentRow.count - 1) : -1;

  const show = useCallback(() => {
    setOpen(true);
    setListOpen(true);
    setFocusRequest(request => request + 1);
  }, []);
  const close = () => {
    setInput('');
    setQuery('');
    setOpen(false);
    restoreFocus.current = true;
  };
  const jump = useCallback((to: number, forward = true) => {
    const pattern = findPattern(query);
    const target = pattern && total ? occurrenceAt(rows, ((to % total) + total) % total) : null;
    if (!pattern || !target) return;
    const { row: { message: { id } }, ordinal } = target;
    setCurrent({ query, messageId: id, ordinal, forward });
    setListOpen(false);
    cancelJump.current?.();
    cancelJump.current = null;
    const reveal = (article: HTMLElement) => revealOccurrence(article, pattern, ordinal);
    // Within the message already in view, move to the occurrence without
    // flashing the message again.
    const article = id === currentId ? findMessageElement(id) : null;
    if (article && scopeRef.current?.contains(article)) reveal(article);
    else cancelJump.current = scrollToMessage(id, { reveal });
    inputRef.current?.focus();
  }, [currentId, query, rows, scopeRef, total]);

  // Stepping onto a message whose rendered text shows no match (the query was
  // in markdown, such as a link's URL) carries on in the same direction.
  useEffect(() => {
    if (current?.query !== query || currentRow || !total || !results) return;
    const at = results.matches.findIndex(match => match.message.id === current.messageId);
    if (at < 0) return;
    const byId = new Map(rows.map(row => [row.message.id, row]));
    const next = results.matches.slice(at + 1).map(match => byId.get(match.message.id)).find(row => row !== undefined);
    const start = next ? next.start : total;
    jump(current.forward ? start : start - 1, current.forward);
  }, [current, currentRow, jump, query, results, rows, total]);

  useEffect(() => {
    if (focusRequest === 0) return;
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [focusRequest]);
  useEffect(() => {
    if (open || !restoreFocus.current) return;
    restoreFocus.current = false;
    toggleRef.current?.focus();
  }, [open]);
  useEffect(() => () => cancelJump.current?.(), []);

  // Each chat and tangent has its own box, so the shortcut goes to the one the
  // reader is in: where focus is, or where they last clicked when a click on
  // plain text left focus on the page itself.
  useEffect(() => {
    let pointerInside = false;
    const pointer = (event: PointerEvent) => { pointerInside = Boolean(scopeRef.current?.contains(event.target as Node)); };
    const key = (event: KeyboardEvent) => {
      if (!matchesShortcut(event, 'mod+f')) return;
      const focused = document.activeElement;
      const onPage = !focused || focused === document.body;
      if (!rootRef.current?.contains(focused) && !scopeRef.current?.contains(focused) && !(onPage && pointerInside)) return;
      event.preventDefault();
      show();
    };
    document.addEventListener('pointerdown', pointer, true);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('pointerdown', pointer, true);
      document.removeEventListener('keydown', key);
    };
  }, [scopeRef, show]);

  useEffect(() => {
    if (!active || !listOpen) return;
    const outside = (event: PointerEvent) => { if (!rootRef.current?.contains(event.target as Node)) setListOpen(false); };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [active, listOpen]);

  // Mark the query in the matching messages that are rendered, again whenever
  // the thread re-renders (scrolling mounts other rows; a reply streams in).
  // What each shows is also counted, so the counter agrees with the marks.
  useEffect(() => {
    const scope = scopeRef.current;
    const pattern = findPattern(query);
    if (!active || !scope || !pattern || !sources.size) return;
    const owner = {};
    let frame: number | null = null;
    const mark = () => {
      frame = null;
      const ranges: Range[] = [];
      const currentRanges: Range[] = [];
      const counted = new Map<string, RenderedCount>();
      scope.querySelectorAll<HTMLElement>('article[id^="message-"]').forEach(article => {
        const id = article.id.slice('message-'.length);
        const content = sources.get(id);
        if (content === undefined) return;
        const found = renderedOccurrences(article, pattern);
        counted.set(id, { content, count: found.length });
        const currentAt = id === currentId ? Math.min(currentOrdinal, found.length - 1) : -1;
        found.forEach((range, at) => { if (range) (at === currentAt ? currentRanges : ranges).push(range); });
      });
      setHighlightRanges(HIGHLIGHT, owner, ranges);
      setHighlightRanges(CURRENT_HIGHLIGHT, owner, currentRanges);
      setRendered(previous => mergeCounts(previous, query, counted));
    };
    mark();
    const observer = new MutationObserver(() => { if (frame === null) frame = window.requestAnimationFrame(mark); });
    observer.observe(scope, { childList: true, characterData: true, subtree: true });
    return () => {
      observer.disconnect();
      if (frame !== null) window.cancelAnimationFrame(frame);
      setHighlightRanges(HIGHLIGHT, owner, []);
      setHighlightRanges(CURRENT_HIGHLIGHT, owner, []);
    };
  }, [active, currentId, currentOrdinal, query, scopeRef, sources]);

  if (!open) {
    return <IconButton ref={toggleRef} label="Find in conversation" shortcut={isMacOS() ? '⌘F' : 'Ctrl+F'} onClick={show}><Search /></IconButton>;
  }

  return (
    <div ref={rootRef} className="relative min-w-0" onKeyDown={(event) => {
      if (event.key !== 'Escape') return;
      // Handled here, so the tangent sidebar around it stays open.
      event.preventDefault();
      event.stopPropagation();
      close();
    }}>
      <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
      <input
        ref={inputRef}
        type="search"
        value={input}
        onChange={(event) => { setInput(event.target.value); setListOpen(true); }}
        onClick={() => setListOpen(true)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault();
            jump(event.shiftKey ? (index < 0 ? -1 : index - 1) : index + 1, !event.shiftKey);
          } else if (event.key === 'ArrowDown' && active) {
            event.preventDefault();
            setListOpen(true);
          }
        }}
        placeholder="Find in conversation"
        aria-label="Find in conversation"
        className="h-7 w-56 max-w-full rounded-md bg-[hsl(var(--text-primary)/0.04)] pl-7 pr-[5.5rem] text-[12.5px] text-text-primary placeholder:text-text-muted focus:outline-hidden focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-search-cancel-button]:hidden"
      />
      <div className="absolute right-1 top-1/2 flex -translate-y-1/2 items-center">
        {/* Past the cap the total is a floor, so it reads "412+", never exact. */}
        {active && <span className="px-1 text-[11px] tabular-nums text-text-muted" aria-live="polite"
          title={results?.truncated ? `Counted in the first ${MAX_FIND_RESULTS} matching messages` : undefined}>
          {total ? `${index < 0 ? '–' : index + 1}/${total}${results?.truncated ? '+' : ''}` : '0'}
        </span>}
        <button type="button" aria-label="Previous match" disabled={!total} onClick={() => jump(index < 0 ? -1 : index - 1, false)}
          className="inline-flex h-5 w-5 items-center justify-center rounded-sm text-text-tertiary hover:text-text-primary disabled:opacity-40">
          <ChevronUp className="h-3.5 w-3.5" strokeWidth={2} />
        </button>
        <button type="button" aria-label="Next match" disabled={!total} onClick={() => jump(index + 1)}
          className="inline-flex h-5 w-5 items-center justify-center rounded-sm text-text-tertiary hover:text-text-primary disabled:opacity-40">
          <ChevronDown className="h-3.5 w-3.5" strokeWidth={2} />
        </button>
        <button type="button" aria-label="Close find" onClick={close}
          className="inline-flex h-5 w-5 items-center justify-center rounded-sm text-text-tertiary hover:text-text-primary">
          <X className="h-3 w-3" strokeWidth={2} />
        </button>
      </div>
      {active && listOpen && results && (
        <div className="absolute right-0 top-full z-40 mt-1 max-h-[min(60vh,480px)] w-[22rem] max-w-[calc(100vw-2rem)] overflow-y-auto rounded-lg border border-border-subtle bg-surface py-1 shadow-xl" aria-label="Find results">
          {rows.length === 0 ? (
            <p className="px-3 py-2 text-xs text-text-muted">No messages match.</p>
          ) : (
            <>
              {rows.map(match => (
                <button
                  key={match.message.id}
                  type="button"
                  aria-current={match.message.id === currentId || undefined}
                  onClick={() => jump(match.start)}
                  className={`row-hover flex w-full flex-col gap-0.5 px-3 py-1.5 text-left ${match.message.id === currentId ? 'bg-[hsl(var(--text-primary)/0.06)]' : ''}`}
                >
                  <span className="truncate text-[11.5px] text-text-muted">
                    {match.message.role === 'user' ? 'You' : 'Assistant'} · {formatChatTimestamp(match.message.createdAt)}
                    {match.count > 1 ? ` · ${match.count} matches` : ''}
                  </span>
                  <span className="line-clamp-2 text-[12.5px] leading-snug text-text-primary">
                    {match.snippet.before}<mark className="rounded-sm bg-[hsl(var(--highlight))] text-inherit">{match.snippet.match}</mark>{match.snippet.after}
                  </span>
                </button>
              ))}
              {results.truncated && <p className="px-3 py-2 text-[11px] text-text-muted">Only the first {MAX_FIND_RESULTS} matching messages are shown and counted. Narrow the search.</p>}
            </>
          )}
        </div>
      )}
    </div>
  );
}
