import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { LessonEvidenceQuery } from '@/features/learning/api/evidenceQueries';
import { LessonEvidencePanel, LessonSectionEvidence } from '@/features/learning/lessons/LessonEvidencePanel';

const mocks = vi.hoisted(() => ({ evidence: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningLessonEvidence: mocks.evidence } }));
function show() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><LessonEvidencePanel programId="course" lessonId="lesson" /></QueryClientProvider>);
}
describe('lesson evidence', () => {
  beforeEach(() => vi.clearAllMocks());
  it('loads on demand and makes limitations and source versions visible', async () => {
    mocks.evidence.mockResolvedValue({ ok: true, data: {
      policy: 'lesson-evidence-v2', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: 'embedding', contentSha256: 'hash', claimCount: 9, generalKnowledgeCount: 0, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: ['rust'], sourcesCurrent: false,
      teachingClaims: [{ sectionIndex: 0, contentQuote: 'Borrowing leaves ownership with the owner.', claim: 'Borrowing does not transfer ownership.', verdict: 'supported', reason: 'The source supports this.', supportingQuote: 'Borrowing gives access.', passages: [{ sourceVersionId: 'version-1', title: 'Saved Rust reference', url: 'javascript:alert(1)', text: 'Borrowing gives access.', startByte: 0, endByte: 23, retrievalKind: 'hybrid' }] }],
    } });
    show();
    expect(mocks.evidence).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/9 claims checked/)).toBeVisible();
    expect(screen.getByText(/were not compiled or executed/)).toBeVisible();
    expect(screen.getByText(/collection has changed/)).toBeVisible();
    await userEvent.click(screen.getByText('Borrowing does not transfer ownership.'));
    await userEvent.click(screen.getByText('Saved Rust reference'));
    expect(screen.getByText(/Saved version version-1/)).toBeVisible();
    expect(screen.queryByRole('link', { name: 'Open original page' })).not.toBeInTheDocument();
  });
  it('does not imply older lessons passed verification', async () => {
    mocks.evidence.mockResolvedValue({ ok: true, data: null });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/no saved verification report/)).toBeVisible();
  });

  it('shows invalid saved evidence as unavailable without presenting it as checked', async () => {
    mocks.evidence.mockResolvedValue({ ok: false, error: 'Saved lesson evidence is unavailable: invalid report.' });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('evidence is unavailable');
    expect(screen.queryByText(/claims verified against sources/)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retry evidence' })).toBeVisible();
  });

  it('puts checked claims beside the generated section they verify', async () => {
    const data = {
      policy: 'lesson-evidence-v17', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash', claimCount: 1, generalKnowledgeCount: 0, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
      teachingClaims: [{ sectionIndex: 1, contentQuote: 'The generated sentence.', claim: 'A grounded claim.', verdict: 'supported', reason: 'Exact passage agrees.', supportingQuote: null, passages: [{ sourceVersionId: 'version-2', title: 'Frozen source', url: 'https://example.com', text: 'The exact frozen passage.', startByte: 40, endByte: 65, retrievalKind: 'lexical' }] }],
    };
    const report = { data, isFetching: false, isError: false } as unknown as LessonEvidenceQuery;
    render(<LessonSectionEvidence report={report} sectionIndex={1} />);

    expect(screen.getByText(/Evidence · 1 checked claim · 1 passage/)).toBeVisible();
    await userEvent.click(screen.getByText(/Evidence · 1 checked claim/));
    expect(screen.getByText('A grounded claim.')).toBeVisible();
    expect(screen.getByText(/The generated sentence/)).toBeVisible();
    await userEvent.click(screen.getByText('Frozen source'));
    expect(screen.getByText('The exact frozen passage.')).toBeVisible();
  });

  it('labels unsourced claims as general knowledge, neutrally and counted apart', async () => {
    const data = {
      policy: 'lesson-evidence-v18', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash', claimCount: 3, generalKnowledgeCount: 1, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
      teachingClaims: [
        { sectionIndex: 0, contentQuote: 'Queries only read.', claim: 'Query handlers read state.', verdict: 'supported', reason: 'Exact passage agrees.', supportingQuote: null, passages: [{ sourceVersionId: 'version-3', title: 'CQRS notes', url: null, text: 'Queries read state.', startByte: 0, endByte: 19, retrievalKind: 'hybrid' }] },
        { sectionIndex: 0, contentQuote: 'Handlers stay small.', claim: 'Small handlers are easier to test.', verdict: 'general_knowledge', reason: 'No saved reference states this.', supportingQuote: null, passages: [] },
      ],
    };
    const report = { data, isFetching: false, isError: false } as unknown as LessonEvidenceQuery;
    render(<LessonSectionEvidence report={report} sectionIndex={0} />);

    expect(screen.getByText(/Evidence · 1 checked claim · 1 general knowledge · 1 passage/)).toBeVisible();
    await userEvent.click(screen.getByText(/Evidence · 1 checked claim/));
    const label = screen.getByText('General knowledge · not checked against a source');
    expect(label).toBeVisible();
    expect(label.className).not.toMatch(/rose|red|amber|emerald/);
    expect(screen.queryByText('general_knowledge')).not.toBeInTheDocument();

    mocks.evidence.mockResolvedValue({ ok: true, data });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/2 claims verified against sources · 1 general knowledge, not checked against a source/)).toBeVisible();
  });

  it('shows a confirmation as the user’s, apart from machine verification, and flags removed sources', async () => {
    const passage = { sourceVersionId: 'version-4', title: 'Steam tables', url: null, text: 'Water boils at 99.97 °C at one atmosphere.', startByte: 0, endByte: 42, retrievalKind: 'hybrid' };
    mocks.evidence.mockResolvedValue({ ok: true, data: {
      policy: 'lesson-evidence-v19', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash', claimCount: 17, generalKnowledgeCount: 2, confirmedCount: 3, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
      teachingClaims: [
        { sectionIndex: 0, contentQuote: 'Water boils at 100 °C.', claim: 'Water boils at 100 °C at sea level.', verdict: 'contradicted', reason: 'The passage gives 99.97 °C.', supportingQuote: null, passages: [passage], resolution: 'confirmed_by_you', note: 'Rounded for beginners.' },
        { sectionIndex: 0, contentQuote: 'Ice floats.', claim: 'Ice floats on water.', verdict: 'supported', reason: 'Supported.', supportingQuote: null, passages: [passage], resolution: 'source_removed', note: null },
      ],
    } });
    show();
    await userEvent.click(screen.getByRole('button', { name: 'View lesson evidence' }));
    expect(await screen.findByText(/12 claims verified against sources · 3 confirmed by you · 2 general knowledge/)).toBeVisible();
    const confirmed = screen.getByText('Confirmed by you');
    expect(confirmed.className).not.toMatch(/emerald/);
    expect(screen.getByText(/Rounded for beginners/)).toBeVisible();
    expect(screen.getByText('Source removed · review')).toBeVisible();
    expect(screen.getByRole('status')).toHaveTextContent('1 claim lost its source');
  });
});
