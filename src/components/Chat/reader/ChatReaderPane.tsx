import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from 'react';

import {
  READER_DEFAULT_WIDTH,
  READER_MAX_WIDTH,
  READER_MIN_CHAT_WIDTH,
  READER_MIN_WIDTH,
  useChatReaderStore,
} from '@/stores/chatReaderStore';
import { useConversationsStore } from '@/stores/conversationsStore';

import { locatorFromSource, rememberLocation } from '../../Reading/passageLocator';
import { FilePreviewModal } from '../FilePreviewModal';
import { SourceReaderBody } from './SourceReaderBody';

/**
 * The source reader as the third pane of the chat.
 *
 * Mounted once, beside the thread rather than on top of it, because reading a
 * source and reading the answer that cites it is one act: the claim has to
 * stay on screen while its evidence is checked.
 *
 * Below a certain width there is no beside — a 380px reader next to a chat
 * column narrower than 560px gives two unreadable columns instead of one
 * readable one — so under that the reader falls back to the overlay it used to
 * be. The decision comes from the row this pane lives in, not the window: a
 * collapsed sidebar is space the reader can have.
 */

/** How far a keypress moves the drag handle. */
const KEY_STEP = 16;
/** What is left of the chat while the reader is expanded: enough to see it is still there. */
const FOCUSED_CHAT_WIDTH = 320;

export interface ChatReaderPaneProps {
  /** Width of the flex row the pane is a child of. */
  rowWidth: number;
  /** That row minus the conversation sidebar: what the chat and the reader share. */
  availableWidth: number;
}

export function ChatReaderPane({ rowWidth, availableWidth }: ChatReaderPaneProps) {
  const session = useChatReaderStore((state) => state.session);
  const storedWidth = useChatReaderStore((state) => state.width);
  const setStoredWidth = useChatReaderStore((state) => state.setWidth);
  const setIndex = useChatReaderStore((state) => state.setIndex);
  const close = useChatReaderStore((state) => state.close);
  const rememberResolvedLocation = useChatReaderStore((state) => state.rememberResolvedLocation);
  const activeConversationId = useConversationsStore((state) => state.activeConversationId);

  const [isFocused, setIsFocused] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const paneRef = useRef<HTMLElement | null>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const dragRef = useRef<{ pointerId: number; startX: number; startWidth: number } | null>(null);

  const source = session ? session.citations[session.index] ?? null : null;
  const isOpen = Boolean(source);

  // A reader left open across a conversation switch would be showing a source
  // for an answer that is no longer on screen.
  useEffect(() => {
    close();
  }, [activeConversationId, close]);

  // Leaving chat closes it too: the pane is the only thing that draws it, and
  // a session that outlives its pane comes back unannounced.
  useEffect(() => close, [close]);

  useEffect(() => {
    if (!isOpen) setIsFocused(false);
  }, [isOpen]);

  // Remember what had focus when the reader opened, and take focus, so the
  // reader is reachable from the keyboard and Esc has somewhere to send it back.
  useEffect(() => {
    if (isOpen) {
      returnFocusRef.current = document.activeElement as HTMLElement | null;
      paneRef.current?.focus({ preventScroll: true });
      return;
    }
    const target = returnFocusRef.current;
    returnFocusRef.current = null;
    if (target?.isConnected) target.focus({ preventScroll: true });
  }, [isOpen]);

  const maxWidth = useMemo(
    () =>
      Math.max(
        READER_MIN_WIDTH,
        Math.min(READER_MAX_WIDTH, rowWidth * 0.6, availableWidth - READER_MIN_CHAT_WIDTH)
      ),
    [rowWidth, availableWidth]
  );
  const dockedWidth = Math.min(storedWidth, maxWidth);
  const paneWidth = isFocused
    ? Math.max(dockedWidth, availableWidth - FOCUSED_CHAT_WIDTH)
    : dockedWidth;
  const canDock = availableWidth >= READER_MIN_CHAT_WIDTH + READER_MIN_WIDTH;

  const applyWidth = useCallback(
    (px: number) => setStoredWidth(Math.min(maxWidth, Math.max(READER_MIN_WIDTH, px))),
    [maxWidth, setStoredWidth]
  );

  const handleLocationResolved = useCallback(
    (chunkId: string, label: string) => {
      // Remembered globally, so every citation row for this chunk shows the
      // page, and in the store, so the answers on screen re-render with it now.
      rememberLocation(chunkId, label);
      rememberResolvedLocation(chunkId, label);
    },
    [rememberResolvedLocation]
  );

  // Esc closes from anywhere in the chat, except where it is someone's
  // character: a composer or a field owns its own Escape.
  useEffect(() => {
    if (!isOpen || !canDock) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest('input, textarea, select, [contenteditable="true"]')
      ) {
        return;
      }
      event.preventDefault();
      close();
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [isOpen, canDock, close]);

  // A drag that crosses the answer must not select it.
  useEffect(() => {
    if (!isDragging) return;
    const previous = document.body.style.userSelect;
    document.body.style.userSelect = 'none';
    return () => {
      document.body.style.userSelect = previous;
    };
  }, [isDragging]);

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = { pointerId: event.pointerId, startX: event.clientX, startWidth: paneWidth };
    setIsDragging(true);
  };

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (drag?.pointerId !== event.pointerId) return;
    // The reader is on the right edge: dragging towards the answer widens it.
    applyWidth(drag.startWidth - (event.clientX - drag.startX));
  };

  const endDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (drag?.pointerId !== event.pointerId) return;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    dragRef.current = null;
    setIsDragging(false);
  };

  const handleHandleKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'ArrowLeft') {
      event.preventDefault();
      applyWidth(paneWidth + KEY_STEP);
      return;
    }
    if (event.key === 'ArrowRight') {
      event.preventDefault();
      applyWidth(paneWidth - KEY_STEP);
    }
  };

  const locator = useMemo(
    () => (source ? locatorFromSource(source, source.chunkId) : null),
    [source]
  );

  if (!session || !source) return null;

  if (!canDock) {
    return (
      <FilePreviewModal
        presentation="reading-pane"
        isOpen
        onClose={close}
        source={source}
        initialLocator={locator}
        citations={session.citations}
        citationIndex={session.index}
        onCitationIndexChange={setIndex}
        onLocationResolved={handleLocationResolved}
      />
    );
  }

  return (
    <aside
      ref={paneRef}
      tabIndex={-1}
      aria-label="Source reader"
      className={`chat-reader-pane${isDragging ? ' chat-reader-pane--dragging' : ''}`}
      style={{ width: `${paneWidth}px` }}
    >
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize source reader"
        aria-valuenow={Math.round(paneWidth)}
        aria-valuemin={READER_MIN_WIDTH}
        aria-valuemax={Math.round(maxWidth)}
        tabIndex={0}
        className="chat-reader-handle"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={() => applyWidth(READER_DEFAULT_WIDTH)}
        onKeyDown={handleHandleKeyDown}
      />
      <SourceReaderBody
        presentation="docked"
        source={source}
        onClose={close}
        initialLocator={locator}
        citations={session.citations}
        citationIndex={session.index}
        onCitationIndexChange={setIndex}
        onLocationResolved={handleLocationResolved}
        isFocused={isFocused}
        onToggleFocus={() => setIsFocused((value) => !value)}
      />
    </aside>
  );
}
