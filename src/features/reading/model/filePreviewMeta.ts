import type { PassageMatchTier, SourceWithMetadata } from '@/types/conversation';
import { formatFileSize } from '@/utils/files';

type HeaderMetaSource = Pick<SourceWithMetadata, 'category' | 'fileSizeBytes'>;

export function sourceHeaderMeta(source: HeaderMetaSource): string[] {
  const parts: Array<string | undefined> = [
    source.category?.trim() || undefined,
    source.fileSizeBytes > 0 ? formatFileSize(source.fileSizeBytes) : undefined,
  ];
  return parts.filter((part): part is string => Boolean(part));
}

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
