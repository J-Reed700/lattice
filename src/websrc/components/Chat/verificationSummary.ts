/**
 * The one-line summary above the verification claim lists.
 *
 * Replaces a pair of chips that read "Verified: 4" and "Unverified: 0" — a
 * count of nothing, stated as if it were news. When every claim is grounded the
 * line says so and the unverified side of the panel is not drawn at all.
 */
export function verificationSummaryLine(
  claimsEvaluated: number,
  unsupportedCount: number
): string {
  const claims = `${claimsEvaluated} claim${claimsEvaluated !== 1 ? 's' : ''}`;
  if (unsupportedCount === 0) {
    return `Every claim is grounded in your sources · ${claims} checked`;
  }
  return `${unsupportedCount} of ${claims} could not be grounded in your sources`;
}
