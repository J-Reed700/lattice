/**
 * The one-line summary above the verification claim lists.
 *
 * Replaces a pair of chips that read "Verified: 4" and "Unverified: 0" — a
 * count of nothing, stated as if it were news. When every claim is grounded the
 * line says so and the unverified side of the panel is not drawn at all.
 *
 * A claim nothing checked (the judge ran out of time, or the page had no
 * archived text) is neither grounded nor ungrounded, so it is counted apart
 * and never folded into "could not be grounded".
 */
export function verificationSummaryLine(
  claimsEvaluated: number,
  unsupportedCount: number,
  unverifiedCount = 0
): string {
  const claims = `${claimsEvaluated} claim${claimsEvaluated !== 1 ? 's' : ''}`;
  const notChecked = unverifiedCount > 0 ? ` · ${unverifiedCount} not checked` : '';
  if (unsupportedCount === 0 && unverifiedCount === 0) {
    return `Every claim is grounded in your sources · ${claims} checked`;
  }
  if (unsupportedCount === 0) {
    const checked = claimsEvaluated - unverifiedCount;
    return `Every checked claim is grounded in your sources · ${checked} of ${claims} checked`;
  }
  return `${unsupportedCount} of ${claims} could not be grounded in your sources${notChecked}`;
}
