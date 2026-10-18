import { createRef } from 'react';

import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ChatModelButton, type ChatModelButtonProps } from '@/features/chat/components/composer/ChatModelButton';
import type { ChatModelOption } from '@/utils/chatModelSelection';

vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ downloadedModels: [] }) }));
vi.mock('@/features/model/hooks/useLoadedModels', () => ({ useLoadedModels: () => [], degradedReasonFor: () => null }));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({ useSettingsQuery: () => ({ data: undefined }) }));

const option = (label: string, overrides: Partial<ChatModelOption> = {}): ChatModelOption => ({
  id: label, ref: { kind: 'local', model_id: label }, label, triggerLabel: label, subtitle: null,
  group: 'Local', backend: 'local', contextLabel: null, supportsTools: null, seesImages: null, isDefault: false, ...overrides,
});

const draw = (props: Partial<ChatModelButtonProps>) =>
  render(
    <ChatModelButton
      choice={{ kind: 'default' }}
      effective={null}
      defaultOption={null}
      degraded={null}
      canChoose
      onSelect={vi.fn()}
      onUseDefault={vi.fn()}
      triggerRef={createRef<HTMLButtonElement>()}
      {...props}
    />
  );

describe('ChatModelButton', () => {
  it('says a chat on the default follows the app default', () => {
    const qwen = option('Qwen3.5-27B', { isDefault: true });
    draw({ effective: qwen, defaultOption: qwen });
    const button = screen.getByRole('button', { name: 'Model for this chat: Qwen3.5-27B (app default).' });
    expect(button).toHaveTextContent('Default · Qwen3.5-27B');
  });

  it("says a chat's own model is for this chat", () => {
    draw({
      choice: { kind: 'explicit', model: { kind: 'local', model_id: 'Ornith-1.5-9B' } },
      effective: option('Ornith-1.5-9B'),
    });
    const button = screen.getByRole('button', { name: 'Model for this chat: Ornith-1.5-9B (chosen for this chat).' });
    expect(button).toHaveTextContent('Ornith-1.5-9Bfor this chat');
    expect(button).not.toHaveTextContent('Default');
  });

  it('says in words that a chosen model is unavailable, and why', () => {
    const reason = 'This chat is set to Ornith-1.5-9B, which is no longer downloaded.';
    draw({
      choice: { kind: 'explicit', model: { kind: 'local', model_id: 'ornith' } },
      effective: { unavailable: true, label: 'Ornith-1.5-9B', reason },
    });
    const button = screen.getByRole('button', {
      name: `Model for this chat: Ornith-1.5-9B (chosen for this chat, unavailable). ${reason}`,
    });
    expect(button).toHaveTextContent('Ornith-1.5-9Bunavailable');
    expect(button).toHaveAttribute('title', reason);
  });

  it('says a model on the CPU is on the CPU, with the reason and the cost', () => {
    draw({
      choice: { kind: 'explicit', model: { kind: 'local', model_id: 'Ornith-1.5-9B' } },
      effective: option('Ornith-1.5-9B'),
      degraded: 'Metal buffer limit',
    });
    const sentence = 'Ornith-1.5-9B is running on the CPU: Metal buffer limit. Answers will be slow.';
    const button = screen.getByRole('button', {
      name: `Model for this chat: Ornith-1.5-9B (chosen for this chat). ${sentence}`,
    });
    expect(button).toHaveTextContent('· on CPU');
    expect(button).toHaveAttribute('title', sentence);
  });

  it('offers a choice when no default is set, and draws nothing with nothing to choose', () => {
    const { unmount } = draw({});
    expect(screen.getByRole('button', { name: 'Choose a model for this chat' })).toHaveTextContent('Choose model');
    unmount();
    draw({ canChoose: false });
    expect(screen.queryByRole('button')).toBeNull();
  });
});
