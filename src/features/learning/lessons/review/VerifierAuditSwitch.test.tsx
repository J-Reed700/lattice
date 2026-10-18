import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { learningProgramKey } from '@/features/learning/api/learningQueryKeys';
import { auditLine } from '@/features/learning/lessons/review/LessonAudit';
import { VerifierAuditSwitch } from '@/features/learning/lessons/review/VerifierAuditSwitch';
import type { LearningVerifierAuditDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ get: vi.fn(), set: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningProgramVerifierAudit: api.get, setLearningProgramVerifierAudit: api.set } }));

const off: LearningVerifierAuditDto = {
  programId: 'program-1', on: false, webSearch: false, consentAt: null, consentProvider: null, verifier: null,
  destination: 'Anthropic (claude-x)', consentRequiredFor: 'anthropic', blocked: null, usage: { audits: 0, inputTokens: 0, outputTokens: 0 },
};

function show(data: LearningVerifierAuditDto) {
  api.get.mockResolvedValue({ ok: true, data });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(learningProgramKey('program-1'), { summary: { title: 'Field methods' } });
  render(<QueryClientProvider client={client}><VerifierAuditSwitch programId="program-1" /></QueryClientProvider>);
}

describe('Verifier audit switch', () => {
  beforeEach(() => vi.resetAllMocks());

  it('asks for consent naming the provider and what is sent, and turns on only from the confirm button', async () => {
    show(off);
    api.set.mockResolvedValue({ ok: true, data: { ...off, on: true, webSearch: true, consentAt: 1, consentProvider: 'anthropic' } });
    await userEvent.click(await screen.findByRole('switch', { name: /Off/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Audit lessons with your Verifier model before publishing' });
    expect(dialog).toHaveTextContent('Before a lesson of “Field methods” becomes ready');
    expect(dialog).toHaveTextContent('Anthropic (claude-x)');
    expect(dialog).toHaveTextContent('Nothing else from your library is sent.');
    expect(api.set).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('checkbox', { name: 'Allow the Verifier to use web search for this course' }));
    await userEvent.click(screen.getByRole('button', { name: 'Turn on and send lesson text to Anthropic' }));
    await waitFor(() => expect(api.set).toHaveBeenCalledWith('program-1', true, true, true));
    expect(await screen.findByRole('switch')).toHaveAttribute('aria-checked', 'true');
  });

  it('turns off at once and says why an audit that is on would refuse', async () => {
    show({ ...off, on: true, consentAt: 1, consentProvider: 'anthropic', blocked: 'Consent was for Anthropic; confirm again for OpenAI', usage: { audits: 2, inputTokens: 118_420, outputTokens: 6_140 } });
    api.set.mockResolvedValue({ ok: true, data: off });
    expect(await screen.findByText(/Consent was for Anthropic; confirm again for OpenAI/)).toBeVisible();
    expect(screen.getByText(/2 audits · 118,420 tokens sent · 6,140 received/)).toBeVisible();
    await userEvent.click(screen.getByRole('switch'));
    await waitFor(() => expect(api.set).toHaveBeenCalledWith('program-1', false, false, false));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('turns a local Verifier on without the cloud consent dialog', async () => {
    show({ ...off, destination: 'your llama.cpp server (qwen3.5-27b)', consentRequiredFor: null });
    api.set.mockResolvedValue({ ok: true, data: { ...off, on: true, consentRequiredFor: null } });
    await userEvent.click(await screen.findByRole('switch'));
    await waitFor(() => expect(api.set).toHaveBeenCalledWith('program-1', true, false, false));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('describes audit progress in batches and tokens', () => {
    const audit = { lessonId: 'lesson-1', verifierModel: 'cloud:anthropic/claude-x', batchesDone: 2, batchesPlanned: 7, inputTokens: 48_200, outputTokens: 1_200, lastError: null, webNote: null, startedAt: 1, finishedAt: null };
    expect(auditLine({ ...audit, state: 'running' })).toBe('Auditing with cloud:anthropic/claude-x · batch 3 of 7 · 48,200 tokens sent · 1,200 received');
    expect(auditLine({ ...audit, state: 'stopped' })).toBe('Audit stopped after batch 2 of 7 · 48,200 tokens sent · 1,200 received');
  });
});
