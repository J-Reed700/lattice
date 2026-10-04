import { useState } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { LearningMemoryDto, LearningProgramDto, StudyCardDto, WorkspaceNoteDto } from '@/lib/bindings';

import { NotebookPanel, RecallPanel } from './MemoryPanels';
import { useLearningMemory } from './useLearningMemory';

const api = vi.hoisted(() => ({
  getLearningMemory: vi.fn(), ensureLearningLessonNote: vi.fn(), generateLearningCardDrafts: vi.fn(),
  saveLearningCardDraft: vi.fn(), acceptLearningCardDraft: vi.fn(), discardLearningCardDraft: vi.fn(),
  updateWorkspaceNote: vi.fn(), updateStudyCard: vi.fn(), reviewStudyCard: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: api }));
vi.mock('@/components/TiptapEditor', () => ({
  TiptapEditor: ({ value, onChange, placeholder, ariaLabel }: { value: string; onChange: (value: string) => void; placeholder?: string; ariaLabel?: string }) => <textarea aria-label={ariaLabel} value={value} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} />,
}));

const source = { id: 'source-1', title: 'Field guide', url: 'https://example.org/guide', excerpt: 'A source excerpt supporting recall.', acquiredAt: 1_790_000_000_000 };
const extraSource = { id: 'source-2', title: 'Unrelated notes', url: 'https://example.org/other', excerpt: 'A different passage.', acquiredAt: 1_790_000_000_000 };
const lesson = { id: 'lesson-1', title: 'The central idea', objective: 'Explain the central idea', estimatedMinutes: 20, preparation: 'ready' as const, completed: false, blocks: [{ kind: 'explanation' as const, title: 'The idea', body: 'A useful explanation.', sourceIds: ['source-1'] }], questions: [] };
const program: LearningProgramDto = {
  summary: { id: 'program-1', title: 'Reasoning well', goal: 'Learn to reason well.', status: 'active', revision: 1, moduleCount: 1, lessonCount: 1, completedLessons: 0, currentLessonId: lesson.id, createdAt: 1_790_000_000_000 },
  priorKnowledge: '', minutesPerSession: 30, modelName: 'Model', modules: [{ id: 'module-1', title: 'Foundations', summary: 'Start here.', outcomes: ['Explain it'], lessons: [lesson] }], sources: [source], attempts: [],
};
const note: WorkspaceNoteDto = { id: 'note-1', title: 'The central idea', journalId: 'journal-1', content: '', linkedDocumentIds: [], linkedConversationIds: [], highlights: [], stickyNotes: [], conversationSnapshots: [], sources: [], createdAt: '2026-09-30T12:00:00Z', updatedAt: '2026-09-30T12:00:00Z' };
const secondLesson = { ...lesson, id: 'lesson-2', title: 'A second idea', objective: 'Explain another idea' };
const secondNote: WorkspaceNoteDto = { ...note, id: 'note-2', title: secondLesson.title };
const card: StudyCardDto = { id: 'card-1', format: 'question_answer', schedulerVersion: 'expanding_v1', deckId: 'deck-1', question: 'What is the central idea?', answer: 'A practical explanation.', options: [], correctIndex: 0, explanation: 'The source explains why.', source: { chunkId: '', documentId: '', fileName: 'Field guide', filePath: '', excerpt: source.excerpt }, topic: 'The central idea', dueAt: 0, intervalDays: 1, reviewCount: 0, lapses: 0 };
const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });

function memory(overrides: Partial<LearningMemoryDto> = {}): LearningMemoryDto {
  return { programId: program.summary.id, journalId: 'journal-1', lessonNotes: [], studyDeck: null, drafts: [], acceptedCards: [], dueCount: 0, schedulerVersion: 'expanding_v1', ...overrides };
}

function NotebookHarness({ enabled = true, lessonId = lesson.id }: { enabled?: boolean; lessonId?: string }) {
  const query = useLearningMemory(program.summary.id);
  const [drafts, setDrafts] = useState<Record<string, { title: string; content: string }>>({});
  return <NotebookPanel enabled={enabled} program={program} lessonId={lessonId} memory={query.data} memoryLoading={query.isLoading} memoryError={query.error?.message} onRetryMemory={() => void query.refetch()} drafts={drafts} setDraft={(key, draft) => setDrafts((current) => ({ ...current, [key]: draft }))} />;
}

function NotebookLessonSwitchHarness() {
  const query = useLearningMemory(program.summary.id);
  const [activeLessonId, setActiveLessonId] = useState(lesson.id);
  const [drafts, setDrafts] = useState<Record<string, { title: string; content: string }>>({});
  const twoLessonProgram = { ...program, modules: [{ ...program.modules[0], lessons: [lesson, secondLesson] }] };
  return <><button type="button" onClick={() => setActiveLessonId((id) => id === lesson.id ? secondLesson.id : lesson.id)}>Change lesson</button><NotebookPanel enabled program={twoLessonProgram} lessonId={activeLessonId} memory={query.data} memoryLoading={query.isLoading} memoryError={query.error?.message} onRetryMemory={() => void query.refetch()} drafts={drafts} setDraft={(key, draft) => setDrafts((current) => ({ ...current, [key]: draft }))} /></>;
}

function RecallHarness() {
  const query = useLearningMemory(program.summary.id);
  return <RecallPanel program={program} lessonId={lesson.id} memory={query.data} memoryLoading={query.isLoading} memoryError={query.error?.message} onRetryMemory={() => void query.refetch()} />;
}

function mount(element: React.ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  return { ...render(<QueryClientProvider client={client}><MemoryRouter>{element}</MemoryRouter></QueryClientProvider>), client };
}

function JournalLocation() {
  const location = useLocation();
  return <p data-testid="location">{location.pathname}{location.search}</p>;
}

function draft(id = 'draft-1', origin: 'generated' | 'manual' = 'generated') {
  return { id, lessonId: lesson.id, origin, createdAt: 1_790_000_000_000, question: 'How does the idea work?', answer: 'It connects evidence to action.', explanation: 'The lesson source explains the connection.', sourceIds: ['source-1'] };
}

describe('Learning Studio notebook and recall', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.getLearningMemory.mockResolvedValue(ok(memory()));
    api.ensureLearningLessonNote.mockResolvedValue(ok(memory({ lessonNotes: [{ lessonId: lesson.id, note }] })));
    api.updateWorkspaceNote.mockImplementation(async (updated: WorkspaceNoteDto) => ok(updated));
    api.saveLearningCardDraft.mockImplementation(async (request: { draftId: string | null; question: string; answer: string; explanation: string; sourceIds: string[] }) => ok(memory({ drafts: [{ id: request.draftId ?? 'draft-1', lessonId: lesson.id, origin: 'manual', createdAt: Date.now(), question: request.question, answer: request.answer, explanation: request.explanation, sourceIds: request.sourceIds }] })));
    api.acceptLearningCardDraft.mockResolvedValue(ok(memory({ studyDeck: { id: 'deck-1', title: 'Recall', focus: 'Foundations', modelName: 'Model', createdAt: 1, cards: [card] }, acceptedCards: [{ cardId: card.id, lessonId: lesson.id, origin: 'manual', sourceIds: [], acceptedAt: Date.now() }] })));
    api.reviewStudyCard.mockResolvedValue(ok({ ...card, reviewCount: 1, intervalDays: 2, dueAt: Date.now() + 86_400_000 }));
  });

  it('creates the linked Journal page lazily and autosaves notebook edits', async () => {
    const user = userEvent.setup();
    mount(<NotebookHarness />);
    await user.type(await screen.findByLabelText('Your notes'), 'A note worth keeping.');
    await waitFor(() => expect(api.updateWorkspaceNote).toHaveBeenCalledWith(expect.objectContaining({ id: note.id, content: 'A note worth keeping.' })), { timeout: 3000 });
    expect(api.ensureLearningLessonNote).toHaveBeenCalledWith({ programId: program.summary.id, lessonId: lesson.id });
    expect(screen.getByText('Saved to Journal')).toBeVisible();
  });

  it('does not create a page until Notebook is opened and exposes an accessible writing surface', async () => {
    const { rerender, client } = mount(<NotebookHarness enabled={false} />);
    await screen.findByText('Opening lesson notebook…');
    expect(api.ensureLearningLessonNote).not.toHaveBeenCalled();
    rerender(<QueryClientProvider client={client}><MemoryRouter><NotebookHarness /></MemoryRouter></QueryClientProvider>);
    expect(await screen.findByLabelText('Your notes')).toBeVisible();
    expect(api.ensureLearningLessonNote).toHaveBeenCalledTimes(1);
  });

  it('keeps notebook edits on save failure and retries without losing text', async () => {
    const user = userEvent.setup();
    api.updateWorkspaceNote.mockResolvedValueOnce(fail('Journal write failed')).mockImplementationOnce(async (updated: WorkspaceNoteDto) => ok(updated));
    mount(<NotebookHarness />);
    const editor = await screen.findByLabelText('Your notes');
    await user.type(editor, 'This must survive an error.');
    expect(await screen.findByRole('alert')).toHaveTextContent('Journal write failed');
    expect(editor).toHaveValue('This must survive an error.');
    await user.click(screen.getByRole('button', { name: 'Retry save' }));
    await waitFor(() => expect(api.updateWorkspaceNote).toHaveBeenCalledTimes(2));
    expect(api.updateWorkspaceNote.mock.calls[1][0]).toMatchObject({ id: note.id, content: 'This must survive an error.' });
  });

  it('blocks blank notebook titles and flushes pending changes before opening Journal', async () => {
    const user = userEvent.setup();
    mount(<><NotebookHarness /><JournalLocation /></>);
    const editor = await screen.findByLabelText('Your notes');
    const title = screen.getByLabelText('Notebook page title');
    await user.clear(title);
    await user.type(editor, 'Unsaved until navigation.');
    expect(screen.getByRole('button', { name: 'Open full Journal' })).toBeDisabled();
    expect(api.updateWorkspaceNote).not.toHaveBeenCalled();
    await user.type(title, 'A useful title');
    expect(screen.getByRole('button', { name: 'Open full Journal' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: 'Open full Journal' }));
    await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent('/journals?noteId=note-1&journalSpaceId=journal-1'));
    expect(api.updateWorkspaceNote).toHaveBeenCalledWith(expect.objectContaining({ title: 'A useful title', content: 'Unsaved until navigation.' }));
  });

  it('preserves notebook text while switching lessons and autosaves when returning', async () => {
    const user = userEvent.setup();
    api.getLearningMemory.mockResolvedValue(ok(memory({ lessonNotes: [{ lessonId: lesson.id, note }, { lessonId: secondLesson.id, note: secondNote }] })));
    mount(<NotebookLessonSwitchHarness />);
    const editor = await screen.findByLabelText('Your notes');
    await user.type(editor, 'Keep this thought when I browse away.');
    await user.click(screen.getByRole('button', { name: 'Change lesson' }));
    expect(screen.getByLabelText('Notebook page title')).toHaveValue(secondLesson.title);
    await user.click(screen.getByRole('button', { name: 'Change lesson' }));
    expect(screen.getByLabelText('Your notes')).toHaveValue('Keep this thought when I browse away.');
    await waitFor(() => expect(api.updateWorkspaceNote).toHaveBeenCalledWith(expect.objectContaining({ id: note.id, content: 'Keep this thought when I browse away.' })), { timeout: 3000 });
  });

  it('keeps the notebook title, editor and Journal action keyboard accessible', async () => {
    const user = userEvent.setup();
    mount(<NotebookHarness />);
    await screen.findByLabelText('Notebook page title');
    await user.tab();
    expect(screen.getByRole('button', { name: 'Open full Journal' })).toHaveFocus();
    await user.tab();
    expect(screen.getByLabelText('Notebook page title')).toHaveFocus();
    await user.tab();
    expect(screen.getByLabelText('Your notes')).toHaveFocus();
  });

  it('edits and accepts a manual recall card draft', async () => {
    const user = userEvent.setup();
    mount(<RecallHarness />);
    await user.click(await screen.findByRole('button', { name: 'Create manually' }));
    await user.type(screen.getByLabelText('Question'), 'What should I remember?');
    await user.type(screen.getByLabelText('Answer'), 'A grounded answer.');
    await user.type(screen.getByLabelText('Why this answer works'), 'It follows the source.');
    await user.click(screen.getByRole('checkbox', { name: /Field guide/ }));
    await user.click(screen.getByRole('button', { name: 'Save card draft' }));
    await waitFor(() => expect(api.saveLearningCardDraft).toHaveBeenCalledWith({ draftId: null, programId: program.summary.id, lessonId: lesson.id, question: 'What should I remember?', answer: 'A grounded answer.', explanation: 'It follows the source.', sourceIds: ['source-1'] }));
    await user.clear(await screen.findByLabelText('Question'));
    await user.type(screen.getByLabelText('Question'), 'What is the revised question?');
    await user.click(screen.getByRole('button', { name: 'Save draft edits' }));
    await waitFor(() => expect(api.saveLearningCardDraft).toHaveBeenLastCalledWith(expect.objectContaining({ draftId: 'draft-1', question: 'What is the revised question?' })));
    await user.click(screen.getByRole('button', { name: 'Accept card' }));
    await waitFor(() => expect(api.acceptLearningCardDraft).toHaveBeenCalledWith({ programId: program.summary.id, draftId: 'draft-1' }));
  });

  it('restricts generated sources to lesson evidence while manual drafts can use any or no source', async () => {
    const user = userEvent.setup();
    const withExtraSource = { ...program, sources: [...program.sources, extraSource] };
    mount(<RecallPanel program={withExtraSource} lessonId={lesson.id} memory={memory({ drafts: [draft()] })} memoryLoading={false} onRetryMemory={() => undefined} />);
    const generatedCard = screen.getByLabelText('Question', { selector: 'textarea' });
    expect(generatedCard).toHaveValue('How does the idea work?');
    expect(screen.getByRole('checkbox', { name: 'Field guide' })).toBeChecked();
    expect(screen.queryByRole('checkbox', { name: 'Unrelated notes' })).not.toBeInTheDocument();
    await user.click(screen.getAllByRole('checkbox', { name: 'Field guide' })[0]);
    expect(screen.getByRole('button', { name: 'Save draft edits' })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: 'Create manually' }));
    expect(screen.getAllByRole('checkbox', { name: 'Field guide' })).toHaveLength(2);
    expect(screen.getByRole('checkbox', { name: 'Unrelated notes' })).toBeVisible();
    expect(screen.getByText('Select excerpts that support this card, or leave it as a personal mnemonic.')).toBeVisible();
  });

  it('saves generated topic-only recall without requesting an external source', async () => {
    const user = userEvent.setup();
    const topicProgram = structuredClone(program);
    topicProgram.sources = [];
    topicProgram.modules[0].lessons[0].blocks[0].sourceIds = [];
    const topicDraft = { ...draft(), sourceIds: [] };
    mount(<RecallPanel program={topicProgram} lessonId={lesson.id} memory={memory({ drafts: [topicDraft] })} memoryLoading={false} onRetryMemory={() => undefined} />);
    expect(screen.getByText('This card is based on the prepared lesson. No external source is required.')).toBeVisible();
    const save = screen.getByRole('button', { name: 'Save draft edits' });
    expect(save).toBeEnabled();
    await user.click(save);
    await waitFor(() => expect(api.saveLearningCardDraft).toHaveBeenCalledWith(expect.objectContaining({ draftId: topicDraft.id, sourceIds: [] })));
  });

  it('gates generating and manually creating drafts until the program is active and the lesson is ready', () => {
    const outline = structuredClone(program);
    outline.modules[0].lessons[0].preparation = 'outline';
    outline.modules[0].lessons[0].blocks = [];
    const draftProgram = structuredClone(outline);
    draftProgram.summary.status = 'draft';
    draftProgram.summary.revision = 0;
    const { rerender, client } = mount(<RecallPanel program={outline} lessonId={lesson.id} memory={memory()} memoryLoading={false} onRetryMemory={() => undefined} />);
    expect(screen.getByRole('button', { name: 'Generate drafts' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Create manually' })).toBeDisabled();
    rerender(<QueryClientProvider client={client}><MemoryRouter><RecallPanel program={draftProgram} lessonId={lesson.id} memory={memory()} memoryLoading={false} onRetryMemory={() => undefined} /></MemoryRouter></QueryClientProvider>);
    expect(screen.getByText('Accept the program before generating cards.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Generate drafts' })).toBeDisabled();
  });

  it('keeps accept and discard actions available after recoverable failures', async () => {
    const user = userEvent.setup();
    const cardDraft = draft();
    api.acceptLearningCardDraft.mockResolvedValueOnce(fail('Accept failed')).mockResolvedValueOnce(ok(memory()));
    api.discardLearningCardDraft.mockResolvedValueOnce(fail('Discard failed')).mockResolvedValueOnce(ok(memory()));
    mount(<RecallPanel program={program} lessonId={lesson.id} memory={memory({ drafts: [cardDraft] })} memoryLoading={false} onRetryMemory={() => undefined} />);
    const accept = await screen.findByRole('button', { name: 'Accept card' });
    await user.click(accept);
    expect(await screen.findByRole('alert')).toHaveTextContent('Accept failed');
    expect(accept).toBeEnabled();
    await user.click(accept);
    await waitFor(() => expect(api.acceptLearningCardDraft).toHaveBeenCalledTimes(2));

    const discard = screen.getByRole('button', { name: 'Discard' });
    await user.click(discard);
    expect(await screen.findByRole('alert')).toHaveTextContent('Discard failed');
    expect(discard).toBeEnabled();
    await user.click(discard);
    await waitFor(() => expect(api.discardLearningCardDraft).toHaveBeenCalledTimes(2));
  });

  it('hides accepted answers outside explicit editing', async () => {
    const user = userEvent.setup();
    const notDue = { ...card, dueAt: Date.now() + 86_400_000 };
    mount(<RecallPanel program={program} lessonId={lesson.id} memory={memory({ studyDeck: { id: 'deck-1', title: 'Recall', focus: 'Foundations', modelName: 'Model', createdAt: 1, cards: [notDue] }, acceptedCards: [{ cardId: card.id, lessonId: lesson.id, origin: 'generated', sourceIds: ['source-1'], acceptedAt: Date.now() }] })} memoryLoading={false} onRetryMemory={() => undefined} />);
    expect(screen.queryByText(card.answer)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: `Edit accepted card: ${card.question}` }));
    expect(screen.getByLabelText('Answer')).toHaveValue(card.answer);
    expect(screen.getByRole('button', { name: 'Cancel card edit' })).toBeEnabled();
  });

  it('keeps an accepted-card save error attached to the card that failed', async () => {
    const user = userEvent.setup();
    const secondCard = { ...card, id: 'card-2', question: 'What is the second idea?' };
    api.updateStudyCard.mockResolvedValue(fail('Card update failed'));
    mount(<RecallPanel program={program} lessonId={lesson.id} memory={memory({
      studyDeck: { id: 'deck-1', title: 'Recall', focus: 'Foundations', modelName: 'Model', createdAt: 1, cards: [{ ...card, dueAt: Date.now() + 86_400_000 }, { ...secondCard, dueAt: Date.now() + 86_400_000 }] },
      acceptedCards: [
        { cardId: card.id, lessonId: lesson.id, origin: 'generated', sourceIds: ['source-1'], acceptedAt: Date.now() },
        { cardId: secondCard.id, lessonId: lesson.id, origin: 'generated', sourceIds: ['source-1'], acceptedAt: Date.now() },
      ],
    })} memoryLoading={false} onRetryMemory={() => undefined} />);
    await user.click(screen.getByRole('button', { name: `Edit accepted card: ${card.question}` }));
    await user.click(screen.getByRole('button', { name: `Edit accepted card: ${secondCard.question}` }));
    const firstForm = screen.getByDisplayValue(card.question).closest('form');
    const secondForm = screen.getByDisplayValue(secondCard.question).closest('form');
    expect(firstForm).not.toBeNull();
    expect(secondForm).not.toBeNull();
    await user.click(within(secondForm as HTMLFormElement).getByRole('button', { name: 'Save card' }));
    expect(await within(secondForm as HTMLFormElement).findByRole('alert')).toHaveTextContent('Card update failed');
    expect(within(firstForm as HTMLFormElement).queryByRole('alert')).not.toBeInTheDocument();
  });

  it('retries draft generation after a recoverable error', async () => {
    const user = userEvent.setup();
    api.generateLearningCardDrafts.mockResolvedValueOnce(fail('Generator unavailable')).mockResolvedValueOnce(ok(memory({ drafts: [draft()] })));
    mount(<RecallHarness />);
    await user.click(await screen.findByRole('button', { name: 'Generate drafts' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Generator unavailable');
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByDisplayValue('How does the idea work?')).toBeVisible();
    expect(api.generateLearningCardDrafts).toHaveBeenCalledTimes(2);
  });

  it('keeps draft text after save failure and retries the saved request', async () => {
    const user = userEvent.setup();
    api.saveLearningCardDraft.mockResolvedValueOnce(fail('Draft could not be saved')).mockImplementationOnce(async (request: { draftId: string | null; question: string; answer: string; explanation: string; sourceIds: string[] }) => ok(memory({ drafts: [{ id: 'draft-1', lessonId: lesson.id, origin: 'manual', createdAt: 1_790_000_000_000, question: request.question, answer: request.answer, explanation: request.explanation, sourceIds: request.sourceIds }] })));
    mount(<RecallHarness />);
    await user.click(await screen.findByRole('button', { name: 'Create manually' }));
    await user.type(screen.getByLabelText('Question'), 'Manual question');
    await user.type(screen.getByLabelText('Answer'), 'Manual answer');
    await user.type(screen.getByLabelText('Why this answer works'), 'A clear reason');
    await user.click(screen.getByRole('button', { name: 'Save card draft' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Draft could not be saved');
    expect(screen.getAllByRole('alert')).toHaveLength(1);
    expect(screen.getByLabelText('Question')).toHaveValue('Manual question');
    await user.click(screen.getByRole('button', { name: 'Save card draft' }));
    await waitFor(() => expect(api.saveLearningCardDraft).toHaveBeenCalledTimes(2));
    expect(api.saveLearningCardDraft.mock.calls[1][0]).toMatchObject({ question: 'Manual question', answer: 'Manual answer', explanation: 'A clear reason' });
  });

  it('keeps a due-card answer and provenance hidden until reveal, then retries with the same review id', async () => {
    const user = userEvent.setup();
    api.reviewStudyCard.mockResolvedValueOnce(fail('Connection interrupted')).mockResolvedValueOnce(ok({ ...card, reviewCount: 1, intervalDays: 2, dueAt: Date.now() + 86_400_000 }));
    mount(<RecallPanel program={program} lessonId={lesson.id} memory={memory({ studyDeck: { id: 'deck-1', title: 'Recall', focus: 'Foundations', modelName: 'Model', createdAt: 1, cards: [card] }, acceptedCards: [{ cardId: card.id, lessonId: lesson.id, origin: 'generated', sourceIds: ['source-1'], acceptedAt: Date.now() }], dueCount: 1 })} memoryLoading={false} onRetryMemory={() => undefined} />);
    expect((await screen.findAllByText(card.question))[0]).toBeVisible();
    expect(screen.queryByText(card.answer)).not.toBeInTheDocument();
    expect(screen.queryByText(source.excerpt)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Reveal answer' }));
    expect(screen.getByText(card.answer)).toBeVisible();
    await user.click(screen.getByRole('button', { name: /again/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Connection interrupted');
    await user.click(screen.getByText(source.title));
    expect(screen.getByText(source.excerpt)).toBeVisible();
    const firstReviewId = api.reviewStudyCard.mock.calls[0][0].reviewId;
    await user.click(screen.getByRole('button', { name: /again/i }));
    await waitFor(() => expect(api.reviewStudyCard).toHaveBeenCalledTimes(2));
    expect(api.reviewStudyCard.mock.calls[1][0].reviewId).toBe(firstReviewId);
    expect(screen.getByText('0 due · 1 total')).toBeVisible();
    expect(within(screen.getByText('Review saved. Next interval: 2 days.').parentElement as HTMLElement).getByRole('status')).toBeVisible();
  });

  it('creates a new review id when the learner changes their rating', async () => {
    const user = userEvent.setup();
    api.reviewStudyCard.mockResolvedValueOnce(fail('Try a different rating')).mockResolvedValueOnce(ok({ ...card, reviewCount: 1, intervalDays: 2, dueAt: Date.now() + 86_400_000 }));
    mount(<RecallPanel program={program} lessonId={lesson.id} memory={memory({ studyDeck: { id: 'deck-1', title: 'Recall', focus: 'Foundations', modelName: 'Model', createdAt: 1, cards: [card] }, dueCount: 1 })} memoryLoading={false} onRetryMemory={() => undefined} />);
    await user.click(await screen.findByRole('button', { name: 'Reveal answer' }));
    await user.click(screen.getByRole('button', { name: /again/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Try a different rating');
    const firstId = api.reviewStudyCard.mock.calls[0][0].reviewId;
    await user.click(screen.getByRole('button', { name: /hard/i }));
    await waitFor(() => expect(api.reviewStudyCard).toHaveBeenCalledTimes(2));
    expect(api.reviewStudyCard.mock.calls[1][0].rating).toBe('hard');
    expect(api.reviewStudyCard.mock.calls[1][0].reviewId).not.toBe(firstId);
  });
});
