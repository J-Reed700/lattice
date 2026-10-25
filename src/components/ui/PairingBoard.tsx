import { useId, useLayoutEffect, useRef, useState } from 'react';

import { GripVertical, X } from 'lucide-react';

import { Button } from './button';
import './interactions.css';

type Connector = { key: number; path: string; x: number; y: number; endX: number; endY: number };

/** Select or drag pairs. Connections convey assignments, never correctness. */
export function PairingBoard({ leftItems, rightItems, matches, onChange, disabled = false, labelledBy }: {
  leftItems: string[];
  rightItems: string[];
  matches: (number | null)[];
  onChange: (matches: (number | null)[]) => void;
  disabled?: boolean;
  labelledBy: string;
}) {
  const [active, setActive] = useState(0);
  const [dragged, setDragged] = useState<number | null>(null);
  const [hovered, setHovered] = useState<number | null>(null);
  const [announcement, setAnnouncement] = useState('');
  const [connectors, setConnectors] = useState<Connector[]>([]);
  const root = useRef<HTMLDivElement>(null);
  const left = useRef<(HTMLButtonElement | null)[]>([]);
  const right = useRef<(HTMLButtonElement | null)[]>([]);
  const hint = useId();
  const dragId = useId();

  useLayoutEffect(() => {
    const measure = () => {
      const bounds = root.current?.getBoundingClientRect();
      if (!bounds?.width) return;
      const next = matches.flatMap((target, index): Connector[] => {
        if (target === null || target === undefined) return [];
        const start = left.current[index]?.parentElement?.getBoundingClientRect();
        const end = right.current[target]?.getBoundingClientRect();
        if (!start || !end) return [];
        const x = start.right - bounds.left + 3, y = start.top + start.height / 2 - bounds.top;
        const endX = end.left - bounds.left - 3, endY = end.top + end.height / 2 - bounds.top;
        const mid = (x + endX) / 2;
        return [{ key: index, x, y, endX, endY, path: `M ${x} ${y} C ${mid} ${y}, ${mid} ${endY}, ${endX} ${endY}` }];
      });
      setConnectors((previous) => JSON.stringify(previous) === JSON.stringify(next) ? previous : next);
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    if (root.current) observer.observe(root.current);
    [...left.current, ...right.current].forEach((element) => { if (element) observer.observe(element); });
    return () => observer.disconnect();
  }, [leftItems, rightItems, matches]);

  const pair = (index: number, target: number | null) => {
    if (disabled) return;
    const next = leftItems.map((_, i) => matches[i] ?? null);
    next[index] = target;
    onChange(next);
    setAnnouncement(target === null ? `${leftItems[index]} is now unmatched.` : `${leftItems[index]} matched to ${rightItems[target]}.`);
    const nextOpen = leftItems.findIndex((_, i) => i > index && next[i] === null);
    const firstOpen = next.findIndex((value) => value === null);
    setActive(target === null ? index : nextOpen >= 0 ? nextOpen : firstOpen >= 0 ? firstOpen : index);
    setDragged(null);
    setHovered(null);
  };

  return <div role="group" aria-labelledby={labelledBy} aria-describedby={hint}>
    <div className="ui-pairing-heading"><p id={hint} className="ui-interaction-hint">Choose an item on the left, then its match on the right. Or drag a right-hand card onto its match.</p><span>{leftItems.filter((_, index) => matches[index] != null).length} / {leftItems.length} paired</span></div>
    <div ref={root} className="ui-pairing-board">
      <svg aria-hidden="true" className="ui-pairing-lines">{connectors.map((line) => <g key={line.key} data-active={active === line.key}><path d={line.path} /><circle cx={line.x} cy={line.y} r="2" /><circle cx={line.endX} cy={line.endY} r="2" /></g>)}</svg>
      <div className="ui-pairing-column">{leftItems.map((value, index) => <div key={`${index}-${value}`} className="ui-pairing-left" data-active={active === index} data-paired={matches[index] != null} data-drop-target={hovered === index} onDragOver={(event) => { if (disabled || dragged === null) return; event.preventDefault(); event.dataTransfer.dropEffect = 'link'; setHovered(index); }} onDragLeave={() => setHovered(null)} onDrop={(event) => { event.preventDefault(); if (dragged !== null && event.dataTransfer.getData('text/plain') === dragId) pair(index, dragged); }}>
        <button ref={(element) => { left.current[index] = element; }} type="button" disabled={disabled} aria-label={value} aria-pressed={active === index} aria-description={matches[index] != null ? `Matched to ${rightItems[matches[index]!]}` : 'Unmatched'} onClick={(event) => { setActive(index); if (event.detail === 0) right.current[0]?.focus(); }} className="ui-pairing-card"><span className="ui-pairing-marker">{index + 1}</span><span>{value}</span></button>
        {matches[index] != null && <Button type="button" variant="ghost" size="icon" aria-label={`Clear match for ${value}`} disabled={disabled} onClick={() => pair(index, null)} className="ui-pairing-clear"><X size={13} /></Button>}
      </div>)}</div>
      <div className="ui-pairing-gutter" aria-hidden="true" />
      <div className="ui-pairing-column">{rightItems.map((value, index) => <button ref={(element) => { right.current[index] = element; }} key={`${index}-${value}`} type="button" disabled={disabled} draggable={!disabled} aria-label={value} aria-description={`Match with ${leftItems[active] ?? 'the selected item'}`} className="ui-pairing-card ui-pairing-right" data-paired={matches.includes(index)} data-dragging={dragged === index} onClick={() => { if (leftItems[active]) pair(active, index); }} onDragStart={(event) => { if (disabled) { event.preventDefault(); return; } event.dataTransfer.setData('text/plain', dragId); event.dataTransfer.effectAllowed = 'link'; setDragged(index); }} onDragEnd={() => { setDragged(null); setHovered(null); }}><span className="ui-pairing-marker">{String.fromCharCode(65 + index)}</span><span>{value}</span><GripVertical size={13} aria-hidden="true" className="ui-pairing-grip" /></button>)}</div>
    </div>
    <p role="status" aria-live="polite" className="sr-only">{announcement}</p>
  </div>;
}
