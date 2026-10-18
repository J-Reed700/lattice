import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimEvidenceAttach } from '@/features/learning/lessons/review/ClaimEvidenceAttach';
import type { AttachFailureDto, ClaimAttachReportDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ capture: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  captureLearningSourceSnapshot: api.capture,
} }));

const others = { tried: 0, verified: 0, notVerified: 0, noPassage: 0, stopped: null };

function failed(failure: AttachFailureDto): ClaimAttachReportDto {
  return { revision: 7, source: null, alreadySaved: false, outcome: null, failure, snapshot: { state: 'not_applicable' }, others, lines: [failure.message] };
}

function show(onSubmit = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}><ClaimEvidenceAttach programId="program-1" onSubmit={onSubmit} onCancel={vi.fn()} /></QueryClientProvider>);
  return onSubmit;
}

describe('Adding evidence to a claim', () => {
  beforeEach(() => vi.resetAllMocks());

  it('says honestly that a site refused the link and offers paste or file', async () => {
    const url = 'https://news.example.com/article';
    const submit = show(vi.fn().mockResolvedValue(failed({ kind: 'refused', message: 'news.example.com refused the request (HTTP 403).', nextSteps: [{ step: 'paste_text', origin: url }, { step: 'add_file' }] })));
    await userEvent.type(screen.getByLabelText('Page address'), url);
    await userEvent.click(screen.getByRole('button', { name: 'Add and check' }));
    expect(submit).toHaveBeenCalledWith({ action: 'attach_link', url });
    expect(await screen.findByText('news.example.com refused the request (HTTP 403).')).toBeVisible();
    expect(screen.getByText('Not added')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Save it as PDF and add the file' })).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Open it in your browser, then paste the relevant text' }));
    expect(screen.getByRole('button', { name: 'Paste text' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByLabelText('Where it is from')).toHaveValue(`Text from ${url}`);
    expect(screen.getByText(/Pasted text has no public provenance/)).toBeVisible();
  });

  it.each([
    ['paywall', 'The page needs a login or subscription; only 120 words of text could be read.'],
    ['thin', 'This page did not provide enough static reference text.'],
    ['blocked', 'That address is private or local and cannot be fetched.'],
  ] as const)('shows the %s message the backend wrote', async (kind, message) => {
    show(vi.fn().mockResolvedValue(failed({ kind, message, nextSteps: [{ step: 'paste_text', origin: null }, { step: 'add_file' }] })));
    await userEvent.type(screen.getByLabelText('Page address'), 'https://example.org');
    await userEvent.click(screen.getByRole('button', { name: 'Add and check' }));
    expect(await screen.findByText(message)).toBeVisible();
  });

  it('shows its own pending state until the command returns, then what the check found', async () => {
    let finish: (report: ClaimAttachReportDto) => void = () => undefined;
    show(vi.fn(() => new Promise<ClaimAttachReportDto>((resolve) => { finish = resolve; })));
    await userEvent.click(screen.getByRole('button', { name: 'Paste text' }));
    await userEvent.type(screen.getByLabelText('Text'), 'At sea level water boils at 100 °C.');
    await userEvent.click(screen.getByRole('button', { name: 'Add and check' }));
    expect(screen.getByRole('button', { name: 'Checking…' })).toBeDisabled();
    expect(screen.getByText(/Saving your text to your references and checking this claim/)).toBeVisible();
    finish({
      revision: 8, source: { sourceId: 'source-1', sourceVersionId: 'pasted-1', title: 'Pasted text', url: null, addedBy: 'user' }, alreadySaved: false,
      outcome: { outcome: 'no_passage' }, failure: null, snapshot: { state: 'not_applicable' }, others: { ...others, tried: 4, verified: 1, noPassage: 3 },
      lines: ['Verified against Pasted text.', 'Also checked 4 other open claims against this source: 1 verified, 3 no passage.'],
    });
    expect(await screen.findByText('Verified against Pasted text.')).toBeVisible();
    expect(screen.getByText(/Also checked 4 other open claims/)).toBeVisible();
  });

  it('asks for what is missing before sending anything', async () => {
    const submit = show();
    await userEvent.click(screen.getByRole('button', { name: 'Add and check' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Enter the address of the page.');
    expect(submit).not.toHaveBeenCalled();
  });

  it('captures a frozen copy again when the first capture failed', async () => {
    api.capture.mockResolvedValue({ ok: true, data: { state: 'pending' } });
    show(vi.fn().mockResolvedValue({
      revision: 8, source: { sourceId: 'source-9', sourceVersionId: 'version-9', title: 'Example', url: 'https://example.org', addedBy: 'user' }, alreadySaved: false,
      outcome: { outcome: 'no_passage' }, failure: null, snapshot: { state: 'failed', reason: 'the page timed out' }, others,
      lines: ['No passage in Example mentions this claim’s terms.', 'Frozen copy not captured: the page timed out; the text was saved.'],
    }));
    await userEvent.type(screen.getByLabelText('Page address'), 'https://example.org');
    await userEvent.click(screen.getByRole('button', { name: 'Add and check' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Capture again' }));
    await waitFor(() => expect(api.capture).toHaveBeenCalledWith({ programId: 'program-1', sourceId: 'source-9', versionId: 'version-9' }));
    expect(await screen.findByText(/Frozen copy requested/)).toBeVisible();
  });
});
