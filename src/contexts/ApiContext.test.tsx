import { memo, type ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, renderHook, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { fetchConversationList } from '@/features/chat/api/conversationQueryData';
import { useLessonClaimReview } from '@/features/learning/lessons/review/useLessonClaimReview';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import VaultAPI from '@/lib/api';
import { createTestApi } from '@/tests/apiClient';
import { makeAppSettings } from '@/tests/fixtures/appSettings';

import { ApiProvider, useApi, type ApiClient } from './ApiContext';

function wrapperFor(api: ApiClient) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <ApiProvider client={api}>
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      </ApiProvider>
    );
  };
}

describe('ApiProvider', () => {
  it('uses the desktop adapter for a standalone consumer', () => {
    const { result } = renderHook(useApi);
    expect(result.current).toBe(VaultAPI);
  });

  it('uses the nearest provider and keeps its identity on unrelated renders', () => {
    const outer = createTestApi({});
    const inner = createTestApi({});
    const consume = vi.fn();
    const Consumer = memo(() => {
      consume(useApi());
      return <p>Consumer</p>;
    });
    const tree = (label: string) => (
      <ApiProvider client={outer}>
        <p>{label}</p>
        <ApiProvider client={inner}><Consumer /></ApiProvider>
      </ApiProvider>
    );
    const view = render(tree('First'));
    view.rerender(tree('Second'));
    expect(screen.getByText('Second')).toBeInTheDocument();
    expect(consume).toHaveBeenCalledExactlyOnceWith(inner);
  });

  it('keeps two trees with different APIs and query caches independent', async () => {
    const firstSettings = makeAppSettings();
    const secondSettings = { ...makeAppSettings(), ui: { ...firstSettings.ui, theme: 'dark' as const } };
    const firstApi = createTestApi({ getSettings: vi.fn().mockResolvedValue({ ok: true, data: firstSettings }) });
    const secondApi = createTestApi({ getSettings: vi.fn().mockResolvedValue({ ok: true, data: secondSettings }) });
    const first = renderHook(useSettingsQuery, { wrapper: wrapperFor(firstApi) });
    const second = renderHook(useSettingsQuery, { wrapper: wrapperFor(secondApi) });
    await waitFor(() => {
      expect(first.result.current.data).toEqual(firstSettings);
      expect(second.result.current.data).toEqual(secondSettings);
    });
    expect(firstApi.getSettings).toHaveBeenCalledTimes(1);
    expect(secondApi.getSettings).toHaveBeenCalledTimes(1);
  });

  it('uses the injected API for a mutation and its invalidated read', async () => {
    let settings = makeAppSettings();
    const getSettings = vi.fn(async () => ({ ok: true as const, data: settings }));
    const updateSettings = vi.fn(async () => {
      settings = { ...settings, ui: { ...settings.ui, theme: 'dark' } };
      return { ok: true as const, data: settings };
    });
    const api = createTestApi({ getSettings, updateSettings });
    const { result } = renderHook(() => ({ query: useSettingsQuery(), mutation: useUpdateSettingsMutation() }), { wrapper: wrapperFor(api) });
    await waitFor(() => expect(result.current.query.isSuccess).toBe(true));
    await act(async () => { await result.current.mutation.mutateAsync({ category: 'ui', updates: { theme: 'dark' } }); });
    await waitFor(() => expect(getSettings).toHaveBeenCalledTimes(2));
    expect(updateSettings).toHaveBeenCalledExactlyOnceWith({ category: 'ui', updates: { theme: 'dark' } });
    expect(result.current.query.data?.ui.theme).toBe('dark');
  });

  it('passes a supplied client through non-React query helpers', async () => {
    const listConversationsExplorer = vi.fn().mockResolvedValue({ ok: true, data: { conversations: [], total: 0 } });
    const api = createTestApi({ listConversationsExplorer });
    await expect(fetchConversationList({ spaceId: null, filterMode: 'all', searchQuery: '' }, api)).resolves.toEqual([]);
    expect(listConversationsExplorer).toHaveBeenCalledTimes(1);
  });

  it('refreshes an open claim review when the Learning checker changes', async () => {
    let settings = makeAppSettings();
    const getLearningLessonClaimReview = vi.fn().mockResolvedValue({ ok: true, data: null });
    const api = createTestApi({
      getSettings: vi.fn(async () => ({ ok: true as const, data: settings })),
      getLearningLessonClaimReview,
      updateSettings: vi.fn(async () => {
        settings = { ...settings, llm: { ...settings.llm, learningModel: { kind: 'cloud', provider: 'anthropic', model: 'new-checker' } } };
        return { ok: true as const, data: settings };
      }),
    });
    const { result } = renderHook(() => ({
      settings: useSettingsQuery(),
      review: useLessonClaimReview('course', 'lesson'),
      mutation: useUpdateSettingsMutation(),
    }), { wrapper: wrapperFor(api) });
    await waitFor(() => expect(result.current.settings.isSuccess && result.current.review.isSuccess).toBe(true));
    expect(getLearningLessonClaimReview).toHaveBeenCalledTimes(1);
    await act(async () => { await result.current.mutation.mutateAsync({ category: 'llm', updates: { learningModel: { kind: 'cloud', provider: 'anthropic', model: 'new-checker' } } }); });
    await waitFor(() => expect(getLearningLessonClaimReview).toHaveBeenCalledTimes(2));
  });
});
