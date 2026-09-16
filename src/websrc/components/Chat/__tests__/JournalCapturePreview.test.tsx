import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';

import { JournalCapturePreview } from '../JournalCapturePreview';

vi.mock('@/lib/api', () => ({ default: { quickCapture: vi.fn() } }));

describe('JournalCapturePreview', () => {
  it('keeps the preview on failure and opens the acknowledged note after retry', async () => {
    const open = vi.fn();
    vi.mocked(VaultAPI.quickCapture).mockResolvedValueOnce({ ok: false, error: 'Disk full' })
      .mockResolvedValueOnce({ ok: true, data: { noteId: 'note-42', noteTitle: 'Research', created: false } });
    render(<JournalCapturePreview content={'> A passage\n— Source'} onClose={vi.fn()} onOpenNote={open} />);
    await userEvent.click(screen.getByRole('button', { name: 'Add to journal' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Disk full');
    expect(screen.getByText(/A passage/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Add to journal' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Open entry' }));
    expect(open).toHaveBeenCalledWith('note-42');
    expect(VaultAPI.quickCapture).toHaveBeenLastCalledWith('> A passage\n— Source');
  });

  it('prevents repeat saves and dismissal while persistence is pending', async () => {
    let finish!: (value: Awaited<ReturnType<typeof VaultAPI.quickCapture>>) => void;
    vi.mocked(VaultAPI.quickCapture).mockReset().mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const close = vi.fn();
    render(<JournalCapturePreview content="Passage" onClose={close} onOpenNote={vi.fn()} />);
    await userEvent.dblClick(screen.getByRole('button', { name: 'Add to journal' }));
    expect(VaultAPI.quickCapture).toHaveBeenCalledTimes(1);
    await userEvent.keyboard('{Escape}');
    expect(close).not.toHaveBeenCalled();
    finish({ ok: true, data: { noteId: 'saved', noteTitle: 'Today', created: false } });
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Saved'));
  });
});
