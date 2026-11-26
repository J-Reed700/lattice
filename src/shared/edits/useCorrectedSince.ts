import { useQuery } from '@tanstack/react-query';

import { contentEditKeys, listContentEdits } from './api';

import type { ContentEditQuery, ContentRef } from './types';

export function useCorrectedSince(content: ContentRef | null, since: number | null): boolean {
  const enabled = content !== null && since !== null;
  const query: ContentEditQuery = {
    scope: null,
    contents: content ? [content] : [],
    since,
  };
  const { data } = useQuery({
    queryKey: contentEditKeys.list(query),
    queryFn: () => listContentEdits(query),
    enabled,
  });
  return enabled && (data?.length ?? 0) > 0;
}
