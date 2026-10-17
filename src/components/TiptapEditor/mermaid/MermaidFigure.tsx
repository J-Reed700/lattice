import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';

import { MediaLightbox } from '@/components/MediaLightbox';

import { peekMermaid, renderMermaid, type RenderResult } from './render';
import { useThemeEpoch } from './themeEpoch';

interface MermaidFigureProps {
  source: string;
  /** The fence's code, highlighted as any code block. Shown as `Source`, and on an error. */
  children: ReactNode;
}

interface Drawn {
  source: string;
  epoch: number;
  result: RenderResult;
}

/**
 * The inside of a ```mermaid figure: the drawing once it is ready, the code
 * until then, and the code with Mermaid's complaint when it cannot be drawn.
 * Never an empty box.
 */
export function MermaidFigure({ source, children }: MermaidFigureProps) {
  const epoch = useThemeEpoch();
  const [drawn, setDrawn] = useState<Drawn | null>(() => {
    const ready = peekMermaid(source, epoch);
    return ready ? { source, epoch, result: ready } : null;
  });
  const [showSource, setShowSource] = useState(false);
  const [enlarged, setEnlarged] = useState(false);

  const current = drawn?.source === source && drawn.epoch === epoch;
  useEffect(() => {
    if (current) return;
    let live = true;
    renderMermaid(source, epoch).then(
      (result) => { if (live) setDrawn({ source, epoch, result }); },
      (error: unknown) => {
        if (!live) return;
        const message = error instanceof Error ? error.message : String(error);
        setDrawn({ source, epoch, result: { kind: 'error', message } });
      },
    );
    return () => { live = false; };
  }, [current, source, epoch]);

  // After a theme switch the old drawing stays up until the new one is ready.
  const result = drawn?.source === source ? drawn.result : null;
  const svg = result?.kind === 'svg' ? result.svg : null;
  const naturalWidth = result?.kind === 'svg' ? result.width : undefined;
  const codeHidden = svg !== null && !showSource;

  // Which edges of the frame have drawing beyond them. Flags on the element,
  // not state: they change on every scroll and nothing else depends on them.
  const scrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const frame = scrollRef.current;
    if (!frame) return;
    const measure = () => {
      const hidden = frame.scrollWidth - frame.clientWidth;
      frame.toggleAttribute('data-fade-start', hidden > 1 && frame.scrollLeft > 1);
      frame.toggleAttribute('data-fade-end', hidden > 1 && frame.scrollLeft < hidden - 1);
    };
    measure();
    frame.addEventListener('scroll', measure, { passive: true });
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
    observer?.observe(frame);
    return () => {
      frame.removeEventListener('scroll', measure);
      observer?.disconnect();
    };
  }, [svg, codeHidden]);

  return (
    <>
      {svg !== null && (
        <div className="diagram-header">
          <span className="diagram-label">Diagram</span>
          <button type="button" aria-pressed={showSource} onClick={() => setShowSource((value) => !value)}>
            Source
          </button>
          <button type="button" onClick={() => setEnlarged(true)}>Enlarge</button>
        </div>
      )}
      {codeHidden && (
        // Scrolls sideways when the drawing, at its smallest legible scale,
        // is still wider than the column.
        <div className="diagram-scroll" ref={scrollRef}>
          <button
            type="button"
            className="diagram-svg"
            aria-label="Enlarge diagram"
            style={naturalWidth ? ({ '--diagram-width': `${naturalWidth}px` } as CSSProperties) : undefined}
            onClick={() => setEnlarged(true)}
            // Sanitised in render.ts: SVG profile, no scripts, no foreignObject.
            dangerouslySetInnerHTML={{ __html: svg }}
          />
        </div>
      )}
      <pre hidden={codeHidden}>{children}</pre>
      {result?.kind === 'error' && (
        <p className="diagram-error">Diagram could not be drawn: {result.message}</p>
      )}
      {svg !== null && (
        <MediaLightbox open={enlarged} onOpenChange={setEnlarged} title="Diagram">
          <div className="media-lightbox-diagram" dangerouslySetInnerHTML={{ __html: svg }} />
        </MediaLightbox>
      )}
    </>
  );
}
