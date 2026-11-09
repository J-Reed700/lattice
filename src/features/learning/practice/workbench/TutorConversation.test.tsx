import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { setDraft } from '@/shared/drafts/drafts';

import { TutorConversation, type TutorConversationProps } from './TutorConversation';

const thread = vi.hoisted(() => ({ props: null as null | Record<string, any> }));
vi.mock('@/features/learning/practice/shared/PracticeThread', () => ({
  PracticeThread: (props: Record<string, any>) => {
    thread.props = props;
    return <div role="region" aria-label={props.label} />;
  },
}));

const turn = (overrides: Record<string, unknown>) => ({
  id: 'turn-1', prompt: 'Why?', response: 'Because.', requestKind: 'question', hintLevel: null, citations: [], proposalIds: [], modelName: 'Study model', createdAt: 1, ...overrides,
});

function setup(overrides: Partial<TutorConversationProps> = {}) {
  const props = {
    sessionId: 'attempt-1', turns: [], mode: 'explore', active: true, canTutor: true, busyState: 'idle', busy: false, hintUsed: [],
    showCitations: false, sourceOpeningAllowed: true, onAsk: vi.fn(async () => true), onHint: vi.fn(async () => true), onOpenCitation: vi.fn(),
    ...overrides,
  } as TutorConversationProps;
  render(<TutorConversation {...props} />);
  return props;
}

describe('TutorConversation', () => {
  beforeEach(() => {
    thread.props = null;
    setDraft('practice-tutor:attempt-1', '');
  });

  it('hands the thread the learner questions and tutor replies, and none of the canned hint requests', () => {
    setup({ turns: [turn({ id: 'a' }), turn({ id: 'b', requestKind: 'hint', hintLevel: 'orienting_question', prompt: 'Give me an orienting question' })] as never });
    expect(thread.props?.messages.map((m: { speaker: string }) => m.speaker)).toEqual(['learner', 'coach', 'coach']);
    expect(thread.props?.label).toBe('Tutor messages');
    expect(thread.props?.draftKey).toBe('practice-tutor:attempt-1');
  });

  it('names the tutor box for the thread', () => {
    setup();
    expect(thread.props?.inputLabel).toBe('Ask a question or request critique');
  });

  it('names the busy tutor to the thread', () => {
    setup({ busy: true, busyState: 'tutor' });
    expect(thread.props?.busy).toBe('The tutor is responding…');
  });

  it('closes the thread with its reason when the mode forbids the tutor', () => {
    setup({ canTutor: false, mode: 'demonstrate' });
    expect(thread.props?.disabled).toEqual({ reason: 'The tutor is closed with No help.' });
  });

  it('sends a question from the thread', async () => {
    const props = setup();
    await thread.props?.onSend('How should I state the limit?', false);
    expect(props.onAsk).toHaveBeenCalledWith('question', 'How should I state the limit?');
  });

  it('rejects the send when the ask fails, so the thread keeps the question', async () => {
    setup({ onAsk: vi.fn(async () => false) });
    await expect(thread.props?.onSend('How should I state the limit?', false)).rejects.toThrow();
  });

  it('shows a failed tutor request in the thread, with its retry', () => {
    const retry = vi.fn();
    setup({ error: { message: 'The tutor could not answer.', retry } });
    expect(thread.props?.error).toEqual({ message: 'The tutor could not answer.', retry });
  });

  it('asks for a critique of the unsent text, and disables it with a reason when there is none', async () => {
    const props = setup();
    expect(screen.getByRole('button', { name: 'Request critique' })).toBeDisabled();
    expect(screen.getByText('Write your question first to request a critique.')).toBeVisible();
    setDraft('practice-tutor:attempt-1', 'Check my reasoning');
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Request critique' }));
    expect(props.onAsk).toHaveBeenCalledWith('critique', 'Check my reasoning');
  });

  it('lists the four hints, marks used ones, and locks the worked explanation outside Explore with its reason', async () => {
    const props = setup({ mode: 'practice', hintUsed: ['orienting_question'] });
    expect(screen.getByRole('button', { name: /Orienting question/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Worked explanation/ })).toBeDisabled();
    expect(screen.getByText('Available with Full help')).toBeVisible();
    await userEvent.setup().click(screen.getByRole('button', { name: /Concept or source/ }));
    expect(props.onHint).toHaveBeenCalledWith(expect.objectContaining({ id: 'concept_or_source' }));
  });
});
