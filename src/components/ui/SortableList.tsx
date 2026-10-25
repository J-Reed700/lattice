import { useId, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent, type ReactNode } from 'react';

import { motion, useReducedMotion } from 'framer-motion';
import { ArrowDown, ArrowUp, GripVertical } from 'lucide-react';
import { createPortal } from 'react-dom';

import { Button } from './button';
import './interactions.css';

type DragState = {
  value: string;
  from: number;
  target: number;
  left: number;
  top: number;
  offset: number;
  height: number;
  width: number;
  palette: CSSProperties;
};

/** Sorting commits on drop, so moving a preview never autosaves partial answers. */
export function SortableList({ values, onReorder, disabled = false, labelledBy, renderValue }: {
  values: string[];
  onReorder: (values: string[]) => void;
  disabled?: boolean;
  labelledBy: string;
  renderValue?: (value: string) => ReactNode;
}) {
  const [announcement, setAnnouncement] = useState('');
  const [preview, setPreview] = useState<DragState | null>(null);
  const drag = useRef<DragState | null>(null);
  const list = useRef<HTMLOListElement>(null);
  const hintId = useId();
  const reducedMotion = useReducedMotion();

  const move = (from: number, to: number, focus: boolean, pointer = false) => {
    if (disabled || to < 0 || to >= values.length || to === from) return;
    const next = [...values];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    onReorder(next);
    setAnnouncement(pointer ? `Order updated: ${next.join(', ')}.` : `${moved} moved to position ${to + 1} of ${values.length}.`);
    if (focus) requestAnimationFrame(() => list.current?.querySelectorAll<HTMLElement>(':scope > li')[to]?.focus({ preventScroll: true }));
  };

  const onRowKeyDown = (event: KeyboardEvent<HTMLLIElement>, index: number) => {
    if (event.key === 'Escape' && drag.current) { cancel(); return; }
    if (drag.current || !event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return;
    event.preventDefault();
    move(index, index + (event.key === 'ArrowUp' ? -1 : 1), true);
  };

  const start = (event: PointerEvent<HTMLButtonElement>, index: number) => {
    if (disabled || event.button !== 0) return;
    const row = event.currentTarget.closest('li');
    if (!row) return;
    event.preventDefault();
    event.currentTarget.focus({ preventScroll: true });
    event.currentTarget.setPointerCapture(event.pointerId);
    const bounds = row.getBoundingClientRect();
    const computed = getComputedStyle(row);
    const state: DragState = {
      value: values[index], from: index, target: index,
      left: bounds.left, top: bounds.top, offset: event.clientY - bounds.top,
      height: bounds.height, width: bounds.width,
      palette: { backgroundColor: computed.backgroundColor, color: computed.color, borderColor: computed.borderColor, fontFamily: computed.fontFamily },
    };
    drag.current = state;
    setPreview(state);
    setAnnouncement(`Moving ${state.value}. Release to place it, or press Escape to cancel.`);
  };

  const cancel = () => {
    drag.current = null;
    setPreview(null);
    setAnnouncement('Move cancelled.');
  };

  const update = (event: PointerEvent<HTMLButtonElement>) => {
    const current = drag.current;
    if (!current) return;
    if (disabled) { cancel(); return; }
    const top = event.clientY - current.offset;
    const center = top + current.height / 2;
    const rows = Array.from(list.current?.querySelectorAll<HTMLElement>(':scope > li') ?? []);
    let target = current.from, distance = Infinity;
    rows.forEach((row, index) => {
      const bounds = row.getBoundingClientRect();
      const nextDistance = Math.abs(center - bounds.top - bounds.height / 2);
      if (nextDistance < distance) { distance = nextDistance; target = index; }
    });
    const state = { ...current, top, target };
    drag.current = state;
    setPreview(state);
    // Keep the surrounding reader usable for lists taller than the viewport.
    let scroller = list.current?.parentElement;
    while (scroller && scroller.scrollHeight <= scroller.clientHeight) scroller = scroller.parentElement;
    if (scroller) {
      const bounds = scroller.getBoundingClientRect();
      if (event.clientY < bounds.top + 40) scroller.scrollTop -= 12;
      else if (event.clientY > bounds.bottom - 40) scroller.scrollTop += 12;
    }
  };

  const finish = (event: PointerEvent<HTMLButtonElement>) => {
    const current = drag.current;
    if (!current) return;
    drag.current = null;
    setPreview(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (current.from === current.target) setAnnouncement(`${current.value} remains at position ${current.from + 1}.`);
    else move(current.from, current.target, true, true);
  };

  return <div>
    <p id={hintId} className="ui-interaction-hint">Drag the handles to arrange these, first at the top. You can also use the arrow buttons or Alt+↑ / Alt+↓ on a row.</p>
    <motion.ol ref={list} aria-labelledby={labelledBy} aria-describedby={hintId} className="ui-sortable-list">
      {values.map((value, index) => <motion.li key={value} layout="position" transition={{ duration: reducedMotion ? 0 : .16 }} tabIndex={0} aria-label={`${index + 1} of ${values.length}: ${value}`} onKeyDown={(event) => onRowKeyDown(event, index)} data-dragging={preview?.from === index} data-drop-target={preview && preview.target !== preview.from && preview.target === index} data-drop-position={preview && preview.target > preview.from ? 'after' : 'before'} className="ui-sortable-row">
        <button type="button" aria-label={`Drag ${value}`} disabled={disabled} onPointerDown={(event) => start(event, index)} onPointerMove={update} onPointerUp={finish} onPointerCancel={cancel} className="ui-sortable-grip"><GripVertical size={16} aria-hidden="true" /></button>
        <span aria-hidden="true" className="ui-sortable-number">{String(index + 1).padStart(2, '0')}</span>
        <span className="ui-sortable-content">{renderValue?.(value) ?? value}</span>
        <Button type="button" variant="ghost" size="icon" aria-label={`Move ${value} up`} disabled={disabled || index === 0 || Boolean(preview)} onClick={() => move(index, index - 1, false)} className="ui-sortable-step"><ArrowUp size={15} /></Button>
        <Button type="button" variant="ghost" size="icon" aria-label={`Move ${value} down`} disabled={disabled || index === values.length - 1 || Boolean(preview)} onClick={() => move(index, index + 1, false)} className="ui-sortable-step"><ArrowDown size={15} /></Button>
      </motion.li>)}
    </motion.ol>
    {preview && createPortal(<div aria-hidden="true" className="ui-sortable-ghost" style={{ ...preview.palette, left: preview.left, top: preview.top, width: preview.width, minHeight: preview.height }}><GripVertical size={16} /><span>{String(preview.target + 1).padStart(2, '0')}</span><span>{renderValue?.(preview.value) ?? preview.value}</span></div>, document.body)}
    <p role="status" aria-live="polite" className="sr-only">{announcement}</p>
  </div>;
}
