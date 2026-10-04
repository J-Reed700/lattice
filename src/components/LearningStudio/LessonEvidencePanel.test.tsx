import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonEvidencePanel } from './LessonEvidencePanel';

const mocks = vi.hoisted(() => ({ evidence: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningLessonEvidence: mocks.evidence } }));
function show() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><LessonEvidencePanel programId="course" lessonId="lesson" /></QueryClientProvider>);
}
describe('lesson evidence', () => {
  beforeEach(() => vi.clearAllMocks());
  it('loads on demand and makes limitations and source versions visible', async () => {
    mocks.evidence.mockResolvedValue({ ok: true, data: {
      policy: 'lesson-evidence-v2', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: 'embedding', contentSha256: 'hash', claimCount: 9, executedExamples: 0, unexecutedLanguages: ['rust'], sourcesCurrent: false,
      teachingClaims: [{ sectionIndex: 0, claim: 'Borrowing does not transfer ownership.', reason: 'The source supports this.', supportingQuote: 'Borrowing gives access.', passages: [{ sourceVersionId: 'version-1', title: 'Saved Rust reference', url: 'javascript:alert(1)', text: 'Borrowing gives access.', startByte: 0, endByte: 23, retrievalKind: 'hybrid' }] }],
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
});
