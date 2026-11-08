import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { VoicePhase } from '@/features/voice/model/voiceSession';
import { VOICE_STATUS } from '@/features/voice/components/VoiceButton';

import {
  PracticeThread,
  type PracticeThreadProps,
  type PracticeThreadTurn,
} from './PracticeThread';

const voiceState = {
  phase: 'off' as VoicePhase,
  toggle: vi.fn(),
};

vi.mock('@/features/voice/hooks/useVoiceConversation', () => ({
  useVoiceConversation: () => voiceState,
}));

const turns: PracticeThreadTurn[] = [
  {
    id: 'turn-1',
    speaker: 'coach',
    speakerLabel: 'Coach',
    content: 'Explain your reasoning for choosing this algorithm.',
    badge: 'Stage 1',
  },
  {
    id: 'turn-2',
    speaker: 'learner',
    speakerLabel: 'You',
    content: 'We can use a hash set to achieve O(n) lookup time.',
  },
  {
    id: 'turn-3',
    speaker: 'counterpart',
    speakerLabel: 'Interviewer',
    content: 'What happens if there are duplicate elements?',
  },
];

function renderThread(props: Partial<PracticeThreadProps> = {}) {
  const onSend = vi.fn();
  const defaultProps: PracticeThreadProps = {
    conversationId: 'conv-1',
    turns,
    draftKey: 'test-thread-draft',
    onSend,
    ...props,
  };
  const view = render(<PracticeThread {...defaultProps} />);
  return { ...view, onSend };
}

describe('PracticeThread', () => {
  beforeEach(() => {
    voiceState.phase = 'off';
    voiceState.toggle.mockReset();
  });

  it('renders transcript turns with speaker labels and content', () => {
    renderThread();

    expect(screen.getByText('Coach')).toBeInTheDocument();
    expect(screen.getByText('Explain your reasoning for choosing this algorithm.')).toBeInTheDocument();
    expect(screen.getByText('You')).toBeInTheDocument();
    expect(screen.getByText('We can use a hash set to achieve O(n) lookup time.')).toBeInTheDocument();
    expect(screen.getByText('Interviewer')).toBeInTheDocument();
    expect(screen.getByText('What happens if there are duplicate elements?')).toBeInTheDocument();
  });

  it('renders emptyText when there are no transcript turns', () => {
    renderThread({ turns: [], emptyText: 'No messages in this session yet.' });
    expect(screen.getByText('No messages in this session yet.')).toBeInTheDocument();
  });

  it('submits on send button click', () => {
    const { onSend } = renderThread();
    const textarea = screen.getByRole('textbox');

    fireEvent.change(textarea, { target: { value: 'A duplicate would already exist in the set.' } });
    const sendButton = screen.getByRole('button', { name: /send/i });
    fireEvent.click(sendButton);

    expect(onSend).toHaveBeenCalledWith('A duplicate would already exist in the set.', false);
  });

  it('submits on Ctrl+Enter', () => {
    const { onSend } = renderThread();
    const textarea = screen.getByRole('textbox');

    fireEvent.change(textarea, { target: { value: 'We can handle duplicates by counting frequencies.' } });
    fireEvent.keyDown(textarea, { key: 'Enter', ctrlKey: true });

    expect(onSend).toHaveBeenCalledWith('We can handle duplicates by counting frequencies.', false);
  });

  it('marks textarea readOnly and aria-busy while busy, and restores focus when reply completes', () => {
    const { rerender } = renderThread({ busyLabel: 'Reading your code…' });
    const textarea = screen.getByRole('textbox');

    // While busy, textarea must be readOnly (not disabled) and marked aria-busy
    expect(textarea).toHaveAttribute('readonly');
    expect(textarea).toHaveAttribute('aria-busy', 'true');
    expect(textarea).not.toBeDisabled();

    // When reply completes (busyLabel cleared), focus is restored to the textarea
    rerender(
      <PracticeThread
        conversationId="conv-1"
        turns={turns}
        draftKey="test-thread-draft"
        onSend={vi.fn()}
        busyLabel={null}
      />,
    );

    expect(textarea).not.toHaveAttribute('readonly');
    expect(textarea).toHaveAttribute('aria-busy', 'false');
    expect(document.activeElement).toBe(textarea);
  });

  it('shows StatusLine busy text and error states', () => {
    const onRetry = vi.fn();
    const { rerender } = renderThread({ busyLabel: 'Interviewer is thinking…' });

    expect(screen.getByRole('status')).toHaveTextContent('Interviewer is thinking…');

    rerender(
      <PracticeThread
        conversationId="conv-1"
        turns={turns}
        draftKey="test-thread-draft"
        onSend={vi.fn()}
        error="Turn failed to send."
        onRetry={onRetry}
      />,
    );

    expect(screen.getByRole('alert')).toHaveTextContent('Turn failed to send.');
    const retryBtn = screen.getByRole('button', { name: /try again/i });
    fireEvent.click(retryBtn);
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('mounts VoiceButton and shows VOICE_STATUS.listening in place of shortcut hint when voice is active', () => {
    // With voice off, shortcut hint "Ctrl+Enter" is displayed
    const { rerender } = renderThread();
    expect(screen.getByText(/Ctrl\+Enter/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /start a voice conversation/i })).toBeInTheDocument();

    // Switch voice state to listening
    voiceState.phase = 'listening';
    rerender(
      <PracticeThread
        conversationId="conv-1"
        turns={turns}
        draftKey="test-thread-draft"
        onSend={vi.fn()}
      />,
    );

    // Shortcut hint is replaced by VOICE_STATUS.listening ("Listening…")
    expect(screen.getByText(VOICE_STATUS.listening)).toBeInTheDocument();
    expect(screen.queryByText(/Ctrl\+Enter/i)).not.toBeInTheDocument();
  });

  it('shows disabledReason when disabled', () => {
    renderThread({
      disabled: true,
      disabledReason: 'Complete the examples stage before responding.',
    });

    const sendButton = screen.getByRole('button', { name: /send/i });
    expect(sendButton).toBeDisabled();
    expect(screen.getByText('Complete the examples stage before responding.')).toBeInTheDocument();
  });
});
