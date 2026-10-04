import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useReducer,
  useRef,
  type ReactNode,
  type RefObject,
} from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';

import { registerMessageMountRequester } from '@/utils/chatMessageNavigation';

export interface VirtualizedMessageListHandle {
  scrollToIndex: (index: number, options?: { align?: 'auto' | 'center' | 'end' | 'start'; behavior?: 'auto' | 'smooth' }) => void;
}

interface VirtualizedMessageListProps<T> {
  items: T[];
  scrollElementRef: RefObject<HTMLElement | null>;
  getKey: (item: T) => string;
  getMessageId: (item: T) => string | null;
  onVisibleIndexChange?: (index: number) => void;
  renderItem: (item: T, index: number) => ReactNode;
}

function VirtualizedMessageListImpl<T>(
  { items, scrollElementRef, getKey, getMessageId, onVisibleIndexChange, renderItem }: VirtualizedMessageListProps<T>,
  ref: React.ForwardedRef<VirtualizedMessageListHandle>
) {
  const itemsRef = useRef(items);
  const getKeyRef = useRef(getKey);
  const getMessageIdRef = useRef(getMessageId);
  itemsRef.current = items;
  getKeyRef.current = getKey;
  getMessageIdRef.current = getMessageId;
  const getItemKey = useCallback((index: number) => getKeyRef.current(itemsRef.current[index]), []);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollElementRef.current,
    estimateSize: () => 320,
    getItemKey,
    overscan: 6,
    // Dynamic message rows can change the track height while ResizeObserver
    // is delivering a batch. Let TanStack defer those measurements to a frame
    // so the browser never reports a resize loop to the global error handler.
    useAnimationFrameWithResizeObserver: true,
  });
  const refreshFrameRef = useRef<number | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const [, rerender] = useReducer((count: number) => count + 1, 0);

  // A hidden/minimized Tauri webview can report a zero-sized scroll rect and
  // not emit another ResizeObserver callback when it is shown again. A stale
  // rect leaves the virtualizer with an empty range, which makes the composer
  // appear at the top of an otherwise blank chat. On every browser lifecycle
  // signal that can restore the webview's layout (and on the next frame, so
  // WebKit has committed its dimensions), read the viewport afresh and measure
  // the rows on screen where they stand.
  //
  // Never `virtualizer.measure()`: it forgets every row's height, and a row
  // whose size did not change is never measured again (its ResizeObserver
  // stays quiet and its ref is not called twice). Every long answer then
  // shrank to the 320px estimate and the rows piled on top of each other —
  // after any window focus, including the one that closes the "Delete this
  // message?" dialog — and stayed that way.
  const refreshMeasurements = useCallback(() => {
    const refresh = () => {
      const element = scrollElementRef.current;
      if (element) virtualizer.scrollRect = { width: element.offsetWidth, height: element.offsetHeight };
      listRef.current
        ?.querySelectorAll<HTMLElement>(':scope > [data-index]')
        .forEach((row) => virtualizer.measureElement(row));
      rerender();
    };
    if (typeof window.requestAnimationFrame !== 'function') {
      refresh();
      return;
    }
    if (refreshFrameRef.current !== null) {
      window.cancelAnimationFrame(refreshFrameRef.current);
    }
    refreshFrameRef.current = window.requestAnimationFrame(() => {
      refreshFrameRef.current = null;
      refresh();
    });
  }, [scrollElementRef, virtualizer]);

  useEffect(() => {
    const refresh = () => refreshMeasurements();
    window.addEventListener('focus', refresh);
    window.addEventListener('resize', refresh);
    window.addEventListener('pageshow', refresh);
    window.addEventListener('lattice:window-focused', refresh);
    document.addEventListener('visibilitychange', refresh);
    refresh();

    return () => {
      if (refreshFrameRef.current !== null) {
        window.cancelAnimationFrame(refreshFrameRef.current);
        refreshFrameRef.current = null;
      }
      window.removeEventListener('focus', refresh);
      window.removeEventListener('resize', refresh);
      window.removeEventListener('pageshow', refresh);
      window.removeEventListener('lattice:window-focused', refresh);
      document.removeEventListener('visibilitychange', refresh);
    };
  }, [refreshMeasurements]);
  const requestMessageMount = useCallback((messageId: string) => {
    const index = itemsRef.current.findIndex((item) => getMessageIdRef.current(item) === messageId);
    if (index < 0) return false;
    virtualizer.scrollToIndex(index, { align: 'center' });
    return true;
  }, [virtualizer]);
  useImperativeHandle(ref, () => ({
    scrollToIndex: (index, options) => virtualizer.scrollToIndex(index, options),
  }), [virtualizer]);

  useEffect(() => registerMessageMountRequester(requestMessageMount), [requestMessageMount]);

  const rows = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();
  // Track the message at the reading edge, not the first overscanned row.
  // A long reply remains current until its bottom passes this line.
  const visibleIndex = virtualizer.getVirtualItemForOffset((virtualizer.scrollOffset ?? 0) + 24)?.index;
  useEffect(() => {
    if (visibleIndex !== undefined) onVisibleIndexChange?.(visibleIndex);
  }, [visibleIndex, onVisibleIndexChange]);

  return (
    <div
      ref={listRef}
      className="chat-message-list"
      data-testid="chat-message-list"
      style={{
        position: 'relative',
        height: totalSize,
        width: '100%',
      }}
    >
      {rows.map((row) => (
        <div
          key={row.key}
          data-index={row.index}
          ref={virtualizer.measureElement}
          className="chat-message-row"
          style={{
            position: 'absolute',
            top: 0,
            left: 0,
            width: '100%',
            transform: `translateY(${row.start}px)`,
          }}
        >
          {renderItem(items[row.index], row.index)}
        </div>
      ))}
    </div>
  );
}

export const VirtualizedMessageList = forwardRef(VirtualizedMessageListImpl) as <T>(
  props: VirtualizedMessageListProps<T> & { ref?: React.ForwardedRef<VirtualizedMessageListHandle> }
) => ReactNode;
