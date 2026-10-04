import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type { LearningAssessmentWorkspaceDto, LearningModuleDto, LearningProgramDto } from '@/lib/bindings';

import { ModuleCheckpoint } from './ModuleCheckpoint';

const create = vi.hoisted(() => vi.fn());
vi.mock('@/lib/api', () => ({ default: { createLearningAssessmentBlueprint: create } }));
const module: LearningModuleDto = { id: 'module', title: 'Experimental design', summary: 'Compare interventions', outcomes: ['Design a comparison'], lessons: [{ id: 'lesson', title: 'Controls', objective: 'Design a control group', estimatedMinutes: 30, preparation: 'ready', completed: false, blocks: [], questions: [] }] };
const program: LearningProgramDto = { summary: { id: 'course', title: 'Experiments', goal: 'Run experiments', status: 'active', revision: 2, moduleCount: 1, lessonCount: 1, completedLessons: 0, currentLessonId: 'lesson', createdAt: 0 }, priorKnowledge: '', minutesPerSession: 30, modelName: 'test', modules: [module], sources: [], attempts: [] };
const workspace: LearningAssessmentWorkspaceDto = { programId: 'course', outcomes: [{ id: 'outcome', moduleId: 'module', lessonId: null, title: 'Design a comparison', description: '', ordinal: 0, createdAt: 0 }], blueprints: [], forms: [], evidence: [], followUps: [] };

function show(currentModule = module) {
  const done = vi.fn();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><ModuleCheckpoint program={program} module={currentModule} workspace={workspace} onCreated={done} /></QueryClientProvider>);
  return done;
}

describe('module checkpoint', () => {
  it('authors written and applied tasks from module outcomes and safely retries a failed request', async () => {
    create.mockReset().mockResolvedValueOnce({ ok: false, error: 'Connection interrupted' }).mockResolvedValueOnce({ ok: true, data: workspace });
    const done = show();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Prepare module checkpoint' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Connection interrupted');
    const request = create.mock.calls[0][0];
    expect(request).toMatchObject({ programId: 'course', sourceVersionIds: [], purpose: 'module_test', feedbackTiming: 'after_submission', requirements: [{ outcomeId: 'outcome', format: 'explanation', count: 1 }, { outcomeId: 'outcome', format: 'artifact', count: 1 }] });
    expect(request.rubric).toHaveLength(3);
    await user.click(screen.getByRole('button', { name: 'Retry checkpoint' }));
    await waitFor(() => expect(done).toHaveBeenCalledOnce());
    expect(create.mock.calls[1][0]).toEqual(request);
  });

  it('offers a prior-knowledge challenge without forcing lesson preparation', async () => {
    create.mockReset().mockResolvedValue({ ok: true, data: workspace });
    show({ ...module, lessons: module.lessons.map((lesson) => ({ ...lesson, preparation: 'outline' })) });
    await userEvent.setup().click(screen.getByRole('button', { name: 'Challenge this module' }));
    await waitFor(() => expect(create).toHaveBeenCalledOnce());
    expect(create.mock.calls[0][0].instructions).toContain('optional prior-knowledge challenge');
  });
});
