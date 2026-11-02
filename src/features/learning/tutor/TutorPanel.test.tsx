import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { learningMemoryKey, learningRecallKey } from '@/features/learning/api/learningQueryKeys';
import type { LearningProgramDto } from '@/lib/bindings';
import type { Conversation } from '@/types/conversation';

import { TutorPanel } from './TutorPanel';

const mocks = vi.hoisted(() => ({
  store: {
    activeConversationId: null as string | null,
    inFlightGenerations: new Set<string>(),
    selectConversation: vi.fn(),
    createConversation: vi.fn(),
    deleteConversation: vi.fn(),
  },
  api: {
    listConversationsExplorer: vi.fn(),
    setConversationCourse: vi.fn(),
  },
  listeners: new Set<() => void>(),
  fill: vi.fn(),
}));

vi.mock('@/features/chat/components/ChatPanel', () => ({
  ChatPanel: ({ starters }: { starters?: string[] }) => (
    <div>
      The shared chat
      {starters?.map((starter) => <button key={starter} type="button" onClick={() => mocks.fill(starter)}>{starter}</button>)}
    </div>
  ),
}));
vi.mock('@/contexts/ApiContext', () => ({ useApi: () => mocks.api }));
vi.mock('@/shared/conversations/conversationsStore', async () => {
  const { useSyncExternalStore } = await import('react');
  return {
    useConversationsStore: <T,>(selector: (_state: typeof mocks.store) => T) =>
      useSyncExternalStore(
        (listener) => {
          mocks.listeners.add(listener);
          return () => mocks.listeners.delete(listener);
        },
        () => selector(mocks.store),
      ),
  };
});

const setStore = (change: Partial<typeof mocks.store>) => {
  Object.assign(mocks.store, change);
  act(() => mocks.listeners.forEach((listener) => listener()));
};

const program = {
  summary: { id: 'course-1', title: 'Search' },
} as unknown as LearningProgramDto;
const lesson = { id: 'lesson-1', title: 'Fusion' } as never;

function chat(id: string, title: string): Conversation {
  return { id, title, programId: 'course-1', updatedAt: '2026-11-01T00:00:00Z' } as Conversation;
}

function show(onCourseChanged?: () => void) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}><TooltipProvider>{children}</TooltipProvider></QueryClientProvider>
  );
  render(<TutorPanel program={program} lesson={lesson} onCourseChanged={onCourseChanged} />, { wrapper });
  return client;
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.store.activeConversationId = null;
  mocks.store.inFlightGenerations = new Set();
  mocks.api.listConversationsExplorer.mockResolvedValue({ ok: true, data: { conversations: [] } });
  mocks.api.setConversationCourse.mockResolvedValue(undefined);
  mocks.store.createConversation.mockResolvedValue('fresh');
});

describe('TutorPanel', () => {
  it('lists the course’s chats and shows the shared chat for the one open', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue({
      ok: true,
      data: { conversations: [chat('one', 'Fusion · Nov 1'), chat('two', 'Search · Oct 30')] },
    });
    mocks.store.activeConversationId = 'one';
    show();

    expect(await screen.findByRole('button', { name: 'Fusion · Nov 1' })).toHaveAttribute('aria-current', 'true');
    expect(screen.getByRole('button', { name: 'Search · Oct 30' })).toBeInTheDocument();
    expect(screen.getByText('The shared chat')).toBeInTheDocument();
    expect(mocks.api.listConversationsExplorer).toHaveBeenCalledWith({ programId: 'course-1' });
  });

  it('starts an empty chat from the lesson and the reviews, not the library', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue({
      ok: true,
      data: { conversations: [chat('one', 'Fusion')] },
    });
    mocks.store.activeConversationId = 'one';
    show();

    await userEvent.click(await screen.findByRole('button', { name: 'Teach me Fusion' }));
    expect(mocks.fill).toHaveBeenCalledWith('Teach me Fusion');
    expect(screen.getByRole('button', { name: 'Quiz me on what is due' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'What should I work on next?' })).toBeInTheDocument();
  });

  it('opens the most recent chat when the one open belongs to no course chat', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue({
      ok: true,
      data: { conversations: [chat('one', 'Fusion'), chat('two', 'Search')] },
    });
    mocks.store.activeConversationId = 'a-chat-from-elsewhere';
    show();

    await waitFor(() => expect(mocks.store.selectConversation).toHaveBeenCalledWith('one'));
    expect(screen.queryByText('The shared chat')).not.toBeInTheDocument();
  });

  it('starts a chat bound to the course, and on the lesson when asked', async () => {
    show();

    await userEvent.click(await screen.findByRole('button', { name: 'Start a tutor chat' }));
    await waitFor(() => expect(mocks.api.setConversationCourse).toHaveBeenCalledWith({
      conversationId: 'fresh', programId: 'course-1', lessonId: null,
    }));

    await userEvent.click(screen.getByRole('button', { name: 'Start on Fusion' }));
    await waitFor(() => expect(mocks.api.setConversationCourse).toHaveBeenLastCalledWith({
      conversationId: 'fresh', programId: 'course-1', lessonId: 'lesson-1',
    }));
    expect(mocks.store.createConversation.mock.calls[1][0]).toMatch(/^Fusion · /);
  });

  it('says why a chat could not be started', async () => {
    mocks.api.setConversationCourse.mockRejectedValue(new Error('Accept the course first'));
    show();

    await userEvent.click(await screen.findByRole('button', { name: 'Start a tutor chat' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('Accept the course first');
  });

  it('reads Recall and the Notebook again when a turn ends', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue({ ok: true, data: { conversations: [chat('one', 'Fusion')] } });
    mocks.store.activeConversationId = 'one';
    const onCourseChanged = vi.fn();
    const client = show(onCourseChanged);
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    await screen.findByText('The shared chat');

    setStore({ inFlightGenerations: new Set(['one']) });
    expect(onCourseChanged).not.toHaveBeenCalled();
    setStore({ inFlightGenerations: new Set() });

    expect(invalidate).toHaveBeenCalledWith({ queryKey: learningMemoryKey('course-1') });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: learningRecallKey('course-1') });
    expect(onCourseChanged).toHaveBeenCalledTimes(1);
  });

  it('asks before a chat is deleted', async () => {
    mocks.api.listConversationsExplorer.mockResolvedValue({ ok: true, data: { conversations: [chat('one', 'Fusion')] } });
    mocks.store.activeConversationId = 'one';
    mocks.store.deleteConversation.mockResolvedValue(true);
    show();

    await userEvent.click(await screen.findByRole('button', { name: 'Delete Fusion' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete conversation' }));

    await waitFor(() => expect(mocks.store.deleteConversation).toHaveBeenCalledWith('one'));
  });
});
