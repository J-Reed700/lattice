import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { VOICE_STATUS } from '@/features/voice/components/VoiceButton';
import type { VoicePhase } from '@/features/voice/model/voiceSession';

import { PracticeThread, type PracticeThreadProps, type ThreadMessage } from './PracticeThread';

const voice = { phase: 'off' as VoicePhase, toggle: vi.fn() };
const voiceOptions: { send?: (text: string) => Promise<unknown> } = {};

vi.mock('@/features/voice/hooks/useVoiceConversation', () => ({
  useVoiceConversation: (options: { send: (text: string) => Promise<unknown> }) => {
    voiceOptions.send = options.send;
    return voice;
  },
}));
vi.mock('@/features/speech/hooks/useSpeech', () => ({ useSpeaker: () => vi.fn() }));

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

  it('sends what was said to onSend as a spoken turn', async () => {
    const { onSend } = renderThread();
    await act(async () => {
      await voiceOptions.send?.('I said this.');
    });
    expect(onSend).toHaveBeenCalledWith('I said this.', true);
  });

  it('shows the suggestion', () => {
    renderThread({ suggestion: 'Talk through your thinking: type or use the mic' });
    expect(screen.getByText('Talk through your thinking: type or use the mic')).toBeInTheDocument();
  });
});
