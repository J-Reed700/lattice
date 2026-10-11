import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';

import { useLearningOutlineEvidence } from '@/features/learning/api/evidenceQueries';
import { OutlineEvidenceStatus } from '@/features/learning/curriculum/OutlineEvidence';

const api = vi.hoisted(() => ({ evidence: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningOutlineEvidence: api.evidence } }));

function Evidence() {
  const report = useLearningOutlineEvidence('course', 1);
  return <><OutlineEvidenceStatus report={report} />{report.isSuccess && <p>Citations loaded</p>}</>;
}

it('shows loading, explains failure, and lets the reader retry outline citations', async () => {
  let rejectRequest!: (value: unknown) => void;
  api.evidence.mockImplementationOnce(() => new Promise((resolve) => { rejectRequest = resolve; }));
  api.evidence.mockResolvedValueOnce({ ok: true, data: { citations: [] } });
  const client = new QueryClient();
  const { unmount } = render(<QueryClientProvider client={client}><Evidence /></QueryClientProvider>);
  expect(screen.getByRole('status')).toHaveTextContent('Loading outline citations');
  await act(async () => { rejectRequest({ ok: false, error: 'Saved evidence unavailable.' }); });
  expect(await screen.findByRole('alert')).toHaveTextContent('Outline citations could not be loaded');
  expect(screen.getByRole('alert')).toHaveTextContent('Saved evidence unavailable');
  await userEvent.click(screen.getByRole('button', { name: 'Retry outline citations' }));
  expect(await screen.findByText('Citations loaded')).toBeVisible();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  unmount();
  client.clear();
});
