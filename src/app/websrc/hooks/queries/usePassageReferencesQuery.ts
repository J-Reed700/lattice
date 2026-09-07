import { useMemo } from 'react';

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { PassageReferenceDto } from '@/types/api/passageReferences';
import type { UpdatePassageReferenceRequest } from '@/types/api/references';

/** Contract §4.3 — shared with the ReferenceInbox. */
export const PASSAGE_REFERENCES_QUERY_KEY = ['references', 'passages'] as const;

/**
 * Saved passage references.
 *
 * The query surfaces its own failure (`isError`) so a reader looking at the
 * inbox is told the list could not be loaded, rather than being shown an
 * empty shelf that reads as "you have saved nothing".
 */
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

/** The `{ documentId, chunkId }` pairs a citation mark is matched against. */
export interface PassageReferenceKey {
  documentId: string;
  chunkId: string | null;
}

/**
 * Just the keys behind the "from your references" mark.
 *
 * Degrades to an empty list on any failure: the mark is an extra, and its
 * absence says nothing false — unlike the inbox, which must admit the error.
 */
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

/** Alias for the same key, spelled as GROUND-RULES §4.3 writes it. */
export const passageReferencesKey = PASSAGE_REFERENCES_QUERY_KEY;

/**
 * Annotation-only update. The repository is the SSOT, so success invalidates
 * rather than patching a local copy (CLAUDE.md rule 3).
 */
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
