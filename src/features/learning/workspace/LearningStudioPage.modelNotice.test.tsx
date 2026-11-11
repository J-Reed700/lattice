import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { createMemoryRouter, RouterProvider, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LearningStudioPage } from '@/features/learning/workspace/LearningStudioPage';
import { queryClient as activityClient } from '@/lib/queryClient';
import { STUDY_ACTIVITY_KEY } from '@/lib/studyActivity';
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
  getSettings: vi.fn(), updateSettings: vi.fn(), downloadedModels: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  listConversationSpaces: async () => ({ ok: true, data: [] }),
  listSpaceDocuments: async () => ({ ok: true, data: [] }),
  listLearningPrograms: async () => ({ ok: true, data: [] }),
  listStudyDecks: async () => ({ ok: true, data: [] }),
  generateLearningProgram: vi.fn(),
  getSettings: mocks.getSettings,
  updateSettings: mocks.updateSettings,
  getDownloadedModels: mocks.downloadedModels,
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

function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const router = createMemoryRouter([
    { path: '/studio', element: <LearningStudioPage /> },
    { path: '/settings', element: <SettingsProbe /> },
  ], { initialEntries: ['/studio?new=1'] });
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
    mocks.downloadedModels.mockResolvedValue(ok([]));
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
    await userEvent.click(screen.getByRole('button', { name: `Use ${CHAT_MODEL}` }));
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
