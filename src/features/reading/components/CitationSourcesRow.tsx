import { CitationFootnote } from '@/features/reading/components/CitationFootnote';
import type { SourceWithMetadata } from '@/types/conversation';

export interface CitationSourcesRowEntry {
  number: number;
  source: SourceWithMetadata;
  onView: () => void;
  /** A stable React key; the number when omitted. */
  key?: string;
  provenanceLabel?: string;
  resolvedLocation?: string;
  occurrence?: number | null;
  disabledReason?: string;
  /** The source's name shown after its chip, for rows that list sources rather than only number them. */
  title?: string;
}

/**
 * The "Sources" label and the row of numbered chips under a piece of cited
 * text, drawn once so a chat answer, a lesson section and a saved-source list
 * show their citations the same way.
 */
export function CitationSourcesRow({
  entries,
  label = 'Sources',
  className = 'mt-1',
}: {
  entries: CitationSourcesRowEntry[];
  label?: string;
  /** Positioning only. */
  className?: string;
}) {
  return (
    <div className={`${className} flex flex-wrap items-center gap-2 text-xs`}>
      <span className="text-xxs text-text-muted">{label}</span>
      {entries.map((entry) => {
        const key = entry.key ?? `cite-${entry.number}`;
        const chip = (
          <CitationFootnote
            key={entry.title === undefined ? key : undefined}
            number={entry.number}
            source={entry.source}
            provenanceLabel={entry.provenanceLabel}
            resolvedLocation={entry.resolvedLocation}
            occurrence={entry.occurrence}
            disabledReason={entry.disabledReason}
            onViewFile={entry.onView}
          />
        );
        if (entry.title === undefined) return chip;
        return (
          <span key={key} className="inline-flex items-center gap-1.5">
            {chip}
            <span className="max-w-[18rem] truncate text-text-secondary">{entry.title}</span>
          </span>
        );
      })}
    </div>
  );
}
