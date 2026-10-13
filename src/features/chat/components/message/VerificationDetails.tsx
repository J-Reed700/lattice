import { verificationSummaryLine } from '@/features/chat/model/verificationSummary';

import type { useMessageVerification } from './useMessageVerification';

/**
 * One entry per checked sentence, separated by a rule.
 *
 * Claims are printed as the model wrote them, citation markers included, so an
 * entry routinely ends in `[24]`. Spacing alone left that marker sitting
 * directly above the next claim's first word, where it read as that claim's
 * citation — and an uncited sentence looked cited.
 */
const CLAIM_LIST_CLASS = 'divide-y divide-border-subtle';
const CLAIM_ITEM_CLASS =
  'py-2 first:pt-0 last:pb-0 text-sm leading-relaxed text-[hsl(var(--text-secondary))]';

/** The expanded verification panel: contradicted, verified, ungrounded and unchecked claims. */
export function VerificationDetails({
  verificationPanelId,
  verification,
}: {
  verificationPanelId: string;
  verification: ReturnType<typeof useMessageVerification>;
}) {
  const {
    claimsEvaluated,
    unsupportedCount,
    uncheckedCount,
    contradictedClaims,
    reasonByClaim,
    evidenceByClaim,
    supportedClaims,
    visibleVerifiedClaims,
    showAllVerifiedClaims,
    setShowAllVerifiedClaims,
    ungroundedClaims,
    visibleUngroundedClaims,
    showAllUnverifiedClaims,
    setShowAllUnverifiedClaims,
    uncheckedClaims,
  } = verification;
  return (
    <div id={verificationPanelId} role="region" aria-label="Verification details" className="mb-4 rounded-sm border border-subtle bg-surface p-4">
      <p className="mb-1 text-sm font-medium text-text-primary">Verification details</p>
      <p className="text-xs text-[hsl(var(--text-muted))]">
        {verificationSummaryLine(claimsEvaluated, unsupportedCount, uncheckedCount)}
      </p>

      <div className="mt-3 grid gap-4 lg:grid-cols-2">
        {contradictedClaims.length > 0 && (
          <div>
            <p className="mb-2 text-xs font-medium text-[hsl(var(--danger-fg))]">
              Contradicted by your sources
            </p>
            {/* Ruled, not just spaced. A claim ends in its own citation
                marker, so with nothing but a gap between entries the
                `[24]` closing one sentence reads as though it belongs to
                the sentence printed underneath it. */}
            <ul className={CLAIM_LIST_CLASS}>
              {contradictedClaims.map((claim, idx) => (
                <li key={`contradicted-${idx}`} className={CLAIM_ITEM_CLASS}>
                  {claim}
                  {reasonByClaim.get(claim) && (
                    <span className="mt-1 block text-xs leading-relaxed text-[hsl(var(--text-secondary))]">
                      {reasonByClaim.get(claim)}
                    </span>
                  )}
                  {/* The passage the judge read. Without it the verdict is
                      an assertion; with it the reader can check. */}
                  {evidenceByClaim.get(claim) && (
                    <span className="mt-1 block border-l-2 border-[hsl(var(--danger-muted))] pl-2 text-xs italic text-[hsl(var(--text-muted))]">
                      <span className="not-italic font-medium">What the source says: </span>
                      &ldquo;{evidenceByClaim.get(claim)}&rdquo;
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        )}

        {supportedClaims.length > 0 && (
          <div>
            <p className="mb-2 text-xs font-medium text-[hsl(var(--text-secondary))]">
              Verified claims
            </p>
            <ul className={CLAIM_LIST_CLASS}>
              {visibleVerifiedClaims.map((claim, idx) => (
                <li key={`verified-${idx}`} className={CLAIM_ITEM_CLASS}>
                  {claim}
                </li>
              ))}
            </ul>
            {supportedClaims.length > 5 && (
              <button
                type="button"
                onClick={() => setShowAllVerifiedClaims((prev) => !prev)}
                className="mt-2 text-xs text-[hsl(var(--accent))] underline-offset-2 hover:underline"
              >
                {showAllVerifiedClaims
                  ? 'Show fewer'
                  : `Show all ${supportedClaims.length}`}
              </button>
            )}
          </div>
        )}

        {ungroundedClaims.length > 0 && (
          <div>
            <p className="mb-2 text-xs font-medium text-[hsl(var(--text-secondary))]">
              Not found in your sources
            </p>
            <ul className={CLAIM_LIST_CLASS}>
              {visibleUngroundedClaims.map((claim, idx) => (
                <li key={`unsupported-${idx}`} className={CLAIM_ITEM_CLASS}>
                  {claim}
                </li>
              ))}
            </ul>
            {ungroundedClaims.length > 5 && (
              <button
                type="button"
                onClick={() => setShowAllUnverifiedClaims((prev) => !prev)}
                className="mt-2 text-xs text-[hsl(var(--accent))] underline-offset-2 hover:underline"
              >
                {showAllUnverifiedClaims
                  ? 'Show fewer'
                  : `Show all ${ungroundedClaims.length}`}
              </button>
            )}
          </div>
        )}

        {uncheckedClaims.length > 0 && (
          <div>
            <p className="mb-2 text-xs font-medium text-[hsl(var(--text-muted))]">
              Not checked
            </p>
            <ul className={CLAIM_LIST_CLASS}>
                {uncheckedClaims.map((claim, idx) => (
                  <li key={`unchecked-${idx}`} className={CLAIM_ITEM_CLASS}>
                    {claim}
                    {reasonByClaim.get(claim) && (
                      <span className="mt-1 block text-xs leading-relaxed text-[hsl(var(--text-secondary))]">
                        {reasonByClaim.get(claim)}
                      </span>
                    )}
                  </li>
                ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
