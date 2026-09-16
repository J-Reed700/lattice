import { useMutation, useMutationState, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { GenerateConversationStudyDeckRequestDto, GenerateStudyDeckRequestDto, ReviewStudyCardRequestDto, UpdateStudyCardRequestDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import type { ApiResult } from '@/types';

const key = ['study'] as const;
const generationKey = [...key, 'generate'] as const;
const documentGenerationKey = [...generationKey, 'documents'] as const;
const conversationGenerationKey = [...generationKey, 'conversation'] as const;
type StudyGeneration = {
  id: number;
  status: 'pending' | 'error';
  error?: string;
} & ({ kind: 'documents'; request: GenerateStudyDeckRequestDto } | { kind: 'conversation'; request: GenerateConversationStudyDeckRequestDto });
async function data<T>(result: Promise<ApiResult<T>>): Promise<T> {
  const response = await result;
  if (!response.ok) throw new Error(typeof response.details?.details === 'string' ? response.details.details : response.error);
  return response.data;
}
export function useStudyDecks() {
  return useQuery({ queryKey: [...key, 'decks'], queryFn: () => data(VaultAPI.listStudyDecks()), refetchInterval: 60_000 });
}
export function useStudyDeck(id: string | null) {
  return useQuery({ queryKey: [...key, 'deck', id], queryFn: () => data(VaultAPI.getStudyDeck(id!)), enabled: Boolean(id) });
}
export function useStudyDocuments() {
  return useQuery({ queryKey: ['documents', 'study'], queryFn: () => data(VaultAPI.listAllDocuments(10000)), refetchInterval: 15_000 });
}
export function useGenerateStudyDeck() {
  const client = useQueryClient();
  return useMutation({
    mutationKey: documentGenerationKey,
    gcTime: Infinity,
    mutationFn: (request: GenerateStudyDeckRequestDto) => data(VaultAPI.generateStudyDeck(request)),
    onSuccess: (deck) => { client.setQueryData([...key, 'deck', deck.id], deck); void client.invalidateQueries({ queryKey: key }); toast.success(`${deck.title} is ready`, { message: 'Saved in Study → Your decks.' }); },
  });
}
export function useGenerateConversationStudyDeck() {
  const client = useQueryClient();
  return useMutation({
    mutationKey: conversationGenerationKey,
    gcTime: Infinity,
    mutationFn: (request: GenerateConversationStudyDeckRequestDto) => data(VaultAPI.generateConversationStudyDeck(request)),
    onSuccess: (deck) => { client.setQueryData([...key, 'deck', deck.id], deck); void client.invalidateQueries({ queryKey: key }); toast.success(`${deck.title} is ready`, { message: 'Every verified claim and its citations are saved in Study.' }); },
  });
}
// The mutation cache belongs to the app, so navigating away does not lose an
// in-flight request or its failure. Saved decks remain canonical in the backend.
export function useStudyGenerations() {
  return useMutationState({
    filters: { mutationKey: generationKey, predicate: mutation => mutation.state.status === 'pending' || mutation.state.status === 'error' },
    select: (mutation): StudyGeneration => {
      const request = mutation.state.variables as GenerateStudyDeckRequestDto | GenerateConversationStudyDeckRequestDto;
      const shared = { id: mutation.mutationId, status: mutation.state.status as 'pending' | 'error', error: mutation.state.error?.message };
      return 'conversationId' in request
        ? { ...shared, request, kind: 'conversation' }
        : { ...shared, request, kind: 'documents' };
    },
  });
}
export function useDismissStudyGeneration() {
  const client = useQueryClient();
  return (id: number) => {
    const cache = client.getMutationCache();
    const mutation = cache.getAll().find(item => item.mutationId === id);
    if (mutation && mutation.state.status !== 'pending') cache.remove(mutation);
  };
}
export function useReviewStudyCard() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (request: ReviewStudyCardRequestDto) => data(VaultAPI.reviewStudyCard(request)), onSuccess: () => client.invalidateQueries({ queryKey: key }) });
}
export function useUpdateStudyCard() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (request: UpdateStudyCardRequestDto) => data(VaultAPI.updateStudyCard(request)), onSuccess: () => client.invalidateQueries({ queryKey: key }) });
}
export function useDeleteStudyDeck() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (id: string) => data(VaultAPI.deleteStudyDeck(id)), onSuccess: () => client.invalidateQueries({ queryKey: key }) });
}
