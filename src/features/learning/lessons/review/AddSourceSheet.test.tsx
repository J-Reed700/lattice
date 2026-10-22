import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AddSourceSheet, type AddSourceTarget } from '@/features/learning/lessons/review/AddSourceSheet';
import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { waitingOnSource } from '@/features/learning/lessons/review/claimReviewScreenFixtures';
import { review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { AttachFailureKindDto, AttachNextStepDto, ClaimAttachProgressDto, ClaimAttachReportDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({
  attach: vi.fn(), capture: vi.fn(), review: vi.fn(), audit: vi.fn(), plan: vi.fn(), importFiles: vi.fn(), batchStatus: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: {
  attachLearningClaimEvidence: api.attach, captureLearningSourceSnapshot: api.capture,
  getLearningLessonClaimReview: api.review, getLearningLessonAudit: api.audit, getLearningPlan: api.plan,
  getLearningSourceTrust: async () => ({ ok: true, data: { sources: [], claim: null } }),
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
  startBatchFileImport: api.importFiles, getBatchJobStatus: api.batchStatus,
} }));
const webview = vi.hoisted(() => ({ drop: null as null | ((event: { payload: unknown }) => void) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({
  onDragDropEvent: (handler: (event: { payload: unknown }) => void) => { webview.drop = handler; return Promise.resolve(() => undefined); },
}) }));

const ok = <T,>(data: T) => ({ ok: true, data });
const URL_ = 'https://monarchwatch.example.org/tags';
const source = { sourceId: 'source-watch', sourceVersionId: 'version-watch', title: 'Monarch Watch', url: URL_, addedBy: 'user' as const };
const none = { tried: 0, verified: 0, notVerified: 0, noPassage: 0, stopped: null };
const checked = (supported: boolean): ClaimAttachReportDto['outcome'] => ({ outcome: 'checked', check: {
  checker: { role: 'local', model: 'qwen' }, reason: '', fingerprint: 'f', evidence: [], windowTokens: 1,
  result: supported ? { verdict: 'supported', sourceVersionId: 'version-watch', supportingQuote: 'They fly up to 3,000 miles.' } : { verdict: 'failed', kind: { kind: 'unsupported' } },
} });
const report = (overrides: Partial<ClaimAttachReportDto> = {}): ClaimAttachReportDto => ({
  revision: 8, source, alreadySaved: false, outcome: checked(true), failure: null, snapshot: { state: 'not_applicable' }, others: none, lines: [], ...overrides,
});

/** The attach answers when the test says, with progress events in between. */
function pendingAttach() {
  let progress: ((update: ClaimAttachProgressDto) => void) | null = null;
  let finish: ((value: unknown) => void) | null = null;
  api.attach.mockImplementation((_request: unknown, onProgress: (update: ClaimAttachProgressDto) => void) => {
    progress = onProgress;
    return new Promise((resolve) => { finish = resolve; });
  });
  return {
    progress: async (update: ClaimAttachProgressDto) => { await act(async () => { progress!(update); }); },
    finish: (value: ClaimAttachReportDto) => act(async () => finish!(ok(value))),
  };
}

function sheet(target: AddSourceTarget, onAdded = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <AddSourceSheet programId="program-1" lessonId="lesson-1" revision={7} target={target} initial={null} remaining={32} onClose={vi.fn()} onAdded={onAdded} />
  </QueryClientProvider>);
  return onAdded;
}

function screenWith(...reviews: ReturnType<typeof review>[]) {
  reviews.forEach((value) => api.review.mockResolvedValueOnce(ok(value)));
  api.review.mockResolvedValue(ok(reviews[reviews.length - 1]));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Monarch migration" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}

describe('Add a source', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    webview.drop = null;
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
  });

  it('adds a link for one claim with progress, then shows the result', async () => {
    const user = userEvent.setup();
    const onAdded = sheet({ level: 'claim', claimIds: ['claim-1'] });
    expect(screen.getByRole('heading', { name: 'Add a source for this claim' })).toBeVisible();
    const run = pendingAttach();
    await user.type(screen.getByLabelText('Page address'), URL_);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    expect(api.attach).toHaveBeenCalledWith(
      { programId: 'program-1', lessonId: 'lesson-1', claimId: 'claim-1', expectedRevision: 7, evidence: { kind: 'attach_link', url: URL_ }, scope: { scope: 'claim' } },
      expect.any(Function),
    );
    await run.progress({ stage: 'fetching', host: 'monarchwatch.example.org' });
    expect(screen.getByText('Reading monarchwatch.example.org…')).toBeVisible();
    await run.progress({ stage: 'saving' });
    expect(screen.getByText('Saving it to your sources')).toBeVisible();
    await run.progress({ stage: 'checking', done: 0, total: 1, statement: 'Monarchs migrate up to 3,000 miles.' });
    expect(screen.getByText('Checking claim 1 of 1: Monarchs migrate up to 3,000 miles.')).toBeVisible();
    await run.finish(report());
    await waitFor(() => expect(onAdded).toHaveBeenCalledWith(expect.anything(), expect.objectContaining({ line: 'Added Monarch Watch; it states this. 32 left.', stated: true }), ['claim-1']));
  });

  it('adds one page for a whole group and sums the result', async () => {
    const user = userEvent.setup();
    const ids = Array.from({ length: 15 }, (_, index) => `claim-${index + 1}`);
    const onAdded = sheet({ level: 'group', claimIds: ids });
    expect(screen.getByRole('heading', { name: 'Add a source for these 15 claims' })).toBeVisible();
    const run = pendingAttach();
    await user.type(screen.getByLabelText('Page address'), URL_);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    expect(api.attach.mock.calls[0][0]).toMatchObject({ claimId: 'claim-1', scope: { scope: 'claims', claimIds: ids } });
    await run.progress({ stage: 'checking', done: 4, total: 15, statement: 'Monarch fact number 5.' });
    expect(screen.getByText(/Checking claim 5 of 15/)).toBeVisible();
    expect(screen.getByRole('progressbar', { name: 'Claims checked' })).toHaveAttribute('aria-valuenow', '4');
    await run.progress({ stage: 'checking', done: 9, total: 15, statement: 'Monarch fact number 10.' });
    expect(screen.getByRole('progressbar', { name: 'Claims checked' })).toHaveAttribute('aria-valuenow', '9');
    await run.finish(report({ others: { tried: 14, verified: 11, notVerified: 2, noPassage: 1, stopped: null } }));
    await waitFor(() => expect(onAdded).toHaveBeenCalledWith(expect.anything(),
      expect.objectContaining({ line: 'Added Monarch Watch. Verified 12 of 15 claims; 2 do not match what it says; 1 is not mentioned by it.' }), ids));
  });

  it('shows the closest passage when the page does not say it', async () => {
    const user = userEvent.setup();
    const item = waitingOnSource(1);
    const closest = { passage: { sourceVersionId: 'version-watch', title: 'Monarch Watch', url: URL_, text: 'Tagged monarchs flew about 2,500 miles on average. Some flew farther.', startByte: 0, endByte: 68, retrievalKind: 'hybrid', trust: null }, addedBy: 'user' as const };
    screenWith(review([item]), review([{ ...item, evidence: [closest] }], { revision: 8 }));
    api.attach.mockResolvedValue(ok(report({ outcome: checked(false) })));
    await user.click(await screen.findByRole('button', { name: 'Add a source' }));
    await user.type(screen.getByLabelText('Page address'), URL_);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    const passage = await screen.findByRole('region', { name: 'closest passage in what you added' });
    expect(within(passage).getByText('Tagged monarchs flew about 2,500 miles on average.', { selector: 'mark' })).toBeVisible();
    expect(screen.getByText('Added Monarch Watch; it does not say this. The closest passage is shown: edit the sentence to match it, or add another source.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Edit the sentence' }).className).toMatch(/--action\)/);
    expect(screen.getByRole('button', { name: 'Add another source' })).toBeVisible();
  });

  const failures: Array<[AttachFailureKindDto, string, string, AttachNextStepDto[], string[]]> = [
    ['refused', 'monarchwatch.example.org refused the request (HTTP 403).', 'monarchwatch.example.org refused the request (HTTP 403).', [{ step: 'paste_text', origin: URL_ }, { step: 'add_file' }], ['Open it in your browser, then paste the relevant text', 'Save it as PDF and add the file']],
    ['paywall', 'Sign-in wall.', 'monarchwatch.example.org shows a sign-in or subscription wall.', [{ step: 'paste_text', origin: URL_ }, { step: 'add_file' }], ['Open it in your browser, then paste the relevant text', 'Save it as PDF and add the file']],
    ['thin', 'Too little text.', 'The page has almost no text to check.', [{ step: 'paste_text', origin: null }, { step: 'add_file' }], ['Open it in your browser, then paste the relevant text', 'Save it as PDF and add the file']],
    ['browser_unavailable', 'No browser.', 'The page needs a browser to read and none is available.', [{ step: 'paste_text', origin: null }], ['Open it in your browser, then paste the relevant text']],
    ['blocked', 'Private address.', 'That address is private or local and cannot be fetched.', [{ step: 'paste_text', origin: null }, { step: 'add_file' }], ['Open it in your browser, then paste the relevant text', 'Save it as PDF and add the file']],
    ['unreachable', 'Timed out.', 'monarchwatch.example.org could not be reached.', [{ step: 'retry' }, { step: 'paste_text', origin: null }], ['Try again', 'Open it in your browser, then paste the relevant text']],
  ];

  it.each(failures)('names the %s failure with its next steps', async (kind, message, words, nextSteps, buttons) => {
    const user = userEvent.setup();
    const onAdded = sheet({ level: 'claim', claimIds: ['claim-1'] });
    api.attach.mockResolvedValue(ok(report({ source: null, outcome: null, failure: { kind, message, nextSteps } })));
    await user.type(screen.getByLabelText('Page address'), URL_);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(words);
    expect(within(alert).getAllByRole('button').map((button) => button.textContent)).toEqual(buttons);
    expect(onAdded).not.toHaveBeenCalled();
    if (kind === 'refused') {
      await user.click(within(alert).getByRole('button', { name: 'Open it in your browser, then paste the relevant text' }));
      expect(screen.getByRole('button', { name: 'Paste text instead' })).toHaveAttribute('aria-pressed', 'true');
      expect(screen.getByLabelText('Where it is from')).toHaveValue(URL_);
    }
    if (kind === 'unreachable') {
      await user.click(within(alert).getByRole('button', { name: 'Try again' }));
      await waitFor(() => expect(api.attach).toHaveBeenCalledTimes(2));
      expect(api.attach.mock.calls[1][0]).toEqual(api.attach.mock.calls[0][0]);
    }
  });

  it('accepts a dropped link, a pasted address, pasted text and a dropped file', async () => {
    const user = userEvent.setup();
    screenWith(review([waitingOnSource(1)]));
    const queue = await screen.findByRole('navigation', { name: 'Claims waiting for you' });

    fireEvent.drop(queue, { dataTransfer: { types: ['text/uri-list'], getData: (type: string) => (type === 'text/uri-list' ? URL_ : '') } });
    expect(await screen.findByRole('heading', { name: 'Add a source for this claim' })).toBeVisible();
    expect(screen.getByLabelText('Page address')).toHaveValue(URL_);
    await user.click(screen.getByRole('button', { name: 'Cancel' }));

    fireEvent.paste(queue, { clipboardData: { getData: (type: string) => (type === 'text/plain' ? 'https://journal.example.org/monarchs' : '') } });
    expect(await screen.findByLabelText('Page address')).toHaveValue('https://journal.example.org/monarchs');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));

    const text = 'Tagged monarchs reached central Mexico.\nThe longest flight was about 3,000 miles.';
    fireEvent.paste(queue, { clipboardData: { getData: (type: string) => (type === 'text/plain' ? text : type === 'text/html' ? '<html><!--SourceURL:https://journal.example.org/monarchs--></html>' : '') } });
    expect(await screen.findByLabelText('Text')).toHaveValue(text);
    expect(screen.getByLabelText('Where it is from')).toHaveValue('https://journal.example.org/monarchs');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));

    api.importFiles.mockResolvedValue(ok('job-1'));
    api.batchStatus.mockResolvedValue(ok({ status: 'completed', completedItems: 1, failedItems: 0, totalItems: 1, items: [{ documentId: 'doc-9' }] }));
    api.attach.mockResolvedValue(ok(report()));
    await waitFor(() => expect(webview.drop).not.toBeNull());
    act(() => webview.drop!({ payload: { type: 'drop', paths: ['/Users/me/Monarch tagging.pdf'] } }));
    expect(await screen.findByText('Monarch tagging.pdf', {}, { timeout: 3000 })).toBeVisible();
    expect(api.importFiles).toHaveBeenCalledWith(['/Users/me/Monarch tagging.pdf']);
    await user.click(screen.getByRole('button', { name: 'Add and check' }));
    await waitFor(() => expect(api.attach.mock.calls[0][0]).toMatchObject({ evidence: { kind: 'attach_document', documentId: 'doc-9' } }));
  });
});
