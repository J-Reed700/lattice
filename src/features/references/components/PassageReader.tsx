import { useState } from 'react';

import { ArrowUpRight, Check, Copy, Quote, Trash2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { ReferenceAnnotationStrip } from '@/features/references/components/ReferenceAnnotationStrip';
import { passageTitle } from '@/features/references/model/inboxItems';
import type { PassageReferenceDto } from '@/types/api/references';

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
 * Reader pane for a saved passage: editorial eyebrow, serif heading,
 * framed passage quotation, inline annotation card, and unified action rail.
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
    <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-bg">
      <div className="mx-auto flex w-full max-w-[clamp(680px,72vw,900px)] flex-1 flex-col px-6 pt-10 pb-8">
        <header>
          <span className="inline-flex items-center gap-1.5 rounded-md bg-[hsl(var(--text-primary)/0.055)] px-2 py-0.5 text-[11px] font-medium text-text-secondary">
            <Quote className="h-3 w-3 text-accent" strokeWidth={1.75} aria-hidden="true" />
            Saved passage
          </span>
          <h1 className="mt-2.5 font-serif text-2xl font-normal tracking-tight text-text-primary">
            {displayTitle}
          </h1>
          <p className="mt-1.5 text-sm text-text-tertiary">
            From{' '}
            <button
              type="button"
              onClick={onOpenSource}
              className="text-text-secondary underline-offset-2 transition-colors duration-fast hover:text-accent hover:underline"
            >
              {passage.fileName}
            </button>
            {locator ? <span> · {locator}</span> : null}
            {capturedOn ? <span> · {capturedOn}</span> : null}
          </p>
        </header>

        <div className="mt-6 rounded-xl border border-border-subtle bg-surface p-5 shadow-control">
          <blockquote className="whitespace-pre-wrap border-l-2 border-accent/60 pl-4 font-serif text-base leading-[1.7] text-text-primary">
            {passage.text}
          </blockquote>
        </div>

        <ReferenceAnnotationStrip
          id={passage.id}
          title={passage.title}
          note={passage.note}
          onSave={onSaveAnnotations}
        />

        <p className="mt-6 text-xs text-text-muted">
          Lattice uses your saved references in future answers.
        </p>

        <div className="mt-6 flex flex-wrap items-center justify-between gap-2 border-t border-border-subtle pt-4">
          <div className="flex items-center gap-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={onOpenSource}
              title="Open the source document"
              className="h-8 rounded-lg px-2.5 text-xs text-text-secondary hover:text-text-primary"
            >
              <ArrowUpRight className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
              Open source
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => void handleCopy()}
              title="Copy the saved passage"
              className="h-8 rounded-lg px-2.5 text-xs text-text-secondary hover:text-text-primary"
            >
              {copiedTick ? (
                <Check className="h-3.5 w-3.5 text-accent" strokeWidth={2} aria-hidden="true" />
              ) : (
                <Copy className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
              )}
              Copy
            </Button>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onDelete}
            title="Delete reference"
            className="h-8 rounded-lg px-2.5 text-xs text-text-muted hover:bg-danger-muted hover:text-danger-fg"
          >
            <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
            Delete
          </Button>
        </div>
      </div>
    </main>
  );
}
