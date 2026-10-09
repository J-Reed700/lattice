import { useMemo, useState } from 'react';

import { AlignLeft, BookOpenText } from 'lucide-react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { formatSourceForReader } from '@/features/learning/sources/sourceReaderFormat';

export function SourceReader({
  fullText,
  extractionVersion,
  wordCount,
}: {
  fullText: string;
  extractionVersion: string;
  wordCount: number;
}) {
  const [view, setView] = useState<'reader' | 'exact'>('reader');
  const formatted = useMemo(
    () => formatSourceForReader(fullText, extractionVersion),
    [extractionVersion, fullText],
  );
  const readingMinutes = Math.max(1, Math.ceil(wordCount / 225));

  return (
    <section className="mt-4 overflow-hidden rounded-2xl border border-border bg-background" aria-label="Saved source reader">
      <header className="flex flex-wrap items-center justify-between gap-3 border-b border-border bg-surface/70 px-4 py-3 sm:px-5">
        <div>
          <p className="text-xs font-medium text-text-primary">Saved source</p>
          <p className="mt-0.5 text-[10px] text-text-muted">
            {wordCount.toLocaleString()} words · about {readingMinutes} min
          </p>
        </div>
        <div className="flex rounded-full border border-border bg-background p-1" role="group" aria-label="Source reading view">
          <button
            type="button"
            aria-pressed={view === 'reader'}
            onClick={() => setView('reader')}
            className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1.5 text-[10px] font-medium transition ${view === 'reader' ? 'bg-accent text-accent-fg shadow-sm' : 'text-text-muted hover:text-text-primary'}`}
          >
            <BookOpenText size={12} /> Reader
          </button>
          <button
            type="button"
            aria-pressed={view === 'exact'}
            onClick={() => setView('exact')}
            className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1.5 text-[10px] font-medium transition ${view === 'exact' ? 'bg-accent text-accent-fg shadow-sm' : 'text-text-muted hover:text-text-primary'}`}
          >
            <AlignLeft size={12} /> Exact capture
          </button>
        </div>
      </header>
      <div className="max-h-[68vh] min-h-60 overflow-auto px-5 py-7 sm:px-8 sm:py-10">
        {view === 'reader' ? (
          <article className="source-reader-prose mx-auto max-w-[76ch] font-serif text-[15px] leading-7 text-text-primary sm:text-base">
            <TiptapViewer content={formatted} />
          </article>
        ) : (
          <div className="mx-auto max-w-[92ch]">
            <p className="mb-4 rounded-lg border border-border bg-surface px-3 py-2 text-[10px] leading-4 text-text-muted">
              This is the immutable text used for evidence, search, and the saved content hash.
            </p>
            <pre className="whitespace-pre-wrap wrap-break-word font-mono text-xs leading-6 text-text-secondary">{fullText}</pre>
          </div>
        )}
      </div>
    </section>
  );
}
