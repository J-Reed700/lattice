import { useMemo, useState } from 'react';

import { ShieldAlert, ShieldCheck, ShieldEllipsis, ShieldOff } from 'lucide-react';

import type { MessageVerificationSummary } from '@/types/conversation';

/**
 * What a message reads off its verification summary: the claims sorted by
 * verdict, the counts, the badge, and how much of each list is showing.
 */
export function useMessageVerification({
  verificationSummary,
  isUser,
}: {
  verificationSummary: MessageVerificationSummary | undefined;
  isUser: boolean;
}) {
  const [showAllVerifiedClaims, setShowAllVerifiedClaims] = useState(false);
  const [showAllUnverifiedClaims, setShowAllUnverifiedClaims] = useState(false);

  const claimsEvaluated = verificationSummary?.claimsEvaluated ?? 0;
  const supportedClaims = verificationSummary?.supportedClaimNotes ?? [];
  // Memoized because the `?? []` fallback is a fresh array on every render,
  // which would re-run every hook that reads it.
  const unsupportedClaims = useMemo(
    () => verificationSummary?.unsupportedClaims ?? [],
    [verificationSummary]
  );
  const unsupportedCount = unsupportedClaims.length;
  // "Contradicted" is a stronger claim than "unsupported": a passage the model
  // actually read says otherwise. The backend reports contradictions inside
  // `unsupportedClaims` too, so they are subtracted here rather than counted
  // twice. Absent on messages verified before the judge existed, which is why
  // this reads as an empty list and never as "none were contradicted".
  const contradictedClaims = useMemo(
    () => verificationSummary?.contradictedClaims ?? [],
    [verificationSummary]
  );
  const contradictedCount = contradictedClaims.length;
  const ungroundedClaims = useMemo(() => {
    if (contradictedCount === 0) return unsupportedClaims;
    const contradicted = new Set(contradictedClaims);
    return unsupportedClaims.filter((claim) => !contradicted.has(claim));
  }, [contradictedClaims, contradictedCount, unsupportedClaims]);
  // Claims nothing checked: the judge gave no usable answer or the page had
  // no saved text. Kept apart from the ungrounded ones — "not looked at" is not
  // "not found".
  const uncheckedClaims = useMemo(
    () => (verificationSummary?.claimVerdicts ?? [])
      .filter((verdict) => verdict.verdict === 'unverified')
      .map((verdict) => verdict.sentence),
    [verificationSummary]
  );
  const uncheckedCount = verificationSummary?.verdictCounts?.unverified ?? uncheckedClaims.length;
  const verificationPending = verificationSummary?.enabled === true && verificationSummary.pending === true;
  /** The passage the judge read, keyed by the claim it ruled on. */
  const evidenceByClaim = useMemo(() => {
    const quotes = new Map<string, string>();
    for (const verdict of verificationSummary?.claimVerdicts ?? []) {
      if (verdict.evidenceQuote) quotes.set(verdict.sentence, verdict.evidenceQuote);
    }
    return quotes;
  }, [verificationSummary]);
  const reasonByClaim = useMemo(() => {
    const reasons = new Map<string, string>();
    for (const verdict of verificationSummary?.claimVerdicts ?? []) {
      if (verdict.reason) reasons.set(verdict.sentence, verdict.reason);
    }
    return reasons;
  }, [verificationSummary]);
  const canExpandVerification =
    verificationSummary?.enabled === true && !verificationPending && claimsEvaluated > 0;
  // Drawn on the sentences themselves, so a doubtful figure is doubtful where
  // it is read and not in a list under a badge.
  const claimVerdicts = useMemo(
    () => (verificationSummary?.enabled ? verificationSummary.claimVerdicts ?? [] : []),
    [verificationSummary]
  );

  const visibleVerifiedClaims = showAllVerifiedClaims
    ? supportedClaims
    : supportedClaims.slice(0, 5);
  const visibleUngroundedClaims = showAllUnverifiedClaims
    ? ungroundedClaims
    : ungroundedClaims.slice(0, 5);

  const verificationBadge = useMemo(() => {
    if (isUser || !verificationSummary) return null;
    if (!verificationSummary.enabled) {
      return {
        label: 'Verification off',
        icon: ShieldOff,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-muted))]',
      };
    }
    // The answer is readable while its check runs; say so quietly rather
    // than showing a verdict the check has not reached yet.
    if (verificationPending) {
      return {
        label: 'Checking…',
        icon: ShieldEllipsis,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-muted))]',
      };
    }
    // The check never reported: the app closed under it, or the wait ran
    // out. Say that, rather than "nothing to verify", which would be a lie.
    if (verificationSummary.interrupted) {
      return {
        label: 'Not checked',
        icon: ShieldOff,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-muted))]',
      };
    }
    if (claimsEvaluated === 0) {
      return {
        label: 'Nothing to verify',
        icon: ShieldOff,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-muted))]',
      };
    }
    if (unsupportedCount === 0 && uncheckedCount > 0) {
      return {
        label: `Partly checked · ${uncheckedCount} not checked`,
        icon: ShieldCheck,
        className:
          'border-subtle bg-surface text-[hsl(var(--text-secondary))]',
      };
    }
    if (unsupportedCount === 0) {
      return {
        label: 'Verified',
        icon: ShieldCheck,
        className:
          'border-[hsl(var(--success-muted))] bg-[hsl(var(--success-muted))] text-[hsl(var(--success-fg))]',
      };
    }
    // A contradiction outranks a merely ungrounded claim: the sources were read
    // and they disagree. Folding it into "Partially verified" would report the
    // worse finding as the milder one.
    if (contradictedCount > 0) {
      return {
        label: `Contradicted · ${contradictedCount}`,
        icon: ShieldAlert,
        className:
          'border-[hsl(var(--danger-muted))] bg-[hsl(var(--danger-muted))] text-[hsl(var(--danger-fg))]',
      };
    }
    return {
      label: `Partially verified · ${unsupportedCount}`,
      icon: ShieldAlert,
      className:
        'border-[hsl(var(--warning-muted))] bg-[hsl(var(--warning-muted))] text-[hsl(var(--warning-fg))]',
    };
  }, [
    claimsEvaluated,
    isUser,
    verificationSummary,
    verificationPending,
    unsupportedCount,
    contradictedCount,
    uncheckedCount,
  ]);

  return {
    claimsEvaluated,
    supportedClaims,
    unsupportedCount,
    contradictedClaims,
    ungroundedClaims,
    uncheckedClaims,
    uncheckedCount,
    verificationPending,
    evidenceByClaim,
    reasonByClaim,
    canExpandVerification,
    claimVerdicts,
    showAllVerifiedClaims,
    setShowAllVerifiedClaims,
    showAllUnverifiedClaims,
    setShowAllUnverifiedClaims,
    visibleVerifiedClaims,
    visibleUngroundedClaims,
    verificationBadge,
  };
}
