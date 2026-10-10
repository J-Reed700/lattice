import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';

const KEY_STEP = 24;

export interface SplitHandleProps {
  label: string;
  width: number;
  min: number;
  max: number;
  /** `1` when the pane is left of the handle, `-1` when it is right of it. */
  side: 1 | -1;
  onResize: (_width: number) => void;
  onReset: () => void;
}

/** A vertical drag handle that sizes the pane on one side of it. */
export function SplitHandle({ label, width, min, max, side, onResize, onReset }: SplitHandleProps) {
  const dragRef = useRef<{ pointerId: number; startX: number; startWidth: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const clamp = (value: number) => Math.round(Math.min(max, Math.max(min, value)));

  // Text selection follows a fast drag across the panes otherwise.
  useEffect(() => {
    if (!dragging) return;
    const previous = document.body.style.userSelect;
    document.body.style.userSelect = 'none';
    return () => {
      document.body.style.userSelect = previous;
    };
  }, [dragging]);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = { pointerId: event.pointerId, startX: event.clientX, startWidth: width };
    setDragging(true);
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (drag?.pointerId !== event.pointerId) return;
    onResize(clamp(drag.startWidth + side * (event.clientX - drag.startX)));
  };
  const endDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    dragRef.current = null;
    setDragging(false);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    onResize(clamp(width + side * (event.key === 'ArrowRight' ? KEY_STEP : -KEY_STEP)));
  };

  return (
    <div className="explorer-split">
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label={label}
        aria-valuenow={Math.round(width)}
        aria-valuemin={min}
        aria-valuemax={Math.round(max)}
        tabIndex={0}
        data-dragging={dragging || undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={onReset}
        onKeyDown={onKeyDown}
      />
    </div>
  );
}
