import { createRef } from 'react';

import { render, screen, waitFor } from '@testing-library/react';
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
});
