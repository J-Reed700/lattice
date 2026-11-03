import { act, createEvent, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';

import { TangentDock, TangentExpandButton } from './TangentDock';

const WIDTH_KEY = 'tangents.width';

let row = 0;
let rowResized: (() => void) | undefined;
const observe = vi.fn();
const disconnect = vi.fn();

function renderDock(onClose = vi.fn()) {
  const view = render(
    <TooltipProvider>
      <div className="conversation-with-tangents">
        <TangentDock id="tangents-p1" label="Tangents" onClose={onClose}>
          <TangentExpandButton />
          <p>Inside the dock</p>
        </TangentDock>
      </div>
    </TooltipProvider>,
  );
  return { ...view, onClose, dock: screen.getByRole('complementary', { name: 'Tangents' }) };
}

const handle = () => screen.getByRole('separator', { name: 'Resize tangents' });

beforeEach(() => {
  localStorage.clear();
  row = 0;
  rowResized = undefined;
  observe.mockClear();
  disconnect.mockClear();
  Element.prototype.setPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn(() => true);
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(() => row);
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: () => void) { rowResized = callback; }
    observe = observe;
    disconnect = disconnect;
    unobserve() {}
  });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('TangentDock', () => {
  it('holds what it is given, under the id and label it is given', () => {
    const { dock } = renderDock();
    expect(dock).toHaveAttribute('id', 'tangents-p1');
    expect(screen.getByText('Inside the dock')).toBeInTheDocument();
  });

  describe('width', () => {
    it('opens at its default width, between its limits', () => {
      renderDock();
      expect(handle()).toHaveAttribute('aria-valuenow', '420');
      expect(handle()).toHaveAttribute('aria-valuemin', '340');
      expect(handle()).toHaveAttribute('aria-valuemax', '800');
    });

    it('opens at the width it was left at', () => {
      localStorage.setItem(WIDTH_KEY, '512');
      renderDock();
      expect(handle()).toHaveAttribute('aria-valuenow', '512');
    });

    it.each([
      ['wide', '420'],
      ['0', '420'],
      ['-5', '420'],
      ['100', '340'],
      ['5000', '800'],
    ])('opens at a saved %s as %s', (saved, shown) => {
      localStorage.setItem(WIDTH_KEY, saved);
      renderDock();
      expect(handle()).toHaveAttribute('aria-valuenow', shown);
    });

    it('is resized by dragging its edge, within its limits, and keeps the width', () => {
      renderDock();
      fireEvent.pointerDown(handle(), { button: 0, pointerId: 1, clientX: 1000 });
      fireEvent.pointerMove(handle(), { pointerId: 1, clientX: 900 });
      expect(handle()).toHaveAttribute('aria-valuenow', '520');
      expect(localStorage.getItem(WIDTH_KEY)).toBe('520');
      fireEvent.pointerMove(handle(), { pointerId: 1, clientX: 100 });
      expect(handle()).toHaveAttribute('aria-valuenow', '800');
      fireEvent.pointerMove(handle(), { pointerId: 1, clientX: 2000 });
      expect(handle()).toHaveAttribute('aria-valuenow', '340');
      fireEvent.pointerUp(handle(), { pointerId: 1 });
      expect(localStorage.getItem(WIDTH_KEY)).toBe('340');
    });

    it('steps with the arrow keys and resets on double-click', () => {
      renderDock();
      fireEvent.keyDown(handle(), { key: 'ArrowLeft' });
      expect(handle()).toHaveAttribute('aria-valuenow', '444');
      expect(localStorage.getItem(WIDTH_KEY)).toBe('444');
      fireEvent.keyDown(handle(), { key: 'ArrowRight' });
      fireEvent.keyDown(handle(), { key: 'ArrowRight' });
      expect(handle()).toHaveAttribute('aria-valuenow', '396');
      fireEvent.doubleClick(handle());
      expect(handle()).toHaveAttribute('aria-valuenow', '420');
      expect(localStorage.getItem(WIDTH_KEY)).toBe('420');
    });

    it('leaves room for the conversation beside it, without forgetting the wider width', () => {
      localStorage.setItem(WIDTH_KEY, '800');
      row = 1000;
      renderDock();
      expect(handle()).toHaveAttribute('aria-valuemax', '640');
      expect(handle()).toHaveAttribute('aria-valuenow', '640');
      expect(localStorage.getItem(WIDTH_KEY)).toBe('800');
    });

    it('may cover a narrow row, but never shrinks below its minimum', () => {
      localStorage.setItem(WIDTH_KEY, '800');
      row = 700;
      const view = renderDock();
      expect(handle()).toHaveAttribute('aria-valuenow', '700');
      view.unmount();
      row = 300;
      renderDock();
      expect(handle()).toHaveAttribute('aria-valuenow', '340');
    });

    it('follows the row as it is resized, and stops when it closes', () => {
      row = 1600;
      const { dock, unmount } = renderDock();
      expect(observe).toHaveBeenCalledWith(dock.parentElement);
      expect(handle()).toHaveAttribute('aria-valuemax', '800');
      act(() => {
        row = 1000;
        rowResized?.();
      });
      expect(handle()).toHaveAttribute('aria-valuemax', '640');
      unmount();
      expect(disconnect).toHaveBeenCalled();
    });

    it('does without a row to measure', () => {
      vi.unstubAllGlobals();
      renderDock();
      expect(handle()).toHaveAttribute('aria-valuenow', '420');
      expect(observe).not.toHaveBeenCalled();
    });
  });

  describe('expanding', () => {
    it('takes the whole row and gives its edge up, until it is shrunk again', () => {
      const { dock } = renderDock();
      fireEvent.click(screen.getByRole('button', { name: 'Expand tangents' }));
      expect(dock).toHaveAttribute('data-expanded', 'true');
      expect(screen.queryByRole('separator')).not.toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Shrink tangents' }));
      expect(dock).not.toHaveAttribute('data-expanded');
      expect(handle()).toHaveAttribute('aria-valuenow', '420');
    });

    it('is offered only to what sits inside a dock', () => {
      render(<TooltipProvider><TangentExpandButton /></TooltipProvider>);
      expect(screen.queryByRole('button')).not.toBeInTheDocument();
    });
  });

  describe('Escape', () => {
    it('closes the dock, and nothing else does', () => {
      const { dock, onClose } = renderDock();
      fireEvent.keyDown(dock, { key: 'a' });
      expect(onClose).not.toHaveBeenCalled();
      fireEvent.keyDown(dock, { key: 'Escape' });
      expect(onClose).toHaveBeenCalledTimes(1);
    });

    it('leaves an Escape that something inside already used', () => {
      const { dock, onClose } = renderDock();
      const escape = createEvent.keyDown(dock, { key: 'Escape' });
      escape.preventDefault();
      fireEvent(dock, escape);
      expect(onClose).not.toHaveBeenCalled();
    });
  });
});
