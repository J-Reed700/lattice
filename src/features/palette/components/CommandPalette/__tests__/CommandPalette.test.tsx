import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, expect, it, vi } from 'vitest';

import { OPEN_PALETTE_EVENT } from '@/features/palette/hooks/useCommandPalette';
import { VaultAPI } from '@/lib/api';
import { useToastStore } from '@/stores/toastStore';

import { CommandPalette } from '../CommandPalette';

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;

// cmdk measures its list and scrolls the active item; jsdom has neither.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;
Element.prototype.scrollIntoView ??= () => {};

beforeEach(() => {
  useToastStore.setState({ toasts: [] });
});

async function openPalette() {
  render(
    <MemoryRouter>
      <CommandPalette />
    </MemoryRouter>
  );
  act(() => { window.dispatchEvent(new Event(OPEN_PALETTE_EVENT)); });
  return screen.findByRole('dialog');
}

it('is a modal dialog', async () => {
  const dialog = await openPalette();
  expect(dialog).toHaveAttribute('aria-modal', 'true');
  expect(dialog).toHaveAccessibleName('Command palette');
});

it('tells the user when an action fails after the palette closed', async () => {
  api.clearCache = vi.fn().mockResolvedValue({ ok: false, error: 'cache is locked' });
  await openPalette();
  fireEvent.click(screen.getByText('Clear search cache'));
  await waitFor(() =>
    expect(useToastStore.getState().toasts).toEqual([
      expect.objectContaining({ type: 'error', title: "Couldn't clear the cache", message: 'cache is locked' }),
    ])
  );
});
