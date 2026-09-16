import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Link, MemoryRouter, Route, Routes } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { StudyCardDto } from '@/lib/bindings';

import { NewStudyDeck } from './NewStudyDeck';
import { StudyPage } from './StudyPage';
import { StudySession } from './StudySession';

const mocks = vi.hoisted(() => ({ review: vi.fn(), generate: vi.fn(), documents: vi.fn(), decks: vi.fn(), deck: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { reviewStudyCard: mocks.review, generateStudyDeck: mocks.generate, listAllDocuments: mocks.documents, listStudyDecks: mocks.decks, getStudyDeck: mocks.deck } }));
vi.mock('@/components/ContentViewer/ContentViewer', () => ({ ContentViewer: () => <div>Source viewer</div> }));
const card: StudyCardDto = {
  id: 'card', deckId: 'deck', question: 'What absorbs light in photosynthesis?', answer: 'Chlorophyll',
  options: ['Chlorophyll', 'Water', 'Oxygen', 'Glucose', 'Carbon dioxide'], correctIndex: 0,
  explanation: 'Chlorophyll absorbs the light used in photosynthesis.', topic: 'Chlorophyll',
  source: { chunkId: 'chunk', documentId: 'document', fileName: 'biology.md', filePath: '/biology.md', excerpt: 'Chlorophyll absorbs the light used in photosynthesis.' },
  citations: [{ chunkId: 'chunk', documentId: 'document', fileName: 'biology.md', filePath: '/biology.md', excerpt: 'Chlorophyll absorbs the light used in photosynthesis.' }],
  dueAt: 0, intervalDays: 0, reviewCount: 0, lapses: 0,
};
function show(ui: React.ReactNode) {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })}>{ui}</QueryClientProvider>);
}
describe('Study', () => {
  beforeEach(() => { vi.resetAllMocks(); });
  it('keeps a revealed flashcard in place on save failure and retries the same review', async () => {
    const user = userEvent.setup();
    mocks.review.mockResolvedValueOnce({ ok: false, error: 'Disk full' }).mockResolvedValueOnce({ ok: true, data: { ...card, reviewCount: 1 } });
    show(<StudySession cards={[card]} mode="flashcard" title="Biology" onClose={vi.fn()} />);
    expect(screen.queryByText(card.answer, { exact: true })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Reveal answer' }));
    expect(screen.getAllByText(card.answer, { exact: true }).length).toBeGreaterThan(0);
    await user.click(screen.getByRole('button', { name: 'Good' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Disk full');
    expect(screen.queryByText('Review complete')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Good' }));
    expect(await screen.findByText('Review complete')).toBeVisible();
    expect(mocks.review.mock.calls[1][0].reviewId).toBe(mocks.review.mock.calls[0][0].reviewId);
    expect(mocks.review.mock.calls[0][0]).toMatchObject({ cardId: 'card', rating: 'good', selectedOption: null, expectedReviews: 0 });
  });
  it('records a quiz selection before revealing the answer and reports missed topics', async () => {
    const user = userEvent.setup();
    mocks.review.mockResolvedValue({ ok: true, data: { ...card, reviewCount: 1, lapses: 1 } });
    show(<StudySession cards={[card]} mode="quiz" title="Biology" onClose={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Check answer' })).toBeDisabled();
    await user.click(screen.getByRole('radio', { name: 'B. Water' }));
    await user.click(screen.getByRole('button', { name: 'Check answer' }));
    expect(await screen.findByText('Review this answer')).toBeVisible();
    expect(mocks.review.mock.calls[0][0]).toMatchObject({ selectedOption: 1, expectedReviews: 0 });
    await user.click(screen.getByRole('button', { name: 'Finish practice' }));
    expect(await screen.findByText('1 question · 0 correct')).toBeVisible();
    expect(screen.getByText('Revisit')).toBeVisible();
  });
  it('shows every supporting citation on the answer side', async () => {
    const user = userEvent.setup();
    const citedCard: StudyCardDto = {
      ...card,
      citations: [
        card.source,
        { chunkId: 'chunk-2', documentId: 'document-2', fileName: 'botany.md', filePath: '/botany.md', excerpt: 'A second passage independently supports the verified claim.' },
      ],
    };
    show(<StudySession cards={[citedCard]} mode="flashcard" title="Biology" onClose={vi.fn()} />);
    await user.click(screen.getByRole('button', { name: 'Reveal answer' }));
    await user.click(screen.getByText('Citations · 2 sources'));
    expect(screen.getByText('A second passage independently supports the verified claim.')).toBeVisible();
  });
  it('generates from selected documents with a separate optional learning goal', async () => {
    const user = userEvent.setup();
    mocks.documents.mockResolvedValue({ ok: true, data: ['biology.md', 'history.txt', 'notes.docx', 'manual.pdf'].map((fileName, i) => ({ id: String(i), fileName, filePath: `/${fileName}` })) });
    mocks.generate.mockResolvedValue({ ok: true, data: { id: 'new-deck' } });
    const created = vi.fn();
    show(<NewStudyDeck onCreated={created} onCancel={vi.fn()} />);
    const biology = await screen.findByRole('checkbox', { name: 'biology.md' });
    expect(screen.getByLabelText(/Learning goal/)).toHaveValue('');
    await user.click(biology);
    await user.click(screen.getByRole('checkbox', { name: 'history.txt' }));
    await user.click(screen.getByRole('checkbox', { name: 'notes.docx' }));
    expect(screen.getByRole('checkbox', { name: 'manual.pdf' })).toBeDisabled();
    await user.type(screen.getByLabelText(/Topic or section/), 'photosynthesis');
    await user.type(screen.getByLabelText(/Learning goal/), 'Biology exam');
    await user.click(screen.getByRole('button', { name: 'Generate deck' }));
    await waitFor(() => expect(created).toHaveBeenCalledWith('new-deck'));
    expect(mocks.generate).toHaveBeenCalledWith({ title: 'New study deck', focus: 'photosynthesis', studyGoal: 'Biology exam', count: 6, documentIds: ['0', '1', '2'] });
  });

  it('retains generation progress and failures across navigation and saves a retried deck while away', async () => {
    const user = userEvent.setup();
    mocks.documents.mockResolvedValue({ ok: true, data: [{ id: 'biology', fileName: 'biology.md', filePath: '/biology.md' }] });
    mocks.decks.mockResolvedValue({ ok: true, data: [] });
    let finish!: (result: unknown) => void;
    mocks.generate.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    show(<MemoryRouter initialEntries={['/study?new=1']}><Link to="/away">Leave Study</Link><Routes>
      <Route path="/study" element={<StudyPage />} />
      <Route path="/away" element={<Link to="/study">Return to Study</Link>} />
    </Routes></MemoryRouter>);
    await user.click(await screen.findByRole('checkbox', { name: 'biology.md' }));
    await user.click(screen.getByRole('button', { name: 'Generate deck' }));
    await user.click(screen.getByRole('link', { name: 'Leave Study' }));
    await user.click(screen.getByRole('link', { name: 'Return to Study' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Generating 6 questions');
    expect(screen.queryByText('Put what you read into practice.')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'New deck' })).toBeDisabled();
    await user.click(screen.getByRole('link', { name: 'Leave Study' }));
    await act(async () => { finish({ ok: false, error: 'Network error', details: { details: 'Server timed out' } }); });
    await user.click(screen.getByRole('link', { name: 'Return to Study' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Server timed out');
    await user.click(screen.getByRole('button', { name: 'Retry generation' }));
    expect(mocks.generate.mock.calls[1][0]).toEqual(mocks.generate.mock.calls[0][0]);
    await user.click(screen.getByRole('link', { name: 'Leave Study' }));
    const saved = { id: 'deck', title: 'New study deck', focus: '', studyGoal: '', modelName: 'test', createdAt: 0, cards: [card] };
    mocks.decks.mockResolvedValue({ ok: true, data: [{ ...saved, cardCount: 1, dueCount: 1, quizAttempts: 0, quizCorrect: 0 }] });
    mocks.deck.mockResolvedValue({ ok: true, data: saved });
    await act(async () => { finish({ ok: true, data: saved }); });
    await user.click(screen.getByRole('link', { name: 'Return to Study' }));
    await user.click(await screen.findByRole('button', { name: /New study deck/ }));
    expect(await screen.findByRole('heading', { name: 'New study deck' })).toBeVisible();
    expect(screen.getByText(card.question)).toBeVisible();
    expect(mocks.generate).toHaveBeenCalledTimes(2);
  });
});
