import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ApiProvider, type ApiClient } from '@/contexts/ApiContext';
import { formItem, quizForm } from '@/features/learning/quizzes/quizTestData';
import { useQuizAutosave } from '@/features/learning/quizzes/useQuizAutosave';

const first = formItem('short_answer', { id: 'first' });
const form = quizForm({ items: [first] });

function setup(failure: Record<string, unknown>) {
  const save = vi.fn(async () => failure);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>
      <ApiProvider client={{ saveLearningAssessmentResponse: save } as unknown as ApiClient}>{children}</ApiProvider>
    </QueryClientProvider>
  );
  return renderHook(() => useQuizAutosave(form, form.programId, vi.fn(async () => form)), { wrapper });
}

describe('quiz autosave through the transport', () => {
  it('sees a CONFLICT-coded refusal returned by the api as a conflict', async () => {
    const { result } = setup({ ok: false, error: 'Stale.', details: { code: 'CONFLICT', message: 'Stale.' } });
    await act(async () => {
      result.current.update(first.id, { textResponse: 'An answer.' });
      await result.current.flush();
    });
    expect(result.current.conflict).toBe(true);
  });

  it('does not take a refusal for a conflict because of its wording', async () => {
    const { result } = setup({ ok: false, error: 'The answers changed elsewhere; reload the revision.' });
    await act(async () => {
      result.current.update(first.id, { textResponse: 'An answer.' });
      await result.current.flush();
    });
    expect(result.current.conflict).toBe(false);
  });
});
