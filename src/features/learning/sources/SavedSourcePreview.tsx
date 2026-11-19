import { useLayoutEffect, useMemo, useRef, useState } from 'react';

import { formatSourceForReader } from '@/features/learning/sources/sourceReaderFormat';
import { ArticleText } from '@/features/reading/components/ArticleText';
import { articleBlocks } from '@/features/reading/model/articleBlocks';
import { blocksWithin } from '@/features/reading/model/inlineMarkdown';

/** How much of the saved text the preview shows, in whole blocks, before the reader takes over. */
const PREVIEW_CHARS = 900;

/**
 * A saved version as the Sources library shows it: what it is and the opening
 * of its text. Reading it is the library header's "Open in reader", which
 * opens the docked source reader (the reader chat answers and lessons open
 * their citations in). The immutable capture stays one click away for
 * checking what evidence and hashes used.
 */
export function SavedSourcePreview({
  fullText,
  extractionVersion,
  wordCount,
}: {
  fullText: string;
  extractionVersion: string;
  wordCount: number;
}) {
  const opening = useMemo(() => {
    const text = formatSourceForReader(fullText, extractionVersion, { escapeMarkup: false });
    return { text, truncated: blocksWithin(text, articleBlocks(text), PREVIEW_CHARS).truncated, empty: !text.trim() };
  }, [extractionVersion, fullText]);
  const boxRef = useRef<HTMLDivElement>(null);
  const [overflows, setOverflows] = useState(false);
  // Whole blocks can still be taller than the box, so the fade also follows what is measured.
  useLayoutEffect(() => {
    const box = boxRef.current;
    if (!box) return undefined;
    const measure = () => setOverflows(box.scrollHeight > box.clientHeight);
    measure();
    if (typeof ResizeObserver === 'undefined') return undefined;
    const observer = new ResizeObserver(measure);
    observer.observe(box);
    return () => observer.disconnect();
  }, [opening]);
  const readingMinutes = Math.max(1, Math.ceil(wordCount / 225));

  return (
    <section className="mt-4 overflow-hidden rounded-2xl border border-border bg-background" aria-label="Saved source">
      <header className="flex flex-wrap items-center justify-between gap-3 border-b border-border bg-surface/70 px-4 py-3 sm:px-5">
        <div>
          <p className="text-xs font-medium text-text-primary">Saved source</p>
          <p className="mt-0.5 text-[10px] text-text-muted">
            {wordCount.toLocaleString()} words · about {readingMinutes} min
          </p>
        </div>
      </header>
      <div className="px-5 py-5 sm:px-7">
        {opening.empty ? (
          <p className="font-serif text-[15px] leading-7 text-text-secondary">This saved version has no text.</p>
        ) : (
          <div ref={boxRef} className="relative max-h-72 overflow-hidden text-text-secondary">
            <ArticleText text={opening.text} maxChars={PREVIEW_CHARS} />
            {(opening.truncated || overflows) && (
              <div
                aria-hidden="true"
                className="pointer-events-none absolute inset-x-0 bottom-0 h-16 bg-linear-to-t from-background to-transparent"
              />
            )}
          </div>
        )}
        <details className="mt-4 group">
          <summary className="cursor-pointer text-[11px] font-medium text-text-muted hover:text-text-primary">Exact capture</summary>
          <p className="mt-3 rounded-lg border border-border bg-surface px-3 py-2 text-[10px] leading-4 text-text-muted">
            This is the immutable text used for evidence, search, and the saved content hash.
          </p>
          <pre className="mt-3 max-h-[60vh] overflow-auto whitespace-pre-wrap wrap-break-word font-mono text-xs leading-6 text-text-secondary">{fullText}</pre>
        </details>
      </div>
    </section>
  );
}
