import { TrustBadge } from '@/features/learning/sources/trust/TrustBadge';
import { TrustOverrideMenu } from '@/features/learning/sources/trust/TrustOverrideMenu';
import { bestLevel, bestOf, insufficiencyExplanation, sufficiencyHeadline } from '@/features/learning/sources/trust/trustPresentation';
import { useReprofileClaim, useSourceTrust } from '@/features/learning/sources/trust/useSourceTrust';
import type { ClaimReviewItemDto } from '@/lib/bindings';

/** A run of a code example is not a source and has no standing. */
const EXECUTION = 'execution';

/**
 * The review card's support block (§19): each supporting source's level for
 * the claim's subject, the claim's kind and the bar it must meet, and, for a
 * claim the provenance gate held back (`item.sufficiency`, the ledger's
 * record of its decision), why its support was not enough and what happens
 * next. The block never decides sufficiency itself.
 * Each source can be trusted from here, scoped to this source, its site, or
 * its site for the claim's subject.
 */
export function ClaimSufficiency({
  item,
  programId,
}: {
  item: ClaimReviewItemDto;
  programId: string;
}) {
  const passages = item.evidence.filter(({ passage }) => passage.retrievalKind !== EXECUTION);
  const versionIds = [...new Set(passages.map(({ passage }) => passage.sourceVersionId))];
  const titles = new Map(passages.map(({ passage }) => [passage.sourceVersionId, passage.title]));
  const trust = useSourceTrust(versionIds.length
    ? { programId, sourceIds: [], claim: { statement: item.statement, sourceVersionIds: versionIds }, includeImpact: true }
    : null);
  const reprofile = useReprofileClaim();
  const claim = trust.data?.claim ?? null;
  if (!versionIds.length) return null;
  const waiting = item.state.state === 'open' ? 'machine' : item.state.state === 'queued' ? 'you' : null;
  const headline = claim ? sufficiencyHeadline(claim) : null;
  // The levels the gate weighed, else the levels the sources have now.
  const best = bestOf(item.evidence.flatMap(({ passage }) => (passage.trust ? [passage.trust.level] : [])))
    ?? (claim ? bestLevel(claim) : null);
  const explanation = item.sufficiency && waiting ? insufficiencyExplanation(item.sufficiency, best, waiting) : null;
  const subject = claim?.profile?.subject ?? null;
  const subjectOptions = subject ? [subject.name, ...subject.aliases, ...subject.classes] : [];
  return (
    <section aria-label="Support and its standing">
      <h4 className="text-[10px] font-semibold uppercase tracking-[.14em] text-text-muted">Support</h4>
      {trust.isLoading && <p role="status" className="mt-1 text-xs text-text-muted">Reading the sources’ standing…</p>}
      {trust.error && <p role="alert" className="mt-1 text-xs text-rose-700">Their standing could not be read: {trust.error.message}</p>}
      {claim && (
        <div className="mt-1 space-y-2 text-xs leading-5 text-text-secondary">
          {headline
            ? <p className="text-text-primary">{headline}</p>
            : <p>This claim has not been profiled under your current subject classes, so the bar for it is not known yet.</p>}
          {explanation && <p role="note" className="rounded-lg border border-amber-600/30 p-2 text-text-primary">{explanation}</p>}
          <ul className="space-y-2">
            {claim.supports.map((support) => (
              <li key={support.sourceVersionId} className="space-y-1.5 rounded-lg bg-background p-2">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium text-text-primary">{titles.get(support.sourceVersionId) ?? 'Saved source'}</span>
                  <TrustBadge trust={support.trust} entry={support} titleOf={(id) => titles.get(id) ?? null} />
                </div>
                <TrustOverrideMenu entry={support} programId={programId} sourceTitle={titles.get(support.sourceVersionId) ?? 'this source'} subjectOptions={subjectOptions} />
              </li>
            ))}
          </ul>
          <button
            type="button"
            disabled={reprofile.isPending}
            onClick={() => reprofile.mutate({ programId, claimId: item.claimId })}
            className="min-h-8 rounded-full border border-border px-3 font-medium text-text-primary disabled:opacity-50"
          >
            {reprofile.isPending ? 'Profiling…' : claim.profile ? 'Profile this claim again' : 'Profile this claim'}
          </button>
          {reprofile.error && <p role="alert" className="text-rose-700">{reprofile.error.message}</p>}
        </div>
      )}
    </section>
  );
}
