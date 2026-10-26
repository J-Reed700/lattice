import { type QueryClient, useMutation, useMutationState, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import type { GenerateConversationStudyDeckRequestDto, GenerateStudyDeckRequestDto, ReviewStudyCardRequestDto, UpdateStudyCardRequestDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import type { ApiResult } from '@/types';

const key = ['study'] as const;
const generationKey = [...key, 'generate'] as const;
const documentGenerationKey = [...generationKey, 'documents'] as const;
const conversationGenerationKey = [...generationKey, 'conversation'] as const;
type FlashcardGeneration = {
  id: number;
  status: 'pending' | 'error';
  error?: string;
} & ({ kind: 'documents'; request: GenerateStudyDeckRequestDto } | { kind: 'conversation'; request: GenerateConversationStudyDeckRequestDto });
async function data<T>(result: Promise<ApiResult<T>>): Promise<T> {
  const response = await result;
  if (!response.ok) throw new Error(typeof response.details?.details === 'string' ? response.details.details : response.error);
  return response.data;
}
// A program's recall deck is listed here too, so a review or an edit made in
// Flashcards must also refresh that program's Recall tab.
function invalidateDecks(client: QueryClient) {
  return Promise.all([client.invalidateQueries({ queryKey: key }), client.invalidateQueries({ queryKey: ['learning-memory'] })]);
}
export function useFlashcardDecks() {
  const api = useApi();
  return useQuery({ queryKey: [...key, 'decks'], queryFn: () => data(api.listStudyDecks()), refetchInterval: 60_000 });
}
export function useFlashcardDeck(id: string | null) {
  const api = useApi();
  return useQuery({ queryKey: [...key, 'deck', id], queryFn: () => data(api.getStudyDeck(id!)), enabled: Boolean(id) });
}
export function useFlashcardDocuments() {
  const api = useApi();
  return useQuery({ queryKey: ['documents', 'study'], queryFn: () => data(api.listAllDocuments(10000)), refetchInterval: 15_000 });
}
export function useGenerateFlashcardDeck() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationKey: documentGenerationKey,
    gcTime: Infinity,
    mutationFn: (request: GenerateStudyDeckRequestDto) => data(api.generateStudyDeck(request)),
    onSuccess: (deck) => { client.setQueryData([...key, 'deck', deck.id], deck); void client.invalidateQueries({ queryKey: key }); toast.success(`${deck.title} is ready`, { message: 'Saved in Studio → Flashcards.' }); },
  });
}
export function useGenerateConversationFlashcardDeck() {
  const api = useApi();
  const client = useQueryClient();
  const navigate = useAppNavigate();
  return useMutation({
    mutationKey: conversationGenerationKey,
    gcTime: Infinity,
    mutationFn: (request: GenerateConversationStudyDeckRequestDto) => data(api.generateConversationStudyDeck(request)),
    onSuccess: (deck) => { client.setQueryData([...key, 'deck', deck.id], deck); void client.invalidateQueries({ queryKey: key }); toast.success(`${deck.title} is ready`, { message: 'Every verified claim and its citations are saved in Studio → Flashcards.', action: { label: 'Open deck', onClick: () => { void navigate(`/studio?deck=${encodeURIComponent(deck.id)}`); } } }); },
  });
}
// The mutation cache belongs to the app, so navigating away does not lose an
// in-flight request or its failure. Saved decks remain canonical in the backend.
export function useFlashcardGenerations() {
  return useMutationState({
    filters: { mutationKey: generationKey, predicate: mutation => mutation.state.status === 'pending' || mutation.state.status === 'error' },
    select: (mutation): FlashcardGeneration => {
      const request = mutation.state.variables as GenerateStudyDeckRequestDto | GenerateConversationStudyDeckRequestDto;
      const shared = { id: mutation.mutationId, status: mutation.state.status as 'pending' | 'error', error: mutation.state.error?.message };
      return 'conversationId' in request
        ? { ...shared, request, kind: 'conversation' }
        : { ...shared, request, kind: 'documents' };
    },
  });
}
export function useDismissFlashcardGeneration() {
  const client = useQueryClient();
  return (id: number) => {
    const cache = client.getMutationCache();
    const mutation = cache.getAll().find(item => item.mutationId === id);
    if (mutation && mutation.state.status !== 'pending') cache.remove(mutation);
  };
}
export function useReviewFlashcard() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({ mutationFn: (request: ReviewStudyCardRequestDto) => data(api.reviewStudyCard(request)), onSuccess: () => invalidateDecks(client) });
}
export function useUpdateFlashcard() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({ mutationFn: (request: UpdateStudyCardRequestDto) => data(api.updateStudyCard(request)), onSuccess: () => invalidateDecks(client) });
}
export function useDeleteFlashcardDeck() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({ mutationFn: (id: string) => data(api.deleteStudyDeck(id)), onSuccess: () => invalidateDecks(client) });
}
