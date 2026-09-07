import { useState } from 'react';

import { ArrowUpRight, Check, Copy, Trash2 } from 'lucide-react';

import type { PassageReferenceDto } from '@/types/api/references';

import { passageTitle } from './inboxItems';
import { ReferenceAnnotationStrip } from './ReferenceAnnotationStrip';

interface PassageReaderProps {
  passage: PassageReferenceDto;
  onSaveAnnotations: (next: {
    title: string | null;
    note: string | null;
  }) => Promise<boolean>;
  onOpenSource: () => void;
  onCopy: () => Promise<boolean>;
  onDelete: () => void;
}

function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}

/**
 * Reader pane for a saved passage: the same reading column as the message
 * reader, with the excerpt set as a left-ruled quotation.
 */
export function PassageReader({
  passage,
  onSaveAnnotations,
  onOpenSource,
  onCopy,
  onDelete,
}: PassageReaderProps) {
  const [copiedTick, setCopiedTick] = useState(false);

  const handleCopy = async () => {
    const ok = await onCopy();
    if (!ok) return;
    setCopiedTick(true);
    window.setTimeout(() => setCopiedTick(false), 1300);
  };

  const displayTitle = passageTitle(passage);
  const locator = passage.locator?.trim();
  const capturedOn = formatDate(passage.createdAt);

  return (
    <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-[hsl(var(--bg))]">
      <div className="mx-auto flex w-full max-w-[clamp(680px,72vw,900px)] flex-1 flex-col px-6 pt-10 pb-8">
        <header>
          <h1 className="font-serif text-2xl font-semibold text-[hsl(var(--text-primary))]">
            {displayTitle}
          </h1>
          <p className="mt-1 text-sm text-[hsl(var(--text-tertiary))]">
            From{' '}
            <button
              type="button"
              onClick={onOpenSource}
              className="text-[hsl(var(--text-secondary))] underline-offset-2 transition-colors duration-fast hover:text-[hsl(var(--accent))] hover:underline"
            >
              {passage.fileName}
            </button>
            {locator ? <span> · {locator}</span> : null}
            {capturedOn ? <span> · {capturedOn}</span> : null}
          </p>
        </header>

        <blockquote className="mt-6 whitespace-pre-wrap border-l-2 border-[hsl(var(--border-default))] pl-4 font-serif text-base leading-[1.65] text-[hsl(var(--text-primary))]">
          {passage.text}
        </blockquote>

        <ReferenceAnnotationStrip
          id={passage.id}
          title={passage.title}
          note={passage.note}
          onSave={onSaveAnnotations}
        />

        <p className="mt-6 text-xs text-[hsl(var(--text-muted))]">
          Lattice uses your saved references in future answers.
        </p>

        <div className="mt-6 flex items-center justify-between border-t border-[hsl(var(--border-subtle))] pt-4">
          <div className="flex items-center gap-4">
            <button
              type="button"
              onClick={onOpenSource}
              title="Open the source document"
              className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))] transition-colors duration-fast"
            >
              <ArrowUpRight className="h-3.5 w-3.5" strokeWidth={1.75} />
              Open source
            </button>
            <button
              type="button"
              onClick={() => void handleCopy()}
              title="Copy the saved passage"
              className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))] transition-colors duration-fast"
            >
              {copiedTick ? (
                <Check className="h-3.5 w-3.5 text-[hsl(var(--accent))]" strokeWidth={2} />
              ) : (
                <Copy className="h-3.5 w-3.5" strokeWidth={1.75} />
              )}
              Copy
            </button>
          </div>
          <button
            type="button"
            onClick={onDelete}
            title="Delete reference"
            className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-muted))] hover:text-[hsl(var(--danger-fg))] transition-colors duration-fast"
          >
            <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
            Delete
          </button>
        </div>
      </div>
    </main>
  );
}
