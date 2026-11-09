import { useMemo } from 'react';
import { BookOpen, ChevronRight, X } from 'lucide-react';

import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import { SourceReader } from '@/features/learning/sources/SourceReader';
import { useLearningSourceVersion, useLearningSourceWorkspace } from '@/features/learning/sources/useLearningSources';
import type { LearningSourceLibraryItemDto } from '@/lib/bindings';

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

export function CitationCards({
  citations,
  onOpen,
  disabled = false,
}: {
  citations: { sourceId: string; versionId: string; quote: string }[];
  onOpen: (citation: { sourceId: string; versionId: string; quote: string }) => void;
  disabled?: boolean;
}) {
  return (
    <div className="mt-3 grid gap-2">
      {citations.map((citation, index) => (
        <button
          key={`${citation.sourceId}-${citation.versionId}-${index}`}
          type="button"
          disabled={disabled}
          onClick={() => onOpen(citation)}
          className="rounded-lg border border-accent bg-accent-muted p-3 text-left focus-visible:outline-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-55"
        >
          <span className="text-xs font-semibold uppercase tracking-wider text-accent">
            Saved source quote · exact version
          </span>
          <span className="mt-1 block text-xs leading-5 text-text-secondary">“{citation.quote}”</span>
          <span className="mt-2 block text-xs text-text-muted">
            {disabled ? 'Unavailable during this action' : 'Open frozen source'}
          </span>
        </button>
      ))}
    </div>
  );
}

export function ExactSourceReader({
  query,
  onClose,
}: {
  query: ReturnType<typeof useLearningSourceVersion>;
  onClose: () => void;
}) {
  return (
    <section aria-labelledby="exact-source-title" className="rounded-2xl border border-accent bg-surface p-5">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-xs font-semibold uppercase tracking-[.15em] text-accent">Immutable source version</div>
          <h4 id="exact-source-title" className="mt-1 font-serif text-lg text-text-primary">
            {query.data?.version.title ?? 'Saved source'}
          </h4>
        </div>
        <button type="button" onClick={onClose} aria-label="Close saved source reader" className="rounded-full p-1.5 text-text-muted hover:bg-surface-sunken">
          <X size={15} />
        </button>
      </div>
      {query.isLoading ? (
        <p role="status" className="mt-3 text-xs text-text-muted">Opening exact saved version…</p>
      ) : query.isError ? (
        <div className="mt-3">
          <StatusLine state={{ kind: 'error', message: query.error.message, retry: () => void query.refetch() }} />
        </div>
      ) : query.data && (
        <>
          <p className="mt-1 text-xs text-text-muted">
            Version {query.data.version.versionNumber} · saved {dateLabel(query.data.version.acquiredAt)}
          </p>
          <SourceReader fullText={query.data.fullText} extractionVersion={query.data.version.extractionVersion} wordCount={query.data.version.wordCount} />
          <p className="mt-2 text-xs leading-4 text-text-muted">
            This reader displays the locally saved version. It does not fetch the reference URL.
          </p>
        </>
      )}
    </section>
  );
}
