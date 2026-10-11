import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AssessmentEvidencePanel } from '@/features/learning/assessment/AssessmentEvidencePanel';

const mocks = vi.hoisted(() => ({
  workspace: vi.fn(), form: vi.fn(), start: vi.fn(), save: vi.fn(), interrupt: vi.fn(), submit: vi.fn(), accept: vi.fn(), dismiss: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  getLearningAssessmentWorkspace: mocks.workspace,
  getLearningAssessmentForm: mocks.form,
  startLearningAssessmentForm: mocks.start,
  saveLearningAssessmentResponse: mocks.save,
  interruptLearningAssessmentForm: mocks.interrupt,
  submitLearningAssessmentForm: mocks.submit,
  acceptLearningFollowUp: mocks.accept,
  dismissLearningFollowUp: mocks.dismiss,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const blueprint = { id: 'blueprint-1', revision: 3, predecessorRevision: 2, purpose: 'practice', title: 'Reasoning from evidence', instructions: 'Explain what the passage supports.', passingScore: 0.7, rubric: [], sourceVersionIds: ['version-1'], requirements: [{ outcomeId: 'outcome-1', format: 'multiple_choice', count: 1, difficultyMin: 1, difficultyMax: 3 }], status: 'accepted', changeReason: '', createdAt: 1_790_000_000_000 };
const item = { id: 'item-1', outcomeIds: ['outcome-1'], format: 'multiple_choice', difficulty: 2, prompt: 'Which claim is best supported?', options: ['The measured pattern repeats.', 'The cause is fully known.'], artifactKind: null, rubric: [], points: 1, sourceVersionIds: ['version-1'], previouslyExposed: true, selectedIndex: null, textResponse: null, orderedValues: [], artifactJson: null, responseRevision: 0 };
const explanationItem = { ...item, id: 'item-2', format: 'explanation', prompt: 'Explain the limit of this evidence.', options: [], previouslyExposed: false, points: 2 };
const form = (overrides: Record<string, unknown> = {}) => ({ id: 'form-1', programId: 'program-1', blueprintId: blueprint.id, blueprintRevision: 3, retakeOfFormId: null, status: 'active', revision: 1, title: blueprint.title, instructions: blueprint.instructions, purpose: 'practice', passingScore: 0.7, rubric: [], sourceVersionIds: ['version-1'], modelName: 'Assessment model', items: [item, explanationItem], createdAt: 1_790_000_000_000, updatedAt: 1_790_000_000_000, submittedAt: null, submission: null, ...overrides });
const workspace = (overrides: Record<string, unknown> = {}) => ({ programId: 'program-1', outcomes: [{ id: 'outcome-1', moduleId: null, lessonId: null, title: 'Reason carefully', description: 'Use evidence within its limits.', ordinal: 0, createdAt: 1_790_000_000_000 }], blueprints: [blueprint], forms: [], evidence: [{ id: 'ev-1', outcomeId: 'outcome-1', sourceKind: 'assessment', sourceId: 'form-old', dimension: 'transfer', result: 'uncertain', score: null, observation: 'An unfamiliar scenario response needs review.', evidenceQuote: 'the situation changed', assistance: {}, observedAt: 1_790_000_000_000 }], followUps: [{ id: 'follow-1', outcomeId: 'outcome-1', reasonCode: 'uncertain_grade', explanation: 'A new example would provide another observation.', actionKind: 'practice', actionRef: null, status: 'pending', evidenceEventIds: ['ev-1'], createdAt: 1_790_000_000_000, decidedAt: null }], ...overrides });

function renderPanel() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<AssessmentEvidencePanel programId="program-1" />, { wrapper });
}

describe('Learning Studio Assess & evidence', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.workspace.mockResolvedValue(ok(workspace()));
    mocks.form.mockImplementation(async () => ok(form()));
    mocks.start.mockImplementation(async (request) => ok(form({ id: request.formId })));
    mocks.save.mockImplementation(async (request) => ok(form({ revision: request.expectedRevision + 1 })));
    mocks.interrupt.mockImplementation(async () => ok(form({ status: 'interrupted' })));
    mocks.submit.mockImplementation(async () => ok(form({ status: 'submitted', revision: 3, submission: { score: 0.75, passed: true, gradeStatus: 'provisional', graderModel: 'Assessment model', graderDisagreement: ['Open-response grading is provisional.'], feedback: 'A promising answer with one point to revisit.', itemResults: [], submittedAt: 1_790_100_000_000 } })));
    mocks.accept.mockResolvedValue(ok(workspace({ followUps: [] })));
    mocks.dismiss.mockResolvedValue(ok(workspace({ followUps: [] })));
  });

  it('shows four evidence dimensions without turning missing signals into failure', async () => {
    renderPanel();
    expect(await screen.findByText('Four ways to observe learning')).toBeVisible();
    expect(screen.getAllByText('No recorded evidence yet').length).toBeGreaterThan(0);
    expect(screen.getAllByText(/uncertain/i).length).toBeGreaterThan(0);
    expect(screen.getByText(/not that an outcome was missed/)).toBeVisible();
  });

  it('starts a selected-purpose form and renders MCQ and open response without answer keys', async () => {
    const user = userEvent.setup();
    renderPanel();
    await user.click(await screen.findByRole('button', { name: 'Start' }));
    expect(await screen.findByRole('heading', { name: 'Reasoning from evidence' })).toBeVisible();
    expect(screen.getByText('Exposed repeat')).toBeVisible();
    expect(screen.getByRole('radio', { name: 'The measured pattern repeats.' })).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Your explanation' })).toBeInTheDocument();
    expect(screen.queryByText(/correct answer|answer key/i)).not.toBeInTheDocument();
    expect(mocks.start.mock.calls[0][0]).toMatchObject({ programId: 'program-1', blueprintId: 'blueprint-1', blueprintRevision: 3, retakeOfFormId: null });
    expect(mocks.start.mock.calls[0][0].formId).toMatch(/^[0-9a-f-]{36}$/i);
  });

  it('renders assessment instructions and prompts as structured rich text', async () => {
    const richForm = form({
      instructions: '## Instructions\n\n- Use the saved evidence\n- State its limit',
      items: [{ ...item, prompt: '### Scenario\n\nWhich claim follows from `sample_size`?' }, explanationItem],
    });
    mocks.start.mockResolvedValue(ok(richForm));
    mocks.form.mockResolvedValue(ok(richForm));
    const user = userEvent.setup();
    const view = renderPanel();
    await user.click(await screen.findByRole('button', { name: 'Start' }));
    expect(await screen.findByRole('heading', { name: 'Instructions' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Scenario' })).toBeVisible();
    expect(view.container.querySelector('code')).toHaveTextContent('sample_size');
  });

  it('autosaves responses with CAS revisions and retains the exact request for a lost-response retry', async () => {
    const user = userEvent.setup();
    mocks.save.mockImplementationOnce(async () => fail('Connection interrupted')).mockImplementation(async (request) => ok(form({ revision: request.expectedRevision + 1 })));
    renderPanel();
    await user.click(await screen.findByRole('button', { name: 'Start' }));
    await user.click(await screen.findByRole('radio', { name: 'The measured pattern repeats.' }));
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(1), { timeout: 4_000 });
    const request = mocks.save.mock.calls[0][0];
    expect(await screen.findByRole('button', { name: 'Retry same save' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Retry same save' }));
    await waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2));
    expect(mocks.save.mock.calls[1][0]).toEqual(request);
    expect(request.response).toMatchObject({ itemId: 'item-1', selectedIndex: 0 });
  });

  it('requires all items before submission and uses explicit submit confirmation', async () => {
    const user = userEvent.setup();
    renderPanel();
    await user.click(await screen.findByRole('button', { name: 'Start' }));
    expect(screen.getByRole('button', { name: 'Submit assessment' })).toBeDisabled();
    await user.click(screen.getByRole('radio', { name: 'The measured pattern repeats.' }));
    await user.type(screen.getByRole('textbox', { name: 'Your explanation' }), 'The evidence shows a repeated pattern, but not its cause.');
    await waitFor(() => expect(screen.getByRole('button', { name: 'Submit assessment' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Submit assessment' }));
    expect(await screen.findByRole('alertdialog', { name: 'Submit this assessment?' })).toBeVisible();
    expect(mocks.submit).not.toHaveBeenCalled();
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Submit now' }));
    await waitFor(() => expect(mocks.submit).toHaveBeenCalledWith(expect.objectContaining({ expectedRevision: expect.any(Number), formId: 'form-1' })));
  });

  it('backs out of the pause confirmation on Escape without pausing', async () => {
    const user = userEvent.setup();
    renderPanel();
    await user.click(await screen.findByRole('button', { name: 'Start' }));
    await user.click(screen.getByRole('button', { name: 'Pause attempt' }));
    expect(await screen.findByRole('alertdialog', { name: 'Pause this attempt?' })).toBeVisible();
    expect(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Keep working' })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    expect(mocks.interrupt).not.toHaveBeenCalled();
  });

  it('labels uncertainty and lets learners accept or dismiss deterministic follow-ups', async () => {
    const user = userEvent.setup();
    renderPanel();
    expect(await screen.findByText('Evidence-based follow-ups')).toBeVisible();
    expect(screen.getByText('Uncertain grade')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Accept suggestion' }));
    await waitFor(() => expect(mocks.accept).toHaveBeenCalledWith(expect.objectContaining({ followUpId: 'follow-1', programId: 'program-1' })));
  });

  it('shows an honest empty state when no accepted assessment blueprint exists', async () => {
    mocks.workspace.mockResolvedValue(ok(workspace({ blueprints: [] })));
    renderPanel();
    expect(await screen.findByText('No accepted practice form yet')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Start' })).not.toBeInTheDocument();
  });
});
