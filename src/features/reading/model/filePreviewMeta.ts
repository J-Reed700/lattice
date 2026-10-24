import type { PassageMatchTier, SourceWithMetadata } from '@/types/conversation';
import { formatFileSize } from '@/utils/files';

/**
 * The two honest lines in the file preview: what we know about the file, and
 * how well we found the passage inside it.
 *
 * Both exist as functions rather than inline JSX because both are claims about
 * the world, and a claim is worth a test.
 */

type HeaderMetaSource = Pick<SourceWithMetadata, 'category' | 'fileSizeBytes'>;

/**
 * The muted line under the file name: category, then size.
 *
 * A citation built from a passage reference or a compare cell carries neither
 * value. `formatFileSize(0)` renders "0.0 B", which is not "unknown" — it is a
 * measurement we never took. An unknown part is therefore left out entirely,
 * and a source with nothing to say gets no line at all.
 */
export function sourceHeaderMeta(source: HeaderMetaSource): string[] {
  const parts: Array<string | undefined> = [
    source.category?.trim() || undefined,
    source.fileSizeBytes > 0 ? formatFileSize(source.fileSizeBytes) : undefined,
  ];
  return parts.filter((part): part is string => Boolean(part));
}

/**
 * What to say when the highlight may not be where the passage really is.
 *
 * `null` for an exact match — there is nothing to admit — and `null` when the
 * modal was opened on a whole file rather than a citation.
 */
export function passageMatchNotice(
  matchTier: PassageMatchTier,
  hasLocator: boolean
): string | null {
  if (!hasLocator) return null;
  if (matchTier === 'approximate') {
    return 'Approximate position — the file changed since it was indexed.';
  }
  if (matchTier === 'none') return "Couldn't find this passage in the file.";
  return null;
}
