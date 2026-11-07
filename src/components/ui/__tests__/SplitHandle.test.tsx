import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SplitHandle } from '../SplitHandle';

function renderHandle(side: 1 | -1 = 1, width = 300) {
  const onResize = vi.fn();
  const onReset = vi.fn();
  render(<SplitHandle label="Resize files pane" width={width} min={200} max={500.4} side={side} onResize={onResize} onReset={onReset} />);
  return { onResize, onReset, handle: screen.getByRole('separator', { name: 'Resize files pane' }) };
}

beforeEach(() => {
  Element.prototype.setPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn(() => true);
});

describe('SplitHandle', () => {
  it('describes its value range', () => {
    const { handle } = renderHandle(1, 299.6);
    expect(handle).toHaveAttribute('aria-valuenow', '300');
    expect(handle).toHaveAttribute('aria-valuemin', '200');
    expect(handle).toHaveAttribute('aria-valuemax', '500');
  });

  it('resizes by dragging with the primary button, clamped, and stops on release', () => {
    const { handle, onResize } = renderHandle();
    fireEvent.pointerDown(handle, { button: 2, pointerId: 1, clientX: 100 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 140 });
    expect(onResize).not.toHaveBeenCalled();

    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 100 });
    expect(handle).toHaveAttribute('data-dragging', 'true');
    expect(document.body.style.userSelect).toBe('none');
    fireEvent.pointerMove(handle, { pointerId: 2, clientX: 900 });
    expect(onResize).not.toHaveBeenCalled();
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 140.4 });
    expect(onResize).toHaveBeenLastCalledWith(340);
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 900 });
    expect(onResize).toHaveBeenLastCalledWith(500);
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: -900 });
    expect(onResize).toHaveBeenLastCalledWith(200);

    fireEvent.pointerUp(handle, { pointerId: 2 });
    expect(handle).toHaveAttribute('data-dragging', 'true');
    fireEvent.pointerUp(handle, { pointerId: 1 });
    expect(Element.prototype.releasePointerCapture).toHaveBeenCalledWith(1);
    expect(handle).not.toHaveAttribute('data-dragging');
    expect(document.body.style.userSelect).toBe('');
    onResize.mockClear();
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 160 });
    expect(onResize).not.toHaveBeenCalled();
  });

  it('grows the other way for a pane right of the handle, and ends on cancel', () => {
    const { handle, onResize } = renderHandle(-1);
    vi.mocked(Element.prototype.hasPointerCapture).mockReturnValue(false);
    fireEvent.pointerDown(handle, { button: 0, pointerId: 3, clientX: 100 });
    fireEvent.pointerMove(handle, { pointerId: 3, clientX: 60 });
    expect(onResize).toHaveBeenLastCalledWith(340);
    fireEvent.pointerCancel(handle, { pointerId: 3 });
    expect(Element.prototype.releasePointerCapture).not.toHaveBeenCalled();
    expect(handle).not.toHaveAttribute('data-dragging');
  });

  it('steps with the arrow keys and resets on double-click', () => {
    const { handle, onResize, onReset } = renderHandle(-1);
    fireEvent.keyDown(handle, { key: 'ArrowRight' });
    expect(onResize).toHaveBeenLastCalledWith(276);
    fireEvent.keyDown(handle, { key: 'ArrowLeft' });
    expect(onResize).toHaveBeenLastCalledWith(324);
    fireEvent.keyDown(handle, { key: 'ArrowUp' });
    expect(onResize).toHaveBeenCalledTimes(2);
    fireEvent.doubleClick(handle);
    expect(onReset).toHaveBeenCalled();
  });
});
