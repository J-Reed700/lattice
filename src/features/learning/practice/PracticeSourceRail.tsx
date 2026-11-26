import { useEffect, useMemo, useRef } from 'react';

import { BookOpen, ChevronRight } from 'lucide-react';

import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import { numberedSavedSourceCitations, SavedSourceCitations } from '@/features/learning/sources/SavedSourceCitations';
import { useLearningSourceWorkspace } from '@/features/learning/sources/useLearningSources';
import { useSavedSourceReader } from '@/features/learning/sources/useSavedSourceReader';
import { useReaderClosesWithOwner } from '@/features/reading/hooks/useReaderClosesWithOwner';
import type { LearningPracticeCitationDto, LearningSourceLibraryItemDto, SupportDto } from '@/lib/bindings';

function dateLabel(value: number) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'Time unavailable' : date.toLocaleString();
}

export function sourceForVersion(
  sources: LearningSourceLibraryItemDto[],
  sourceId: string,
  versionId: string,
) {
  return sources.find((source) => source.id === sourceId && source.versions.some((version) => version.id === versionId));
}

export function FrozenSourceRail({
  programId,
  versionIds,
  onOpen,
  disabledReason,
}: {
  programId: string;
  versionIds: string[];
  onOpen: (source: LearningSourceLibraryItemDto, versionId: string) => void;
  disabledReason?: string;
}) {
  const workspace = useLearningSourceWorkspace(programId);
  const frozen = useMemo(() => {
    const versionSet = new Set(versionIds);
    return (workspace.data?.sources ?? []).flatMap((source) =>
      source.versions.filter((version) => versionSet.has(version.id)).map((version) => ({ source, version })),
    );
  }, [workspace.data, versionIds]);

  return (
    <section aria-labelledby="frozen-sources-heading" className="rounded-2xl border border-border-default bg-surface p-5">
      <div className="flex items-center gap-2 text-accent">
        <BookOpen size={15} />
        <span className="text-xs font-semibold uppercase tracking-[.15em]">Frozen source context</span>
      </div>
      <h3 id="frozen-sources-heading" className="mt-1 font-serif text-xl text-text-primary">
        Saved versions for this attempt
      </h3>
      <p className="mt-1 text-xs leading-5 text-text-muted">
        This attempt keeps its original versions even if the library changes later.
      </p>
      {disabledReason && (
        <p className="mt-3 rounded-lg bg-surface-sunken p-3 text-xs leading-5 text-text-secondary">
          {disabledReason}
        </p>
      )}
      {workspace.isLoading ? (
        <p role="status" className="mt-4 text-xs text-text-muted">Loading frozen sources…</p>
      ) : workspace.isError ? (
        <div className="mt-4">
          <StatusLine state={{ kind: 'error', message: workspace.error.message, retry: () => void workspace.refetch() }} />
        </div>
      ) : frozen.length ? (
        <div className="mt-4 space-y-2">
          {frozen.map(({ source, version }) => (
            <button
              key={version.id}
              type="button"
              disabled={Boolean(disabledReason)}
              onClick={() => onOpen(source, version.id)}
              className="w-full rounded-xl border border-border-default bg-surface p-3 text-left transition hover:border-accent focus-visible:outline-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-55"
            >
              <span className="flex items-start justify-between gap-2">
                <span className="min-w-0">
                  <span className="block truncate text-xs font-semibold text-text-primary">{version.title}</span>
                  <span className="mt-1 block text-xs text-text-muted">
                    Version {version.versionNumber} · saved {dateLabel(version.acquiredAt)}
                  </span>
                </span>
                <ChevronRight size={14} className="mt-1 shrink-0 text-text-muted" />
              </span>
              <span className="mt-2 line-clamp-3 block text-xs leading-5 text-text-secondary">{version.excerpt}</span>
            </button>
          ))}
        </div>
      ) : (
        <p className="mt-4 rounded-lg bg-surface-sunken p-3 text-xs text-text-muted">
          No frozen source versions are attached to this session.
        </p>
      )}
    </section>
  );
}

/**
 * The saved sources a tutor turn, hint or worked solution cites: numbered chips
 * with their quotes, as chat answers and lessons draw theirs. Opening one goes
 * through `onOpen`, which records the opening on the attempt before the docked
 * reader shows the source. Like any citation, they hide in reading mode.
 */
export function PracticeCitations({
  programId,
  citations,
  onOpen,
  disabledReason,
}: {
  programId: string;
  citations: LearningPracticeCitationDto[];
  onOpen: (citation: LearningPracticeCitationDto) => void;
  /** Why sources cannot be opened right now; the chips still show what is cited. */
  disabledReason?: string;
}) {
  const numbered = numberedSavedSourceCitations(citations);
  return (
    <SavedSourceCitations
      programId={programId}
      citations={numbered}
      quotes
      label="Saved sources cited"
      disabledReason={disabledReason}
      onOpen={(_citation, index) => onOpen(numbered[index]!)}
    />
  );
}

/** Why an attempt's sources cannot be opened now, or nothing when they can. */
export function sourceClosedReason(sourceOpeningAllowed: boolean, busy: boolean): string | undefined {
  if (!sourceOpeningAllowed) return 'Sources are closed with No help.';
  return busy ? 'Another workbench action is in progress.' : undefined;
}

/**
 * Shows the source an attempt opened in the docked source reader: the same
 * reader, with the same marked passage, that chat and lessons open sources in.
 * The attempt records each opening first, so this follows what it opened. It
 * opens only when the selection itself changes, so a source the person closed
 * stays closed when the owner key changes (a new attempt, say).
 */
export function usePracticeSourceReader(
  programId: string,
  ownerKey: string,
  selected: { sourceId: string; versionId: string; quote?: string | null; number?: number; support?: SupportDto | null } | null,
) {
  const reader = useSavedSourceReader(programId);
  useReaderClosesWithOwner(ownerKey);
  const openRef = useRef(reader.open);
  openRef.current = reader.open;
  const ownerRef = useRef(ownerKey);
  ownerRef.current = ownerKey;
  useEffect(() => {
    // A source opened from a citation carries its quoted words and is a citation
    // like any other: reading mode closes it, as in chat and lessons. One opened
    // from the source rail has no quote and is read for itself, so it stays open.
    if (selected) void openRef.current(ownerRef.current, [selected], { citing: Boolean(selected.quote) });
  }, [selected]);
}
