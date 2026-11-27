import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';

import { useLearningOutlineEvidence } from '@/features/learning/api/evidenceQueries';
import { OutlineCitation, OutlineEvidenceStatus } from '@/features/learning/curriculum/OutlineEvidence';
import type { LearningOutlineCitationDto } from '@/lib/bindings';

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
  await waitFor(() => expect(rejectRequest).toBeTypeOf('function'));
  await act(async () => { rejectRequest({ ok: false, error: 'Saved evidence unavailable.' }); });
  expect(await screen.findByRole('alert')).toHaveTextContent('Outline citations could not be loaded');
  expect(screen.getByRole('alert')).toHaveTextContent('Saved evidence unavailable');
  await userEvent.click(screen.getByRole('button', { name: 'Retry outline citations' }));
  expect(await screen.findByText('Citations loaded')).toBeVisible();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  unmount();
  client.clear();
});

it('renders backend citation numbers via CitationCard and opens saved sources', async () => {
  const onOpen = vi.fn();
  const citation: LearningOutlineCitationDto = {
    number: 4,
    path: '/modules/0/summary',
    target: 'module_summary',
    moduleId: 'm1',
    lessonId: null,
    itemIndex: null,
    claim: 'Verified claim',
    sourceId: 'ver-4',
    sourceTitle: 'Architecture spec',
    sourceUrl: null,
    quote: 'Verbatim passage from the specification.',
    support: {
      relation: 'quotes',
      anchors: [{ exact: 'Verbatim passage from the specification.', span: { start: 10, end: 52 } }],
    },
  };
  render(<OutlineCitation citation={citation} onOpen={onOpen} />);
  await userEvent.click(screen.getAllByText('Architecture spec')[0]!);
  const card = screen.getByRole('group', { name: 'Source 4, quoted from Architecture spec' });
  expect(card).toHaveTextContent('“Verbatim passage from the specification.”');
  await userEvent.click(screen.getByRole('button', { name: 'Open source' }));
  expect(onOpen).toHaveBeenCalledWith(citation);
});

it('renders unverified outline citations as cited without quotation marks', async () => {
  const citation: LearningOutlineCitationDto = {
    number: 2,
    path: '/modules/0/summary',
    target: 'module_summary',
    moduleId: 'm1',
    lessonId: null,
    itemIndex: null,
    claim: 'Paraphrased claim',
    sourceId: 'ver-2',
    sourceTitle: 'Design brief',
    sourceUrl: null,
    quote: 'Summary paraphrase not verbatim in source.',
    support: {
      relation: 'cites',
      anchors: [{ exact: 'Summary paraphrase not verbatim in source.', span: null }],
    },
  };
  render(<OutlineCitation citation={citation} onOpen={vi.fn()} />);
  await userEvent.click(screen.getAllByText('Design brief')[0]!);
  const card = screen.getByRole('group', { name: 'Source 2, cited (not checked) in Design brief' });
  expect(card).toHaveTextContent('Summary paraphrase not verbatim in source.');
  expect(card.textContent).not.toContain('“');
});
