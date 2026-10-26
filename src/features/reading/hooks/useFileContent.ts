import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';

import { sourceTextQueryOptions } from './readSourceFile';


export function useFileContent(filePath: string | undefined, enabled = true) {
  const api = useApi();
  const active = Boolean(filePath) && enabled;
  const query = useQuery({ ...sourceTextQueryOptions(filePath ?? '', api), enabled: active });

  if (!active) return { content: '', isLoading: false, error: null };
  return {
    content: query.data ?? '',
    isLoading: query.isPending,
    error: query.isError ? (query.error instanceof Error ? query.error.message : 'Failed to load file') : null,
  };
}
