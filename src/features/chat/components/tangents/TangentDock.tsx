import { createContext, useContext, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';

import { Maximize2, Minimize2 } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { SplitHandle } from '@/components/ui/SplitHandle';

import './tangents.css';

const WIDTH_KEY = 'tangents.width';
const TANGENT_DEFAULT = 420;
const TANGENT_MIN = 340;
const TANGENT_MAX = 800;
/** The row is this wide before the panel sits beside the conversation instead of over it. */
const DOCKED_ROW = 960;
/** What the conversation keeps beside a docked panel. */
const CONVERSATION_MIN = 360;

function readWidth(): number {
  try {
    const value = Number(localStorage.getItem(WIDTH_KEY));
    return Number.isFinite(value) && value > 0 ? Math.min(TANGENT_MAX, Math.max(TANGENT_MIN, value)) : TANGENT_DEFAULT;
  } catch {
    return TANGENT_DEFAULT;
  }
}

function writeWidth(value: number): void {
  try {
    localStorage.setItem(WIDTH_KEY, String(Math.round(value)));
  } catch {
    // Preference only.
  }
}

/** The widest the panel may be in a row `row` wide; its own limit while the row is unknown. */
function widestIn(row: number): number {
  if (row <= 0) return TANGENT_MAX;
  const room = row >= DOCKED_ROW ? row - CONVERSATION_MIN : row;
  return Math.max(TANGENT_MIN, Math.min(TANGENT_MAX, room));
}

const TangentDockContext = createContext<{ expanded: boolean; toggleExpanded: () => void } | null>(null);

/**
 * The panel a conversation's tangents, or a course's, open in. Its left edge
 * drags to the width the reader likes, which is remembered; its header can
 * expand it over what it sits beside. It sits in the row that holds that too.
 */
export function TangentDock({ id, label, onClose, children }: {
  id: string;
  label: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const dockRef = useRef<HTMLElement>(null);
  const [row, setRow] = useState(0);
  const [preferred, setPreferred] = useState(readWidth);
  const [expanded, setExpanded] = useState(false);
  const widest = widestIn(row);
  const width = Math.min(preferred, widest);
  const dock = useMemo(() => ({ expanded, toggleExpanded: () => setExpanded((value) => !value) }), [expanded]);

  useEffect(() => {
    const element = dockRef.current?.parentElement;
    if (!element || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => setRow(element.clientWidth));
    observer.observe(element);
    setRow(element.clientWidth);
    return () => observer.disconnect();
  }, []);

  const resize = (next: number) => {
    setPreferred(next);
    writeWidth(next);
  };

  return (
    <TangentDockContext.Provider value={dock}>
      <aside ref={dockRef} id={id} aria-label={label} className="tangent-sidebar" data-expanded={expanded || undefined}
        style={{ '--tangent-width': `${width}px` } as CSSProperties}
        onKeyDown={(event) => {
          if (event.key === 'Escape' && !event.defaultPrevented) { event.stopPropagation(); onClose(); }
        }}>
        {!expanded && (
          <div className="tangent-split">
            <SplitHandle label="Resize tangents" width={width} min={TANGENT_MIN} max={widest} side={-1}
              onResize={resize} onReset={() => resize(TANGENT_DEFAULT)} />
          </div>
        )}
        {children}
      </aside>
    </TangentDockContext.Provider>
  );
}

/** For a header inside a dock: expands the panel over what it sits beside, and shrinks it back. */
export function TangentExpandButton() {
  const dock = useContext(TangentDockContext);
  if (!dock) return null;
  return (
    <IconButton label={dock.expanded ? 'Shrink tangents' : 'Expand tangents'} onClick={dock.toggleExpanded}>
      {dock.expanded ? <Minimize2 /> : <Maximize2 />}
    </IconButton>
  );
}
