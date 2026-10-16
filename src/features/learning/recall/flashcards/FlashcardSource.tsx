import { useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import { FilePreviewModal } from '@/features/reading/components/FilePreviewModal';
import { fromStudySource } from '@/features/reading/model/sourceAdapter';
import type { StudySourceDto } from '@/lib/bindings';

function safeExternalHref(value?: string | null) {
  if (!value) return undefined;
  try {
    const parsed = new URL(value);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:' ? parsed.href : undefined;
  } catch {
    return undefined;
  }
}

export function FlashcardSource({ sources }: { sources: StudySourceDto[] }) {
  const [openIndex, setOpenIndex] = useState<number | null>(null);

  const targets = useMemo(() => sources.map((s, idx) => fromStudySource(s, idx)), [sources]);
  const allSources = useMemo(() => targets.map((t) => t.source), [targets]);

  if (!sources.length) return null;

  return (
    <details className="mt-5 border-t border-border-subtle pt-4 text-sm">
      <summary className="cursor-pointer text-text-secondary">
        {sources.length === 1 ? `Citation · ${sources[0].fileName}` : `Citations · ${sources.length} sources`}
      </summary>
      <div className="space-y-5 py-4">
        {sources.map((source, index) => {
          const href = safeExternalHref(source.url);
          const filePath = source.filePath?.trim();
          const hasTarget = Boolean(href || filePath || source.documentId);
          return (
            <div key={`${source.chunkId}-${index}`}>
              {sources.length > 1 && (
                <p className="mb-2 text-xs font-medium text-text-primary">
                  [{index + 1}] {source.fileName}
                </p>
              )}
              <blockquote className="border-l-2 border-accent pl-4 leading-relaxed text-text-secondary">
                {source.excerpt}
              </blockquote>
              {hasTarget ? (
                <Button
                  className="mt-3"
                  variant="secondary"
                  size="sm"
                  onClick={() => setOpenIndex(index)}
                >
                  {href ? 'Open reference' : 'Open source'}
                </Button>
              ) : (
                <p className="mt-3 text-xs text-text-muted">Saved source snapshot</p>
              )}
            </div>
          );
        })}
      </div>
      <FilePreviewModal
        isOpen={openIndex !== null && Boolean(targets[openIndex])}
        onClose={() => setOpenIndex(null)}
        source={openIndex !== null ? (targets[openIndex]?.source ?? null) : null}
        initialLocator={openIndex !== null ? (targets[openIndex]?.locator ?? null) : null}
        citations={allSources}
        citationIndex={openIndex ?? undefined}
        onCitationIndexChange={setOpenIndex}
      />
    </details>
  );
}
