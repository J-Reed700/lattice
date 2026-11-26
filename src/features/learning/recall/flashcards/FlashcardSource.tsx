import { useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import { CitationCard } from '@/features/reading/citations';
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

  const targets = useMemo(() => sources.map((source) => fromStudySource(source)), [sources]);
  const allSources = useMemo(() => targets.map((t) => t.source), [targets]);

  if (!sources.length) return null;

  return (
    <details className="mt-5 border-t border-border-subtle pt-4 text-sm">
      <summary className="cursor-pointer text-text-secondary">
        {sources.length === 1 ? `Citation · ${sources[0].fileName}` : `Citations · ${sources.length} sources`}
      </summary>
      <div className="space-y-4 py-4">
        {sources.map((source, index) => {
          const href = safeExternalHref(source.url);
          const filePath = source.filePath?.trim();
          const hasTarget = Boolean(href || filePath || source.documentId);
          const excerpts = source.support?.anchors.map((anchor) => anchor.exact).filter(Boolean) ?? [];
          const passage = excerpts.length === 0 ? (source.excerpt?.trim() || undefined) : undefined;
          const site = href ? new URL(href).hostname : undefined;
          const openButton = hasTarget && href ? (
            <Button
              className="mt-2"
              variant="secondary"
              size="sm"
              onClick={() => setOpenIndex(index)}
            >
              Open reference
            </Button>
          ) : !hasTarget ? (
            <p className="mt-2 text-xs text-text-muted">Saved source snapshot</p>
          ) : undefined;
          const title = source.fileName?.trim() || 'Saved source';
          return (
            <div key={`${source.chunkId}-${source.citationId ?? 'unnumbered'}-${index}`}>
              {typeof source.citationId === 'number' && source.citationId > 0 ? (
                <CitationCard
                  number={source.citationId}
                  title={title}
                  site={site}
                  relation={source.support?.relation}
                  excerpts={excerpts}
                  passage={passage}
                  onOpen={hasTarget && !href ? () => setOpenIndex(index) : undefined}
                  footer={openButton}
                />
              ) : (
                <div>
                  <p className="mb-2 text-xs font-medium text-text-primary">{title}</p>
                  {source.excerpt && (
                    <p className="leading-relaxed text-text-secondary">{source.excerpt}</p>
                  )}
                  {hasTarget && !href ? (
                    <Button
                      className="mt-2"
                      variant="secondary"
                      size="sm"
                      onClick={() => setOpenIndex(index)}
                    >
                      Open source
                    </Button>
                  ) : openButton}
                </div>
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
