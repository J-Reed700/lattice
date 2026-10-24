import { useMemo } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { PassageReferenceDto } from '@/types/api/passageReferences';
import type { UpdatePassageReferenceRequest } from '@/types/api/references';

export const PASSAGE_REFERENCES_QUERY_KEY = ['references', 'passages'] as const;

export function usePassageReferencesQuery() {
  return useQuery<PassageReferenceDto[]>({
    queryKey: PASSAGE_REFERENCES_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.listPassageReferences(200);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 60_000,
    retry: false,
  });
}

export interface PassageReferenceKey {
  documentId: string;
  chunkId: string | null;
}

export function usePassageReferenceIds(): PassageReferenceKey[] {
  const { data } = usePassageReferencesQuery();
  return useMemo(
    () =>
      (data ?? []).map((reference) => ({
        documentId: reference.documentId,
        chunkId: reference.chunkId,
      })),
    [data]
  );
}

export const passageReferencesKey = PASSAGE_REFERENCES_QUERY_KEY;

export function useUpdatePassageReferenceMutation() {
  const queryClient = useQueryClient();
  return useMutation<PassageReferenceDto, Error, UpdatePassageReferenceRequest>({
    mutationFn: async (request) => {
      const result = await VaultAPI.updatePassageReference(request);
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PASSAGE_REFERENCES_QUERY_KEY });
    },
  });
}

export function useDeletePassageReferenceMutation() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: async (id) => {
      const result = await VaultAPI.deletePassageReference(id);
      if (!result.ok) {
        throw new Error(result.error);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PASSAGE_REFERENCES_QUERY_KEY });
    },
  });
}
