import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { VOICE_STATUS, VoiceButton } from './VoiceButton';

import type { VoicePhase } from '../model/voiceSession';

describe('VoiceButton', () => {
  it('offers to start a conversation when off', () => {
    const onToggle = vi.fn();
    render(<VoiceButton phase="off" onToggle={onToggle} />);

    const button = screen.getByRole('button', { name: 'Start a voice conversation' });
    expect(button).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(button);
    expect(onToggle).toHaveBeenCalledOnce();
  });

  it.each(Object.keys(VOICE_STATUS) as Array<Exclude<VoicePhase, 'off'>>)('can always be pressed to end it while %s', (phase) => {
    const onToggle = vi.fn();
    render(<VoiceButton phase={phase} onToggle={onToggle} />);

    const button = screen.getByRole('button', { name: 'End the voice conversation' });
    expect(button).toHaveAttribute('aria-pressed', 'true');
    expect(button).toBeEnabled();
    fireEvent.click(button);
    expect(onToggle).toHaveBeenCalledOnce();
  });
});
