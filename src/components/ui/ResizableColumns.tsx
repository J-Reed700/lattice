import { useEffect, useId, useRef, useState, type CSSProperties, type ReactNode } from 'react';

import { GripVertical } from 'lucide-react';

import { cn } from '@/lib/utils';

import './interactions.css';

function savedWidth(key: string | undefined, fallback: number) {
  try { const value = key ? localStorage.getItem(key) : null; const width = value === null ? NaN : Number(value); return Number.isFinite(width) ? width : fallback; }
  catch { return fallback; }
}

/** A trailing pane with pointer/keyboard resizing and a stacked narrow layout. */
export function ResizableColumns({ children, sidebar, label, storageKey, defaultWidth = 245, minWidth = 210, maxWidth = 420, minMainWidth = 300, className, id }: {
  children: ReactNode;
  sidebar?: ReactNode;
  label: string;
  storageKey?: string;
  defaultWidth?: number;
  minWidth?: number;
  maxWidth?: number;
  minMainWidth?: number;
  className?: string;
  id?: string;
}) {
  const [width, setWidth] = useState(() => savedWidth(storageKey, defaultWidth));
  const [available, setAvailable] = useState(maxWidth);
  const [dragging, setDragging] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; width: number } | null>(null);
  const paneId = useId();
  const maximum = Math.max(minWidth, Math.min(maxWidth, available));
  const clamp = (next: number) => Math.round(Math.max(minWidth, Math.min(maximum, next)));
  const currentWidth = clamp(width);
  const persist = (next: number) => {
    setWidth(next);
    try { if (storageKey) localStorage.setItem(storageKey, String(next)); } catch { /* A pane remains usable when storage is unavailable. */ }
  };

  useEffect(() => {
    const element = container.current;
    if (!element) return;
    const measure = () => { if (element.clientWidth > 0) setAvailable(element.clientWidth - minMainWidth - 12); };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [minMainWidth]);

  return <div id={id} ref={container} className={cn('ui-resizable-columns', sidebar ? 'has-sidebar' : 'without-sidebar', dragging && 'is-resizing', className)} style={{ '--sidebar-width': `${currentWidth}px` } as CSSProperties}>
    <div className="ui-resizable-grid">
      <div className="ui-resizable-main">{children}</div>
      {sidebar && <>
        <div role="separator" aria-orientation="vertical" aria-label={label} aria-controls={paneId} aria-valuemin={minWidth} aria-valuemax={maximum} aria-valuenow={currentWidth} aria-valuetext={`${currentWidth} pixels`} tabIndex={0} className="ui-resize-handle" title="Drag to resize · Arrow keys to adjust · Double-click to reset" onDoubleClick={() => persist(clamp(defaultWidth))} onKeyDown={(event) => {
          const next = event.key === 'Home' ? minWidth : event.key === 'End' ? maximum : event.key === 'ArrowLeft' ? currentWidth + (event.shiftKey ? 40 : 16) : event.key === 'ArrowRight' ? currentWidth - (event.shiftKey ? 40 : 16) : null;
          if (next === null) return;
          event.preventDefault();
          persist(clamp(next));
        }} onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          event.currentTarget.focus();
          event.currentTarget.setPointerCapture(event.pointerId);
          drag.current = { x: event.clientX, width: currentWidth };
          setDragging(true);
        }} onPointerMove={(event) => { if (drag.current) setWidth(clamp(drag.current.width + drag.current.x - event.clientX)); }} onPointerUp={(event) => {
          if (!drag.current) return;
          persist(clamp(drag.current.width + drag.current.x - event.clientX));
          drag.current = null;
          setDragging(false);
          event.currentTarget.releasePointerCapture(event.pointerId);
        }} onPointerCancel={() => { if (drag.current) setWidth(drag.current.width); drag.current = null; setDragging(false); }}>
          <GripVertical size={12} aria-hidden="true" />
        </div>
        <div id={paneId} className="ui-resizable-sidebar">{sidebar}</div>
      </>}
    </div>
  </div>;
}
