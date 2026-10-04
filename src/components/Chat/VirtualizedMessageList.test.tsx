import { createRef } from 'react';

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { VirtualizedMessageList } from './VirtualizedMessageList';

interface Item { id: string; text: string }

describe('VirtualizedMessageList', () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 640 });
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', { configurable: true, get: () => 640 });
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0, y: 0, top: 0, left: 0, right: 800, bottom: 640, width: 800, height: 640, toJSON: () => ({}),
    });
  });
  afterEach(() => vi.restoreAllMocks());

  it.each([100, 1000, 10_000])('keeps the mounted row count bounded for %i messages', async (count) => {
    const items: Item[] = Array.from({ length: count }, (_, index) => ({ id: `message-${index}`, text: `Message ${index}` }));
    const scrollElementRef = createRef<HTMLDivElement>();
    const list = (
      <div ref={scrollElementRef} style={{ height: 640, overflow: 'auto' }}>
        <VirtualizedMessageList
          items={items}
          scrollElementRef={scrollElementRef}
          getKey={(item) => item.id}
          getMessageId={(item) => item.id}
          renderItem={(item) => <div id={item.id}>{item.text}</div>}
        />
      </div>
    );
    const rendered = render(list);
    rendered.rerender(list);

    await waitFor(() => {
      const mountedRows = screen.getByTestId('chat-message-list').querySelectorAll('.chat-message-row');
      expect(mountedRows.length).toBeGreaterThan(0);
      expect(mountedRows.length).toBeLessThanOrEqual(20);
    });
  });

  /**
   * The bug: a window focus (closing the "Delete this message?" dialog is
   * one) called `virtualizer.measure()`, which forgot every row's height.
   * Rows whose size had not changed were never measured again, so long
   * answers collapsed to the 320px estimate and overlapped for good.
   */
  it('keeps measured row heights when the window regains focus', async () => {
    const items: Item[] = Array.from({ length: 5 }, (_, index) => ({ id: `message-${index}`, text: `Message ${index}` }));
    const scrollElementRef = createRef<HTMLDivElement>();
    render(
      <div ref={scrollElementRef} style={{ height: 640, overflow: 'auto' }}>
        <VirtualizedMessageList
          items={items}
          scrollElementRef={scrollElementRef}
          getKey={(item) => item.id}
          getMessageId={(item) => item.id}
          renderItem={(item) => <div id={item.id}>{item.text}</div>}
        />
      </div>
    );
    const offsets = () =>
      [...screen.getByTestId('chat-message-list').querySelectorAll<HTMLElement>('.chat-message-row')].map(
        (row) => row.style.transform
      );
    // Every row measures 640px tall (the mocked rect), not the 320px estimate.
    await waitFor(() => expect(offsets().slice(0, 3)).toEqual(['translateY(0px)', 'translateY(640px)', 'translateY(1280px)']));

    await act(async () => {
      window.dispatchEvent(new Event('focus'));
      await new Promise((resolve) => window.requestAnimationFrame(() => resolve(null)));
    });

    expect(offsets().slice(0, 3)).toEqual(['translateY(0px)', 'translateY(640px)', 'translateY(1280px)']);
    expect(screen.getByTestId('chat-message-list').style.height).toBe(`${5 * 640}px`);
  });

  it('tracks the reading edge inside a long answer instead of reporting overscan rows', async () => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
      configurable: true,
      get() { return (this as HTMLElement).dataset.index === '1' ? 4000 : 640; },
    });
    const items = Array.from({ length: 30 }, (_, index) => ({ id: `message-${index}`, text: `Message ${index}` }));
    const scrollElementRef = createRef<HTMLDivElement>();
    const onVisibleIndexChange = vi.fn();
    render(
      <div ref={scrollElementRef} style={{ height: 640, overflow: 'auto' }}>
        <VirtualizedMessageList
          items={items}
          scrollElementRef={scrollElementRef}
          getKey={(item) => item.id}
          getMessageId={(item) => item.id}
          onVisibleIndexChange={onVisibleIndexChange}
          renderItem={(item) => <div>{item.text}</div>}
        />
      </div>
    );
    await waitFor(() => expect(onVisibleIndexChange).toHaveBeenLastCalledWith(0));
    // The parent scroll ref is attached after the child's first layout effect;
    // the scheduled measurement refresh attaches the scroll observer.
    await act(async () => {
      await new Promise((resolve) => window.requestAnimationFrame(() => resolve(null)));
    });
    const scrollTo = (top: number) => {
      scrollElementRef.current!.scrollTop = top;
      fireEvent.scroll(scrollElementRef.current!);
    };
    scrollTo(1000);
    await waitFor(() => expect(onVisibleIndexChange).toHaveBeenLastCalledWith(1));
    scrollTo(3500);
    expect(onVisibleIndexChange).toHaveBeenLastCalledWith(1);
    scrollTo(4640);
    await waitFor(() => expect(onVisibleIndexChange).toHaveBeenLastCalledWith(2));
  });
});
