import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { createMemoryRouter, RouterProvider, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { quizOverview } from '@/features/learning/quizzes/quizTestData';
import { LearningStudioPage } from '@/features/learning/workspace/LearningStudioPage';
import { queryClient as activityClient } from '@/lib/queryClient';
import { STUDY_ACTIVITY_KEY } from '@/lib/studyActivity';
import { deferred } from '@/tests/deferred';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import { ok } from '@/tests/learningStudioFixtures';
import type { AppSettings } from '@/types/api/settings';

/**
 * A fresh install sets up a chat model and an embedding model, and leaves the
 * Learning model unset. "Create outline" then fails after the learner has
 * typed a goal and waited. The course builder says so up front, with the one
 * click that fixes it, as the Learning model banner in Settings already does.
 */

const mocks = vi.hoisted(() => ({
  getSettings: vi.fn(), updateSettings: vi.fn(), downloadedModels: vi.fn(), activeModels: vi.fn(),
  listPrograms: vi.fn(), getProgram: vi.fn(),
}));
vi.mock('@/lib/api', async (importOriginal) => ({ ...(await importOriginal<typeof import('@/lib/api')>()), default: {
  listConversationSpaces: async () => ({ ok: true, data: [] }),
  listSpaceDocuments: async () => ({ ok: true, data: [] }),
  listLearningPrograms: mocks.listPrograms,
  getLearningProgram: mocks.getProgram,
  getLearningPlan: async () => ({ ok: true, data: null }),
  getLearningMemory: async () => ({ ok: true, data: null }),
  getLearningQuizOverview: async () => ({ ok: true, data: quizOverview() }),
  getLearningOutlineEvidence: async () => ({ ok: true, data: null }),
  getLearningLessonEvidence: async () => ({ ok: true, data: null }),
  getLearningPracticeWorkspace: async (programId: string) => ({ ok: true, data: { programId, sessions: [] } }),
  listStudyDecks: async () => ({ ok: true, data: [] }),
  courseIndexOpen: async () => ({ ok: true, data: null }),
  courseIndexClose: async () => ({ ok: true, data: null }),
  generateLearningProgram: vi.fn(),
  getSettings: mocks.getSettings,
  updateSettings: mocks.updateSettings,
  getDownloadedModels: mocks.downloadedModels,
  getActiveModels: mocks.activeModels,
} }));

const NOTICE = 'Courses need a model to write them.';
const CHAT_MODEL = 'claude-test-chat';
const CHAT_REF = { kind: 'cloud', provider: 'anthropic', model: CHAT_MODEL } as const;

/** Settings with a cloud chat model chosen, so a chat model is available. */
function withChatModel(learningModel: AppSettings['llm']['learningModel'] = null): AppSettings {
  const base = makeAppSettings();
  return { ...base, llm: { ...base.llm, provider: 'anthropic', model: CHAT_MODEL, learningModel } };
}

/** Settings with no chat model of any kind: auto provider, nothing downloaded, no server saved. */
const withoutChatModel = () => makeAppSettings();

function SettingsProbe() {
  const location = useLocation();
  return <p data-testid="settings-page">{location.pathname}{location.search}</p>;
}

function show(entry = '/studio?new=1') {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const router = createMemoryRouter([
    { path: '/studio', element: <LearningStudioPage /> },
    { path: '/settings', element: <SettingsProbe /> },
  ], { initialEntries: [entry] });
  return render(<QueryClientProvider client={client}><RouterProvider router={router} /></QueryClientProvider>);
}

/** Lets every settings and model read the builder makes finish, so a notice that is going to appear has appeared. */
async function settle() {
  for (let round = 0; round < 3; round += 1) await act(async () => { await new Promise((resolve) => { setTimeout(resolve, 0); }); });
}

describe('Studio course builder with no Learning model', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    activityClient.removeQueries({ queryKey: STUDY_ACTIVITY_KEY });
    mocks.listPrograms.mockResolvedValue(ok([]));
    mocks.getProgram.mockResolvedValue(ok(null));
    mocks.downloadedModels.mockResolvedValue(ok([]));
    mocks.activeModels.mockImplementation(async () => {
      const { data } = await mocks.getSettings();
      const chat = data.llm.provider === 'anthropic' ? [CHAT_REF] : [];
      return ok({
        chat,
        chat_unset: chat.length ? null : 'No chat model is selected in Settings → Downloaded, and no llama.cpp or Ollama server is configured in Settings → Chat',
        utility: [], llama_cpp: null, ollama: null, ollama_utility_tag: null,
      });
    });
  });

  it('says courses need a model and offers the chat model in one click, which sets the Learning model', async () => {
    let saved = withChatModel();
    mocks.getSettings.mockImplementation(async () => ok(saved));
    mocks.updateSettings.mockImplementation(async () => {
      saved = withChatModel(CHAT_REF);
      return ok(saved);
    });
    show();
    expect(await screen.findByText(NOTICE)).toBeVisible();
    await userEvent.click(await screen.findByRole('button', { name: `Use ${CHAT_MODEL}` }));
    // The same write the banner in Settings → Downloaded makes.
    expect(mocks.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { learningModel: CHAT_REF } });
    await waitFor(() => expect(screen.queryByText(NOTICE)).not.toBeInTheDocument());
    expect(screen.getByLabelText('Your goal')).toBeVisible();
  });

  it('offers to choose a model when there is no chat model either, opening Settings where the Learning role is set', async () => {
    mocks.getSettings.mockResolvedValue(ok(withoutChatModel()));
    show();
    expect(await screen.findByText(NOTICE)).toBeVisible();
    expect(screen.queryByRole('button', { name: /^Use / })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Choose a model' }));
    expect(await screen.findByTestId('settings-page')).toHaveTextContent('/settings?tab=downloaded-models');
    expect(mocks.updateSettings).not.toHaveBeenCalled();
  });

  it('says nothing when a Learning model is set', async () => {
    mocks.getSettings.mockResolvedValue(ok(withChatModel(CHAT_REF)));
    show();
    expect(await screen.findByLabelText('Your goal')).toBeVisible();
    await waitFor(() => expect(mocks.getSettings).toHaveBeenCalled());
    await settle();
    expect(screen.queryByText(NOTICE)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Choose a model' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: `Use ${CHAT_MODEL}` })).not.toBeInTheDocument();
  });
});

describe('Studio programs workspace with no Learning model', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    activityClient.removeQueries({ queryKey: STUDY_ACTIVITY_KEY });
    mocks.listPrograms.mockResolvedValue(ok([]));
    mocks.getProgram.mockResolvedValue(ok(null));
    mocks.downloadedModels.mockResolvedValue(ok([]));
    mocks.activeModels.mockImplementation(async () => {
      const { data } = await mocks.getSettings();
      const chat = data.llm.provider === 'anthropic' ? [CHAT_REF] : [];
      return ok({
        chat,
        chat_unset: chat.length ? null : 'No chat model',
        utility: [], llama_cpp: null, ollama: null, ollama_utility_tag: null,
      });
    });
  });

  it('shows the Learning model notice in the program library and hides it once adopted', async () => {
    let saved = withChatModel();
    mocks.getSettings.mockImplementation(async () => ok(saved));
    mocks.updateSettings.mockImplementation(async () => {
      saved = withChatModel(CHAT_REF);
      return ok(saved);
    });
    show('/studio');
    expect(await screen.findByText(NOTICE)).toBeVisible();
    await userEvent.click(await screen.findByRole('button', { name: `Use ${CHAT_MODEL}` }));
    expect(mocks.updateSettings).toHaveBeenCalledWith({ category: 'llm', updates: { learningModel: CHAT_REF } });
    await waitFor(() => expect(screen.queryByText(NOTICE)).not.toBeInTheDocument());
  });

  it('keeps the active program workspace clear of the notice so the lesson stays above the fold', async () => {
    mocks.getSettings.mockResolvedValue(ok(withChatModel()));
    const summary = {
      id: 'program-1',
      title: 'Thermodynamics',
      goal: 'Master heat transfer',
      status: 'active',
      revision: 1,
      moduleCount: 1,
      lessonCount: 1,
      completedLessons: 0,
      sourceCount: 0,
      updatedAt: 1_790_000_000_000,
    };
    mocks.listPrograms.mockResolvedValue(ok([summary]));
    mocks.getProgram.mockResolvedValue(ok({
      summary,
      modules: [{
        id: 'mod-1',
        title: 'Zeroth Law',
        summary: 'Thermal equilibrium',
        outcomes: ['State the zeroth law'],
        lessons: [{
          id: 'les-1',
          title: 'Temperature',
          objective: 'Define temperature',
          estimatedMinutes: 15,
          preparation: 'outline',
          completed: false,
          blocks: [],
        }],
      }],
      sources: [],
    }));
    show('/studio?course=program-1');
    expect(await screen.findByText('Thermodynamics')).toBeVisible();
    await settle();
    expect(screen.queryByText(NOTICE)).not.toBeInTheDocument();
  });

  it('warns when the configured Learning model was deleted or its local server URL is missing', async () => {
    mocks.getSettings.mockResolvedValue(ok(withChatModel({ kind: 'local', model_id: 'deleted-local-model' })));
    mocks.downloadedModels.mockResolvedValue(ok([]));
    show('/studio?new=1');
    expect(await screen.findByText(/Learning model deleted-local-model is not downloaded any more/)).toBeVisible();
    expect(screen.getByRole('button', { name: `Use ${CHAT_MODEL}` })).toBeVisible();
  });

  it('does not offer adopting a broken chat model that points to the same unavailable model', async () => {
    const missingRef = { kind: 'local', model_id: 'missing-model' } as const;
    mocks.getSettings.mockResolvedValue(ok(withChatModel(missingRef)));
    mocks.downloadedModels.mockResolvedValue(ok([]));
    mocks.activeModels.mockResolvedValue(ok({
      chat: [missingRef],
      chat_unset: null,
      utility: [],
      llama_cpp: null,
      ollama: null,
      ollama_utility_tag: null,
    }));
    show('/studio?new=1');
    expect(await screen.findByRole('button', { name: 'Choose a model' })).toBeVisible();
    expect(screen.queryByRole('button', { name: /Use missing-model/ })).not.toBeInTheDocument();
  });

  it('does not claim a local Learning model is missing while downloaded models are still loading or failed to load', async () => {
    const localRef = { kind: 'local', model_id: 'local-llama' } as const;
    mocks.getSettings.mockResolvedValue(ok(withChatModel(localRef)));
    const pendingModels = deferred<ReturnType<typeof ok>>();
    mocks.downloadedModels.mockReturnValueOnce(pendingModels.promise);

    show('/studio?new=1');
    expect(await screen.findByLabelText('Your goal')).toBeVisible();
    await settle();
    expect(screen.queryByText(/is not downloaded any more/)).not.toBeInTheDocument();

    // If loading fails, it must still not falsely warn that the model was deleted.
    await act(async () => {
      pendingModels.resolve({ ok: false, error: 'Database locked' } as unknown as ReturnType<typeof ok>);
    });
    await settle();
    expect(screen.queryByText(/is not downloaded any more/)).not.toBeInTheDocument();
  });
});
