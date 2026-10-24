import { useEffect, useMemo, useRef, useState } from 'react';

import {
  AlertCircle,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Highlighter,
  PenTool,
  RotateCcw,
  Sparkles,
  Undo2,
} from 'lucide-react';

export interface VerbatimOverlapResult {
  percent: number;
  matchedPhrases: string[];
  noteWordCount: number;
  sourceWordCount: number;
}

export interface HandwrittenSynthesisToolsProps {
  noteContent: string;
  sourceTexts: string[];
  lessonKey: string;
  onInsertSynthesis?: (markdown: string) => void;
  /** Optional controlled mode when embedded in NotebookPanel's mode switcher */
  sketchpadOpen?: boolean;
  onSketchpadOpenChange?: (open: boolean) => void;
}

interface Point {
  x: number;
  y: number;
}

interface Stroke {
  points: Point[];
  color: string;
  width: number;
  isHighlighter?: boolean;
}

export function computeVerbatimOverlap(noteContent: string, sourceTexts: string[]): VerbatimOverlapResult {
  const cleanNote = noteContent
    .replace(/[#*`_~[\]()<>]/g, ' ')
    .replace(/[^a-zA-Z0-9\s]/g, ' ')
    .toLowerCase();
  const noteWords = cleanNote.split(/\s+/).filter(Boolean);

  const sourceCorpus = sourceTexts.join(' ');
  const cleanSources = sourceCorpus
    .replace(/[#*`_~[\]()<>]/g, ' ')
    .replace(/[^a-zA-Z0-9\s]/g, ' ')
    .toLowerCase();
  const sourceWords = cleanSources.split(/\s+/).filter(Boolean);

  if (noteWords.length === 0) {
    return {
      percent: 0,
      matchedPhrases: [],
      noteWordCount: 0,
      sourceWordCount: sourceWords.length,
    };
  }

  if (sourceWords.length === 0) {
    return {
      percent: 0,
      matchedPhrases: [],
      noteWordCount: noteWords.length,
      sourceWordCount: 0,
    };
  }

  if (noteWords.length < 5) {
    const notePhrase = noteWords.join(' ');
    const isExactMatch = cleanSources.includes(notePhrase);
    return {
      percent: isExactMatch ? 100 : 0,
      matchedPhrases: isExactMatch ? [notePhrase] : [],
      noteWordCount: noteWords.length,
      sourceWordCount: sourceWords.length,
    };
  }

  const sourceNgrams = new Set<string>();
  for (let i = 0; i <= sourceWords.length - 5; i++) {
    sourceNgrams.add(sourceWords.slice(i, i + 5).join(' '));
  }

  const matchedPhrases = new Set<string>();
  const matchedWordIndices = new Set<number>();

  for (let i = 0; i <= noteWords.length - 5; i++) {
    const ngram = noteWords.slice(i, i + 5).join(' ');
    if (sourceNgrams.has(ngram)) {
      matchedPhrases.add(ngram);
      for (let j = 0; j < 5; j++) {
        matchedWordIndices.add(i + j);
      }
    }
  }

  const percent = Math.min(100, Math.round((matchedWordIndices.size / noteWords.length) * 100));

  return {
    percent,
    matchedPhrases: Array.from(matchedPhrases),
    noteWordCount: noteWords.length,
    sourceWordCount: sourceWords.length,
  };
}

const PEN_COLORS = [
  { id: 'ink', label: 'Ink', color: '#1e293b' },
  { id: 'blue', label: 'Blue', color: '#2563eb' },
  { id: 'emerald', label: 'Emerald', color: '#059669' },
  { id: 'violet', label: 'Violet', color: '#7c3aed' },
  { id: 'coral', label: 'Coral', color: '#dc2626' },
];

const PEN_WIDTHS = [
  { id: 'fine', label: 'Fine', width: 2 },
  { id: 'medium', label: 'Medium', width: 4 },
  { id: 'broad', label: 'Broad', width: 6 },
];

export function HandwrittenSynthesisTools({
  noteContent,
  sourceTexts,
  lessonKey,
  onInsertSynthesis,
  sketchpadOpen,
  onSketchpadOpenChange,
}: HandwrittenSynthesisToolsProps) {
  const overlap = useMemo(() => computeVerbatimOverlap(noteContent, sourceTexts), [noteContent, sourceTexts]);

  const [internalOpen, setInternalOpen] = useState(false);
  const isControlled = sketchpadOpen !== undefined;
  const isOpen = isControlled ? sketchpadOpen : internalOpen;

  const [selectedColor, setSelectedColor] = useState('#1e293b');
  const [selectedWidth, setSelectedWidth] = useState(3);
  const [isHighlighter, setIsHighlighter] = useState(false);
  const [takeawayText, setTakeawayText] = useState('');
  const [attachedToast, setAttachedToast] = useState(false);

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const isDrawing = useRef(false);
  const currentStroke = useRef<Stroke | null>(null);
  const [strokes, setStrokes] = useState<Stroke[]>([]);

  const storageKey = `handwritten_sketch_${lessonKey}`;

  const isWarning = overlap.noteWordCount >= 15 && overlap.percent > 25;
  const originalityPercent = Math.max(0, 100 - overlap.percent);

  const handleTogglePad = () => {
    const next = !isOpen;
    onSketchpadOpenChange?.(next);
    if (!isControlled) {
      setInternalOpen(next);
    }
  };

  // Load strokes
  useEffect(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved) {
        const parsed = JSON.parse(saved) as Stroke[];
        if (Array.isArray(parsed)) {
          setStrokes(parsed);
          return;
        }
      }
    } catch {
      // ignore
    }
    setStrokes([]);
  }, [storageKey]);

  // Redraw when strokes change or pad opens
  useEffect(() => {
    if (!isOpen) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Draw lined paper background
    ctx.save();
    ctx.strokeStyle = 'rgba(200, 210, 225, 0.35)';
    ctx.lineWidth = 1;
    for (let y = 30; y < canvas.height; y += 26) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(canvas.width, y);
      ctx.stroke();
    }
    ctx.restore();

    for (const stroke of strokes) {
      if (stroke.points.length === 0) continue;
      ctx.save();
      ctx.beginPath();
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      if (stroke.isHighlighter) {
        ctx.strokeStyle = stroke.color || '#facc15';
        ctx.lineWidth = stroke.width || 18;
        ctx.globalAlpha = 0.35;
      } else {
        ctx.strokeStyle = stroke.color || '#1e293b';
        ctx.lineWidth = stroke.width || 3;
        ctx.globalAlpha = 1;
      }

      const [first, ...rest] = stroke.points;
      ctx.moveTo(first.x, first.y);
      if (rest.length === 0) {
        ctx.lineTo(first.x + 0.1, first.y + 0.1);
      } else {
        for (const pt of rest) {
          ctx.lineTo(pt.x, pt.y);
        }
      }
      ctx.stroke();
      ctx.restore();
    }
  }, [isOpen, strokes]);

  const saveStrokesToStorage = (updated: Stroke[]) => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(updated));
    } catch {
      // ignore
    }
  };

  const getCanvasPoint = (e: React.PointerEvent<HTMLCanvasElement>): Point => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY,
    };
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    try {
      canvas.setPointerCapture(e.pointerId);
    } catch {
      // ignore
    }

    isDrawing.current = true;
    const pt = getCanvasPoint(e);
    const stroke: Stroke = {
      points: [pt],
      color: isHighlighter ? '#facc15' : selectedColor,
      width: isHighlighter ? 18 : selectedWidth,
      isHighlighter,
    };
    currentStroke.current = stroke;

    const ctx = canvas.getContext('2d');
    if (ctx) {
      ctx.save();
      ctx.beginPath();
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      if (stroke.isHighlighter) {
        ctx.strokeStyle = stroke.color;
        ctx.lineWidth = stroke.width;
        ctx.globalAlpha = 0.35;
      } else {
        ctx.strokeStyle = stroke.color;
        ctx.lineWidth = stroke.width;
        ctx.globalAlpha = 1;
      }
      ctx.moveTo(pt.x, pt.y);
      ctx.lineTo(pt.x + 0.1, pt.y + 0.1);
      ctx.stroke();
      ctx.restore();
    }
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isDrawing.current || !currentStroke.current) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const pt = getCanvasPoint(e);
    const prev = currentStroke.current.points[currentStroke.current.points.length - 1];
    currentStroke.current.points.push(pt);

    const ctx = canvas.getContext('2d');
    if (ctx && prev) {
      ctx.save();
      ctx.beginPath();
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      if (currentStroke.current.isHighlighter) {
        ctx.strokeStyle = currentStroke.current.color;
        ctx.lineWidth = currentStroke.current.width;
        ctx.globalAlpha = 0.35;
      } else {
        ctx.strokeStyle = currentStroke.current.color;
        ctx.lineWidth = currentStroke.current.width;
        ctx.globalAlpha = 1;
      }
      ctx.moveTo(prev.x, prev.y);
      ctx.lineTo(pt.x, pt.y);
      ctx.stroke();
      ctx.restore();
    }
  };

  const finishDrawing = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isDrawing.current) return;
    isDrawing.current = false;
    const canvas = canvasRef.current;
    if (canvas) {
      try {
        canvas.releasePointerCapture(e.pointerId);
      } catch {
        // ignore
      }
    }

    if (currentStroke.current && currentStroke.current.points.length > 0) {
      const next = [...strokes, currentStroke.current];
      setStrokes(next);
      saveStrokesToStorage(next);
      currentStroke.current = null;
    }
  };

  const handleUndo = () => {
    if (strokes.length === 0) return;
    const next = strokes.slice(0, -1);
    setStrokes(next);
    saveStrokesToStorage(next);
  };

  const handleClear = () => {
    setStrokes([]);
    saveStrokesToStorage([]);
  };

  const handleAttachTakeaway = () => {
    if (!takeawayText.trim() || !onInsertSynthesis) return;
    const date = new Date().toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
    const markdown = `> ✍️ **[Handwritten Takeaway — ${date}]**\n> ${takeawayText.trim()}`;
    onInsertSynthesis(markdown);
    setTakeawayText('');
    setAttachedToast(true);
    setTimeout(() => setAttachedToast(false), 2500);
  };

  return (
    <div className="mt-3 space-y-2.5">
      {/* 1. Own-Words Synthesis Indicator */}
      {!isWarning ? (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border-subtle bg-surface-sunken/40 px-3 py-1.5 text-xs">
          <div className="flex items-center gap-2">
            <span className="font-semibold text-text-primary">Own-Words Synthesis</span>
            <span className="text-text-muted">·</span>
            {overlap.noteWordCount < 15 ? (
              <span className="text-text-muted">
                Write at least 15 words in your own phrasing to evaluate synthesis.
              </span>
            ) : (
              <span className="inline-flex items-center gap-1.5 font-medium text-success-fg">
                <CheckCircle2 size={13} className="shrink-0" />
                Synthesized in your own words ({originalityPercent}% original phrasing)
              </span>
            )}
          </div>
        </div>
      ) : (
        <div className="rounded-xl border border-warning-muted bg-warning-muted/20 p-3.5 text-xs leading-5">
          <div className="flex items-center gap-2 font-semibold text-warning-fg">
            <AlertCircle size={14} className="shrink-0" />
            <span>High verbatim overlap ({overlap.percent}%)</span>
          </div>
          <p className="mt-2 text-text-secondary">
            <strong className="text-warning-fg">High word-for-word overlap detected ({overlap.percent}%).</strong> Writing forces your brain to synthesize and rephrase ideas rather than mindlessly transcribing word-for-word.
          </p>
          {overlap.matchedPhrases.length > 0 && (
            <div className="mt-2.5">
              <span className="text-[11px] font-medium text-text-primary">Copied source phrases to rephrase:</span>
              <div className="mt-1 flex flex-wrap gap-1.5">
                {overlap.matchedPhrases.slice(0, 4).map((phrase) => (
                  <span
                    key={phrase}
                    className="rounded border border-warning-muted/60 bg-surface px-2 py-0.5 font-mono text-[11px] text-text-secondary"
                  >
                    &ldquo;{phrase}&rdquo;
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* 2. Handwritten Notes / Freehand Ink Sketchpad */}
      <div className="rounded-xl border border-border-default bg-surface shadow-xs">
        <header className="flex items-center justify-between p-3.5">
          <div className="flex items-center gap-2">
            <span className="grid h-6 w-6 place-items-center rounded-md bg-accent/10 text-accent">
              <PenTool size={13} />
            </span>
            <div>
              <div className="text-xs font-semibold text-text-primary">Handwritten Note &amp; Sketch Pad</div>
              <div className="text-[11px] text-text-muted">
                Engages motor-perceptual memory and forces synthesis
              </div>
            </div>
          </div>

          <button
            type="button"
            onClick={handleTogglePad}
            aria-expanded={isOpen}
            className="inline-flex items-center gap-1.5 rounded-full border border-border-default bg-surface-sunken px-3 py-1 text-xs font-medium text-text-secondary hover:text-text-primary"
          >
            <span className="sr-only">Handwritten Note &amp; Sketch Pad &mdash; </span>
            <span>{isOpen ? 'Hide sketchpad' : 'Open sketchpad'}</span>
            {isOpen ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
          </button>
        </header>

        {isOpen && (
          <div className="border-t border-border-subtle p-3.5 sm:p-4">
            <p className="mb-3 text-xs leading-5 text-text-secondary">
              Write key takeaways down by hand instead of typing. Handwriting engages motor-perceptual memory and forces synthesis. Use a stylus, Apple Pencil, trackpad, or mouse.
            </p>

            {/* Toolbar */}
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border-default bg-surface-sunken/60 p-2 text-xs">
              {/* Color choices */}
              <div className="flex items-center gap-1.5">
                <span className="mr-1 text-[11px] text-text-muted">Ink:</span>
                {PEN_COLORS.map((c) => (
                  <button
                    key={c.id}
                    type="button"
                    title={c.label}
                    onClick={() => {
                      setSelectedColor(c.color);
                      setIsHighlighter(false);
                    }}
                    className={`h-5 w-5 rounded-full border transition-transform ${
                      !isHighlighter && selectedColor === c.color ? 'scale-110 border-accent ring-2 ring-accent/30' : 'border-border-default'
                    }`}
                    style={{ backgroundColor: c.color }}
                  />
                ))}

                <button
                  type="button"
                  onClick={() => setIsHighlighter((v) => !v)}
                  className={`ml-2 inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition-colors ${
                    isHighlighter
                      ? 'bg-yellow-400/20 text-yellow-600 dark:text-yellow-400 ring-1 ring-yellow-400'
                      : 'border border-border-default bg-surface text-text-secondary hover:text-text-primary'
                  }`}
                >
                  <Highlighter size={12} />
                  Highlighter
                </button>
              </div>

              {/* Width choices */}
              <div className="flex items-center gap-2">
                <span className="text-[11px] text-text-muted">Width:</span>
                {PEN_WIDTHS.map((w) => (
                  <button
                    key={w.id}
                    type="button"
                    onClick={() => setSelectedWidth(w.width)}
                    className={`rounded px-2 py-0.5 text-[11px] ${
                      selectedWidth === w.width
                        ? 'bg-accent text-accent-fg font-medium'
                        : 'text-text-muted hover:text-text-primary'
                    }`}
                  >
                    {w.label}
                  </button>
                ))}
              </div>

              {/* Actions */}
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  onClick={handleUndo}
                  disabled={strokes.length === 0}
                  className="inline-flex items-center gap-1 rounded border border-border-default bg-surface px-2 py-1 text-[11px] text-text-secondary hover:text-text-primary disabled:opacity-40"
                >
                  <Undo2 size={12} />
                  Undo
                </button>
                <button
                  type="button"
                  onClick={handleClear}
                  disabled={strokes.length === 0}
                  className="inline-flex items-center gap-1 rounded border border-border-default bg-surface px-2 py-1 text-[11px] text-text-secondary hover:text-danger-fg disabled:opacity-40"
                >
                  <RotateCcw size={12} />
                  Clear
                </button>
              </div>
            </div>

            {/* Canvas Surface */}
            <div className="overflow-hidden rounded-xl border border-border-default bg-surface-sunken">
              <canvas
                ref={canvasRef}
                aria-label="Handwritten sketchpad canvas"
                width={720}
                height={260}
                onPointerDown={handlePointerDown}
                onPointerMove={handlePointerMove}
                onPointerUp={finishDrawing}
                onPointerCancel={finishDrawing}
                className="h-64 w-full touch-none cursor-crosshair bg-surface"
              />
            </div>

            {/* Transcribe / Attach Takeaway */}
            <div className="mt-3.5 flex flex-wrap items-center gap-2">
              <input
                type="text"
                value={takeawayText}
                onChange={(e) => setTakeawayText(e.target.value)}
                placeholder="Transcribe key takeaway from handwriting (e.g. 'Always test hypothesis against counter-evidence')..."
                className="min-w-64 flex-1 rounded-xl border border-border-default bg-surface-sunken px-3.5 py-2 text-xs text-text-primary focus:border-accent focus:outline-hidden"
              />
              <button
                type="button"
                onClick={handleAttachTakeaway}
                disabled={!takeawayText.trim() || !onInsertSynthesis}
                className="inline-flex items-center gap-1.5 rounded-full bg-accent px-3.5 py-2 text-xs font-semibold text-accent-fg hover:opacity-95 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {attachedToast ? <Check size={13} /> : <Sparkles size={13} />}
                {attachedToast ? 'Attached!' : 'Attach Takeaway to Note'}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
