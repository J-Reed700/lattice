import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VOICE_STATUS } from '@/features/voice/components/VoiceButton';
import type { VoicePhase } from '@/features/voice/model/voiceSession';

import { PracticeThread, type PracticeThreadProps, type ThreadMessage } from './PracticeThread';

const voice = { phase: 'off' as VoicePhase, toggle: vi.fn() };
const voiceOptions: { send?: (text: string) => Promise<unknown>; reply?: () => string } = {};

vi.mock('@/features/voice/hooks/useVoiceConversation', () => ({
  useVoiceConversation: (options: { send: (text: string) => Promise<unknown>; reply?: () => string }) => {
    voiceOptions.send = options.send;
    voiceOptions.reply = options.reply;
    return voice;
  },
}));

const messages: ThreadMessage[] = [
  { id: 'm1', speaker: 'counterpart', content: 'What happens with duplicates?' },
  { id: 'm2', speaker: 'learner', content: 'I would count them.' },
  { id: 'm3', speaker: 'coach', content: 'Look at lines 3 to 5.', lines: [3, 4, 5] },
];

function renderThread(props: Partial<PracticeThreadProps> = {}) {
  const onSend = vi.fn(async () => {});
  const view = render(
    <PracticeThread label="Interview" messages={messages} onSend={onSend} draftKey="thread-test" {...props} />,
  );
  return { ...view, onSend };
}

describe('PracticeThread', () => {
  beforeEach(() => {
    voice.phase = 'off';
    voice.toggle.mockReset();
    window.localStorage.clear();
  });

  it('names the transcript and lists each message', () => {
    renderThread();
    const log = screen.getByRole('log', { name: 'Interview' });
    expect(log).toHaveTextContent('What happens with duplicates?');
    expect(log).toHaveTextContent('I would count them.');
    expect(log).toHaveTextContent(/lines 3/i);
  });

  it('sends on Ctrl+Enter with spoken false, and keeps the focus in the box', async () => {
    const { onSend } = renderThread();
    const box = screen.getByRole('textbox', { name: 'Your response' });
    fireEvent.change(box, { target: { value: 'Use a set.' } });
    await act(async () => {
      fireEvent.keyDown(box, { key: 'Enter', ctrlKey: true });
    });
    expect(onSend).toHaveBeenCalledWith('Use a set.', false);
    expect(box).toHaveValue('');
    expect(document.activeElement).toBe(box);
  });

  it('names the box by inputLabel, and Your response by default', () => {
    renderThread({ inputLabel: 'Ask a question or request critique' });
    expect(screen.getByRole('textbox', { name: 'Ask a question or request critique' })).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: 'Your response' })).not.toBeInTheDocument();
  });

  it('calls the box Your response when no inputLabel is given', () => {
    renderThread();
    expect(screen.getByRole('textbox', { name: 'Your response' })).toBeInTheDocument();
  });

  it('shows Ctrl+Enter beside the box', () => {
    renderThread();
    expect(screen.getByText(/Ctrl\+Enter/)).toBeInTheDocument();
  });

  it('makes the box read-only with aria-busy and shows the busy line, then refocuses when the reply lands', () => {
    const { rerender, onSend } = renderThread({ busy: 'Reading your code…' });
    const box = screen.getByRole('textbox', { name: 'Your response' });
    expect(box).toHaveAttribute('readonly');
    expect(box).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByRole('status')).toHaveTextContent('Reading your code…');
    rerender(
      <PracticeThread label="Interview" messages={messages} onSend={onSend} draftKey="thread-test" busy={null} />,
    );
    expect(box).not.toHaveAttribute('readonly');
    expect(document.activeElement).toBe(box);
  });

  it('shows an error with Try again', () => {
    const retry = vi.fn();
    renderThread({ error: { message: 'The reply failed.', retry } });
    expect(screen.getByRole('alert')).toHaveTextContent('The reply failed.');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it('disables the box and Send and says why beneath', () => {
    renderThread({ disabled: { reason: 'Finish the examples stage first.' } });
    expect(screen.getByRole('textbox', { name: 'Your response' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    expect(screen.getByText('Finish the examples stage first.')).toBeInTheDocument();
  });

  it('has the mic, and VOICE_STATUS takes the shortcut hint\'s place', () => {
    const { rerender, onSend } = renderThread();
    expect(screen.getByRole('button', { name: /start a voice conversation/i })).toBeInTheDocument();
    voice.phase = 'listening';
    rerender(
      <PracticeThread label="Interview" messages={messages} onSend={onSend} draftKey="thread-test" />,
    );
    expect(screen.getByText(VOICE_STATUS.listening)).toBeInTheDocument();
    expect(screen.queryByText(/Ctrl\+Enter/)).not.toBeInTheDocument();
  });

  it('has no mic when voice is off', () => {
    renderThread({ voice: false });
    expect(screen.queryByRole('button', { name: /voice conversation/i })).not.toBeInTheDocument();
  });

  it('answers a spoken turn only when the reply arrives, and gives the hook its text', async () => {
    const { onSend, rerender } = renderThread();
    let outcome: unknown;
    await act(async () => {
      void voiceOptions.send?.('I said this.').then((value) => {
        outcome = value;
      });
    });
    expect(onSend).toHaveBeenCalledWith('I said this.', true);
    expect(outcome).toBeUndefined();
    await act(async () => {
      rerender(
        <PracticeThread
          label="Interview"
          messages={[...messages, { id: 'm4', speaker: 'counterpart', content: 'And the cost?' }]}
          onSend={onSend}
          draftKey="thread-test"
        />,
      );
    });
    expect(outcome).toBe('answered');
    expect(voiceOptions.reply?.()).toBe('And the cost?');
  });

  it('fails a spoken turn when an error appears while waiting for the reply', async () => {
    const { onSend, rerender } = renderThread();
    let outcome: unknown;
    await act(async () => {
      void voiceOptions.send?.('I said this.').then((value) => {
        outcome = value;
      });
    });
    await act(async () => {
      rerender(
        <PracticeThread
          label="Interview"
          messages={messages}
          onSend={onSend}
          draftKey="thread-test"
          error={{ message: 'The reply failed.' }}
        />,
      );
    });
    expect(outcome).toBe('failed');
  });

  it('keeps waiting when an error that was already showing is rebuilt during the send', async () => {
    const shown = { message: 'Earlier failure.' };
    const { onSend, rerender } = renderThread({ error: shown });
    let outcome: unknown;
    await act(async () => {
      void voiceOptions.send?.('I said this.').then((value) => {
        outcome = value;
      });
    });
    await act(async () => {
      rerender(
        <PracticeThread label="Interview" messages={messages} onSend={onSend} draftKey="thread-test" error={{ message: 'Earlier failure.' }} />,
      );
    });
    expect(outcome).toBeUndefined();
    await act(async () => {
      rerender(
        <PracticeThread
          label="Interview"
          messages={[...messages, { id: 'm4', speaker: 'counterpart', content: 'And the cost?' }]}
          onSend={onSend}
          draftKey="thread-test"
          error={{ message: 'Earlier failure.' }}
        />,
      );
    });
    expect(outcome).toBe('answered');
  });

  it('puts the words back in the box and fails when a spoken send is rejected', async () => {
    const { onSend } = renderThread();
    onSend.mockRejectedValueOnce(new Error('offline'));
    let outcome: unknown;
    await act(async () => {
      outcome = await voiceOptions.send?.('Lost words.');
    });
    expect(outcome).toBe('failed');
    expect(screen.getByRole('textbox', { name: 'Your response' })).toHaveValue('Lost words.');
  });

  it('shows the suggestion', () => {
    renderThread({ suggestion: 'Talk through your thinking: type or use the mic' });
    expect(screen.getByText('Talk through your thinking: type or use the mic')).toBeInTheDocument();
  });
});
