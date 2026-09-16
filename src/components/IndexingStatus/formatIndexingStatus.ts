/**
 * The copy for the indexing rail and its popover, as pure functions.
 *
 * Every string here is the contract — `formatIndexingStatus.test.ts` asserts
 * them verbatim, so change the test and the spec together or not at all.
 */
import type { IndexingSnapshot } from '@/types/api/indexing';

const ACTIVE = new Set(['scanning', 'processing']);

function count(n: number): string {
  return n.toLocaleString();
}

function isActive(snapshot: IndexingSnapshot): boolean {
  return ACTIVE.has(snapshot.status);
}

/** "Indexing 12 of 47" · "Scanning…" · null when there is nothing to say. */
export function formatIndexingLine(snapshot: IndexingSnapshot | undefined): string | null {
  if (!snapshot) return null;

  const { status, paused, processed, totalFiles, failed } = snapshot;

  if (paused) {
    return `Paused · ${count(processed)} of ${count(totalFiles)}`;
  }

  if (status === 'scanning') {
    return 'Scanning…';
  }

  if (status === 'processing') {
    return totalFiles > 0 ? `Indexing ${count(processed)} of ${count(totalFiles)}` : 'Indexing…';
  }

  if (status === 'complete') {
    return failed > 0
      ? `Indexed ${count(processed - failed)} of ${count(totalFiles)} files`
      : `Indexed ${count(totalFiles)} files`;
  }

  if (status === 'cancelled') {
    return `Stopped at ${count(processed)} of ${count(totalFiles)}`;
  }

  if (status === 'error') {
    return 'Indexing stopped';
  }

  return null;
}

/** "1 failed" / "3 failed". Null when nothing failed. */
export function formatFailureLine(snapshot: IndexingSnapshot | undefined): string | null {
  if (!snapshot || snapshot.failed <= 0) return null;
  return `${count(snapshot.failed)} failed`;
}

/** The short label the rail's tooltip and aria-label use. */
export function formatIndexingAriaLabel(snapshot: IndexingSnapshot | undefined): string {
  const line = formatIndexingLine(snapshot);
  const failures = formatFailureLine(snapshot);

  if (line && failures) return `${line}, ${failures}`;
  if (line) return line;
  if (failures) return failures;
  return 'Indexing status';
}

/**
 * True when the rail button should render at all. An idle vault with no
 * failures shows nothing — same discipline as `HeaderDownloadsIndicator`.
 */
export function shouldShowIndexingRail(snapshot: IndexingSnapshot | undefined): boolean {
  if (!snapshot) return false;
  return isActive(snapshot) || snapshot.paused || snapshot.failures.length > 0;
}

/** True while the rail icon should spin: a run genuinely in flight. */
export function shouldSpin(snapshot: IndexingSnapshot | undefined): boolean {
  if (!snapshot) return false;
  return isActive(snapshot) && !snapshot.paused;
}
