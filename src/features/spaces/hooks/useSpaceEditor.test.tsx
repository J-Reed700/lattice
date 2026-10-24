import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { ConversationSnapshotProvider } from '@/shared/conversations/conversationsStore';
import type { ConversationsState } from '@/shared/conversations/conversationsStore.types';
import { toast } from '@/stores/toastStore';
import { deferred } from '@/tests/deferred';
import type { ConversationSpaceDto } from '@/types';

import { useSpaceEditor } from './useSpaceEditor';

const api = vi.hoisted(() => ({
  createConversationSpace: vi.fn(),
  updateConversationSpace: vi.fn(),
  archiveConversationSpace: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

const space = (overrides: Partial<ConversationSpaceDto>): ConversationSpaceDto => ({
  id: 'botany', name: 'Botany', description: null, icon: null, accentColor: null, spacePrompt: null, toolPreferencesJson: null,
  isArchived: false, ...overrides,
} as ConversationSpaceDto);

const botany = space({
  id: 'botany', name: 'Botany', description: 'Plants', icon: '🌿', accentColor: '#3A7', spacePrompt: 'Cite field guides.',
  toolPreferencesJson: JSON.stringify({ knowledge_base: true, web_search: true, deep_research_mode: false }),
});
const general = space({ id: GENERAL_SPACE_ID, name: 'General' });

function setup(selectedSpaceId: string | null, spaces = [general, botany]) {
  const store = { spaces, selectedSpaceId, setSelectedSpace: vi.fn(), loadConversations: vi.fn().mockResolvedValue(undefined) };
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>
      <ConversationSnapshotProvider value={store as unknown as ConversationsState}>{children}</ConversationSnapshotProvider>
    </QueryClientProvider>
  );
  return { store, ...renderHook(() => useSpaceEditor(), { wrapper }) };
}

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset();
  api.createConversationSpace.mockResolvedValue({ ok: true, data: { id: 'new-space' } });
  api.updateConversationSpace.mockResolvedValue({ ok: true, data: botany });
  api.archiveConversationSpace.mockResolvedValue({ ok: true, data: null });
  vi.spyOn(toast, 'success');
  vi.spyOn(toast, 'error');
  vi.spyOn(toast, 'warning');
});

afterEach(() => vi.restoreAllMocks());

describe('useSpaceEditor', () => {
  it('fills the drafts from the selected space, reading legacy tool keys', () => {
    const { result } = setup('botany');
    expect(result.current).toMatchObject({
      spaceNameDraft: 'Botany', spaceDescriptionDraft: 'Plants', spaceIconDraft: '🌿', spaceAccentDraft: '#3A7',
      spacePromptDraft: 'Cite field guides.', spaceKbDefault: true, spaceWebDefault: true, spaceDeepResearchDefault: false,
    });
  });

  it('starts empty with no space selected, and does nothing on save or archive', async () => {
    const { result } = setup(null);
    expect(result.current).toMatchObject({ spaceNameDraft: '', spaceIconDraft: '', spaceKbDefault: false, spaceWebDefault: false });
    await act(() => result.current.saveSpaceEnvironment());
    await act(() => result.current.setSelectedSpaceArchived(true));
    expect(api.updateConversationSpace).not.toHaveBeenCalled();
    expect(api.archiveConversationSpace).not.toHaveBeenCalled();
  });

  it('treats a space with no saved tool preferences as all off', () => {
    const { result } = setup('plain', [space({ id: 'plain', name: 'Plain' })]);
    expect(result.current).toMatchObject({ spaceNameDraft: 'Plain', spaceDescriptionDraft: '', spaceKbDefault: false, spaceDeepResearchDefault: false });
  });

  it('creates a uniquely named space, selects it and opens its editor', async () => {
    const { result, store } = setup('botany');
    let created = false;
    await act(async () => { created = await result.current.createSpace(' botany '); });
    expect(created).toBe(true);
    expect(api.createConversationSpace).toHaveBeenCalledWith({
      name: 'botany 2', description: null, icon: null, accentColor: null, spacePrompt: null,
      toolPreferencesJson: JSON.stringify({ knowledgeBase: false, webSearch: false, deepResearchMode: false, enabledTools: [] }),
    });
    expect(store.setSelectedSpace).toHaveBeenCalledWith('new-space');
    expect(store.loadConversations).toHaveBeenCalledWith({ spaceId: 'new-space' });
    expect(result.current.isSpaceEditorOpen).toBe(true);
    expect(toast.success).toHaveBeenCalledWith('Space created', { message: 'botany 2' });
  });

  it('names a blank request after the next free number and reports a failed create', async () => {
    api.createConversationSpace.mockResolvedValue({ ok: false, error: 'Database is locked' });
    const { result, store } = setup(null);
    let created = true;
    await act(async () => { created = await result.current.createSpace('  '); });
    expect(created).toBe(false);
    expect(api.createConversationSpace).toHaveBeenCalledWith(expect.objectContaining({ name: 'Space 3' }));
    expect(store.setSelectedSpace).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith('Could not update space', { message: expect.stringContaining('Database is locked') });
  });

  it('saves trimmed drafts, falling back to the old name and dropping blanks and bad colours', async () => {
    const { result } = setup('botany');
    act(() => {
      result.current.setSpaceNameDraft('   ');
      result.current.setSpaceDescriptionDraft('  ');
      result.current.setSpaceIconDraft(' 🌵 ');
      result.current.setSpaceAccentDraft('not-a-colour');
      result.current.setSpacePromptDraft('  Prefer herbarium records.  ');
      result.current.setSpaceKbDefault(false);
      result.current.setSpaceWebDefault(false);
    });
    await act(() => result.current.saveSpaceEnvironment());
    expect(api.updateConversationSpace).toHaveBeenCalledWith({
      spaceId: 'botany', name: 'Botany', description: null, icon: '🌵', accentColor: null, spacePrompt: 'Prefer herbarium records.',
      toolPreferencesJson: JSON.stringify({ knowledgeBase: false, webSearch: false, deepResearchMode: false, enabledTools: [] }),
    });
  });

  it('saves a full set of drafts as given', async () => {
    const { result } = setup('botany');
    act(() => {
      result.current.setSpaceNameDraft(' Plant science ');
      result.current.setSpaceDescriptionDraft(' Leaves and roots ');
      result.current.setSpaceIconDraft('  ');
      result.current.setSpacePromptDraft('');
      result.current.setSpaceAccentDraft('ABC');
    });
    await act(() => result.current.saveSpaceEnvironment());
    expect(api.updateConversationSpace).toHaveBeenCalledWith(expect.objectContaining({
      name: 'Plant science', description: 'Leaves and roots', icon: null, spacePrompt: null, accentColor: '#abc',
      toolPreferencesJson: JSON.stringify({ knowledgeBase: true, webSearch: true, deepResearchMode: false, enabledTools: ['web_search', 'fetch_url_content'] }),
    }));
  });

  it('reports a failed save', async () => {
    api.updateConversationSpace.mockRejectedValue(new Error('offline'));
    const { result } = setup('botany');
    await act(() => result.current.saveSpaceEnvironment());
    expect(toast.error).toHaveBeenCalledWith('Could not update space', { message: expect.stringContaining('offline') });
  });

  it('archives the selected space, closes the editor and returns to all chats', async () => {
    const { result, store } = setup('botany');
    act(() => result.current.setIsSpaceEditorOpen(true));
    await act(() => result.current.setSelectedSpaceArchived(true));
    expect(api.archiveConversationSpace).toHaveBeenCalledWith({ spaceId: 'botany', archived: true });
    expect(result.current.isSpaceEditorOpen).toBe(false);
    expect(store.setSelectedSpace).toHaveBeenCalledWith(null);
    expect(store.loadConversations).toHaveBeenCalledWith({ spaceId: null });
  });

  it('unarchives the selected space without leaving it', async () => {
    const { result, store } = setup('botany');
    await act(() => result.current.setSelectedSpaceArchived(false));
    expect(api.archiveConversationSpace).toHaveBeenCalledWith({ spaceId: 'botany', archived: false });
    expect(store.setSelectedSpace).not.toHaveBeenCalled();
  });

  it('never archives the General space', async () => {
    const { result } = setup(GENERAL_SPACE_ID);
    await act(() => result.current.setSelectedSpaceArchived(true));
    expect(api.archiveConversationSpace).not.toHaveBeenCalled();
  });

  it('reports a failed archive or restore', async () => {
    api.archiveConversationSpace.mockResolvedValue({ ok: false, error: 'busy' });
    const { result, store } = setup('botany');
    await act(() => result.current.setSelectedSpaceArchived(true));
    expect(store.setSelectedSpace).not.toHaveBeenCalled();
    await act(() => result.current.restoreArchivedSpace('old'));
    expect(toast.error).toHaveBeenCalledTimes(2);
  });

  it('tells archiving the selected space apart from restoring another', async () => {
    const pending = deferred<{ ok: true; data: null }>();
    api.archiveConversationSpace.mockReturnValue(pending.promise);
    const { result } = setup('botany');
    act(() => { void result.current.restoreArchivedSpace('old-space'); });
    await waitFor(() => expect(result.current.isRestoringSpace).toBe(true));
    expect(result.current.isArchivingSpace).toBe(false);
    await act(async () => { pending.resolve({ ok: true, data: null }); await pending.promise; });
    await waitFor(() => expect(result.current.isRestoringSpace).toBe(false));

    const archiving = deferred<{ ok: true; data: null }>();
    api.archiveConversationSpace.mockReturnValue(archiving.promise);
    act(() => { void result.current.setSelectedSpaceArchived(true); });
    await waitFor(() => expect(result.current.isArchivingSpace).toBe(true));
    expect(result.current.isRestoringSpace).toBe(false);
    await act(async () => { archiving.resolve({ ok: true, data: null }); await archiving.promise; });
  });

  it('warns only when deep research is switched on', () => {
    const { result } = setup('botany');
    act(() => result.current.toggleSpaceDeepResearchDefault());
    expect(result.current.spaceDeepResearchDefault).toBe(true);
    expect(toast.warning).toHaveBeenCalledWith('Space Deep Research default enabled', expect.objectContaining({ duration: 5000 }));
    act(() => result.current.toggleSpaceDeepResearchDefault());
    expect(result.current.spaceDeepResearchDefault).toBe(false);
    expect(toast.warning).toHaveBeenCalledTimes(1);
  });
});
