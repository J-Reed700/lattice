import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { readingKeys, readSavedWebArticle } from '@/features/reading/hooks/readSourceFile';
import type { SavedWebArticleDto } from '@/lib/bindings';
import type { SourceWithMetadata } from '@/types/conversation';
import { isCourseSource, isHttpUrl } from '@/utils/sourcePreview';

/**
 * The saved web article the source's file is, as the web archive says, or null
 * when it is not one. `deciding` holds while the archive has not answered yet,
 * so the reader shows nothing rather than the wrong view first. `path` is the
 * file to ask about; while it is undefined nothing is sent and it is deciding.
 */
export function useSavedWebArticle(
  source: SourceWithMetadata,
  path: string | undefined,
): {
  article: SavedWebArticleDto | null;
  deciding: boolean;
} {
  const api = useApi();
  const filePath = path ?? '';
  const enabled =
    path !== undefined && filePath !== '' && !isHttpUrl(filePath) && !isCourseSource(source);
  const query = useQuery({
    queryKey: readingKeys.savedWebArticle(filePath),
    queryFn: async () => {
      const result = await readSavedWebArticle(filePath, api);
      return result.ok ? result.data : null;
    },
    enabled,
    retry: false,
    staleTime: 0,
  });

  return {
    article: query.data ?? null,
    deciding: path === undefined || (enabled && query.isPending),
  };
}
