import { TrustBadge } from '@/features/learning/sources/trust/TrustBadge';
import { TrustOverrideMenu } from '@/features/learning/sources/trust/TrustOverrideMenu';
import { useReassessSourceTrust, useSourceTrust } from '@/features/learning/sources/trust/useSourceTrust';

/**
 * A source's trust in the Sources tab : its level with the reasons,
 * "Assess" to judge its publisher (again), and the override menu. The
 * level here holds whatever the claim's subject; a source is primary only
 * for a subject it owns, which claim cards show.
 */
export function SourceTrustSection({
  programId,
  sourceId,
  sourceTitle,
  titleOf,
}: {
  programId: string;
  sourceId: string;
  sourceTitle: string;
  titleOf?: (versionId: string) => string | null;
}) {
  const trust = useSourceTrust({ programId, sourceIds: [sourceId], claim: null, includeImpact: true });
  const reassess = useReassessSourceTrust();
  const entry = trust.data?.sources[0] ?? null;
  return (
    <section aria-label="Source trust" className="rounded-xl border border-border-default bg-surface-sunken/40 p-4">
      <h4 className="text-[10px] font-semibold uppercase tracking-wider text-text-muted">Trust</h4>
      {trust.isLoading && <p role="status" className="mt-2 text-xs text-text-muted">Reading this source’s trust…</p>}
      {trust.error && <p role="alert" className="mt-2 text-xs text-danger-fg">Its trust could not be read: {trust.error.message}</p>}
      {trust.isSuccess && !entry && <p className="mt-2 text-xs text-text-muted">This source has no saved version to judge.</p>}
      {entry && (
        <div className="mt-2 space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <TrustBadge trust={entry.trust} entry={entry} titleOf={titleOf} />
            <button
              type="button"
              disabled={reassess.isPending}
              onClick={() => reassess.mutate({ programId, sourceId })}
              className="min-h-8 rounded-full border border-border-default bg-surface px-3 text-xs font-medium text-text-primary hover:bg-surface-sunken disabled:opacity-50"
            >
              {reassess.isPending ? 'Assessing…' : entry.standing ? 'Assess again' : 'Assess'}
            </button>
          </div>
          <p className="text-[11px] leading-5 text-text-muted">A source is primary only for a subject it owns; each claim card shows the level for that claim’s subject.</p>
          {reassess.error && <p role="alert" className="text-xs text-danger-fg">{reassess.error.message}</p>}
          <TrustOverrideMenu entry={entry} programId={programId} sourceTitle={sourceTitle} />
        </div>
      )}
    </section>
  );
}
