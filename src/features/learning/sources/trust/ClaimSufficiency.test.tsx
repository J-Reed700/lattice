import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimSufficiency } from '@/features/learning/sources/trust/ClaimSufficiency';
import { blogOnlyClaim } from '@/features/learning/sources/trust/trustTestData';
import type { ClaimReviewItemDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ get: vi.fn(), reprofile: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningSourceTrust: api.get, reprofileLearningClaim: api.reprofile } }));

const passage = {
  sourceVersionId: 'version-1', title: 'A blog post on structured outputs', url: 'https://example-dev.net/post', text: 'strict mode',
  startByte: 0, endByte: 11, retrievalKind: 'hybrid', trust: null,
};

function item(state: 'open' | 'queued'): ClaimReviewItemDto {
  return {
    claimId: 'claim-1',
    location: { unitIndex: 0, unitKind: 'block', unitTitle: 'Section 1', field: 'body', quote: 'q', span: null },
    statement: 'With strict mode on, the response matches the schema.',
    state: state === 'open' ? { state: 'open', kind: { kind: 'unsupported' }, reason: 'r', machineAttemptsLeft: 1 } : { state: 'queued', kind: { kind: 'unsupported' }, reason: 'r' },
    verdictKind: { kind: 'unsupported' },
    reason: 'Its support does not come from the subject’s owner.',
    evidence: [{ passage, addedBy: 'research' }],
    supportingQuote: 'strict mode',
    research: [], repairs: [], executions: [], human: null, availableActions: [], history: [], localCheck: null, verifierCheck: null,
  } as unknown as ClaimReviewItemDto;
}

function show(state: 'open' | 'queued') {
  api.get.mockResolvedValue({ ok: true, data: { sources: [], claim: blogOnlyClaim() } });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}><ClaimSufficiency item={item(state)} programId="program-1" /></QueryClientProvider>);
}

describe('Claim sufficiency block', () => {
  beforeEach(() => vi.resetAllMocks());

  it('reads the supports’ standing for the claim’s subject, from the passages it compared', async () => {
    show('open');
    await screen.findByText(/^Specification claim about OpenAI API/);
    expect(api.get).toHaveBeenCalledWith({
      programId: 'program-1', sourceIds: [], includeImpact: true,
      claim: { statement: 'With strict mode on, the response matches the schema.', sourceVersionIds: ['version-1'] },
    });
    expect(screen.getByText('Specification claim about OpenAI API · supported only by an established source · accepted alone: a primary or authoritative source')).toBeVisible();
    expect(screen.getByRole('button', { name: /Established · Why\?/ })).toBeVisible();
  });

  it('explains in plain words why the support is not enough and what happens next', async () => {
    show('open');
    expect(await screen.findByRole('note')).toHaveTextContent('An established source (an identified author, such as a blog) isn’t enough for how an API, standard or rule is defined; looking for the owner’s documentation.');
  });

  it('says it waits for the user once the machine is done', async () => {
    show('queued');
    expect(await screen.findByRole('note')).toHaveTextContent('waiting for you: accept this source for this claim, add evidence, edit the claim or remove it');
  });

  it('offers the claim’s subject, aliases and classes to trust a site for', async () => {
    show('open');
    await userEvent.click(await screen.findByRole('button', { name: /Change trust/ }));
    await userEvent.click(screen.getByRole('radio', { name: /for a subject/ }));
    for (const option of ['OpenAI API', 'OpenAI platform', 'LLM APIs']) expect(screen.getByRole('checkbox', { name: option })).toBeVisible();
  });

  it('profiles the claim again on request', async () => {
    api.reprofile.mockResolvedValue({ ok: true, data: blogOnlyClaim().profile });
    show('open');
    await userEvent.click(await screen.findByRole('button', { name: 'Profile this claim again' }));
    await waitFor(() => expect(api.reprofile).toHaveBeenCalledWith({ programId: 'program-1', claimId: 'claim-1' }));
  });
});
