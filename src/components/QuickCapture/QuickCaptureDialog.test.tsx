import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { QuickCaptureDialog } from './QuickCaptureDialog';

const quickCapture = vi.fn();
const ingestWebUrl = vi.fn();
const toastSuccess = vi.fn();
const toastError = vi.fn();

vi.mock('../../lib/api', () => ({
  VaultAPI: {
    quickCapture: (content: string) => quickCapture(content),
    ingestWebUrl: (url: string) => ingestWebUrl(url),
  },
}));

vi.mock('../../stores/toastStore', () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccess(...args),
    error: (...args: unknown[]) => toastError(...args),
  },
}));

describe('QuickCaptureDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    quickCapture.mockResolvedValue({ ok: true, data: { noteId: 'n1', noteTitle: 'Sep 6', created: false } });
    ingestWebUrl.mockResolvedValue({ ok: true, data: {} });
  });

  it('offers the clipboard URL while the textarea is empty', async () => {
    const user = userEvent.setup();
    render(<QuickCaptureDialog open clipboard="https://example.com/a" onClose={vi.fn()} />);

    const importButton = screen.getByRole('button', { name: 'Import example.com' });
    await user.click(importButton);

    expect(ingestWebUrl).toHaveBeenCalledWith('https://example.com/a');
    expect(toastSuccess).toHaveBeenCalledWith('Importing example.com');
  });

  it('hides the import row once something is typed', async () => {
    const user = userEvent.setup();
    render(<QuickCaptureDialog open clipboard="https://example.com/a" onClose={vi.fn()} />);

    await user.type(screen.getByPlaceholderText('What do you want to keep?'), 'a thought');

    expect(screen.queryByRole('button', { name: 'Import example.com' })).not.toBeInTheDocument();
  });

  it('saves on Enter and names the page it landed on', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<QuickCaptureDialog open clipboard={null} onClose={onClose} />);

    const textarea = screen.getByPlaceholderText('What do you want to keep?');
    await user.type(textarea, '  keep this  ');
    await user.type(textarea, '{Enter}');

    expect(quickCapture).toHaveBeenCalledWith('keep this');
    expect(toastSuccess).toHaveBeenCalledWith('Saved to Sep 6', undefined);
    expect(onClose).toHaveBeenCalled();
  });

  it('offers an Open action that opens the page the capture landed on', async () => {
    const user = userEvent.setup();
    const onOpenPage = vi.fn();
    render(
      <QuickCaptureDialog open clipboard={null} onClose={vi.fn()} onOpenPage={onOpenPage} />,
    );

    await user.type(screen.getByPlaceholderText('What do you want to keep?'), 'keep this{Enter}');

    expect(toastSuccess).toHaveBeenCalledWith('Saved to Sep 6', {
      action: { label: 'Open', onClick: expect.any(Function) },
    });
    const options = toastSuccess.mock.calls[0][1] as {
      action: { onClick: () => void };
    };
    options.action.onClick();
    expect(onOpenPage).toHaveBeenCalledWith('n1');
  });

  it('does not save on Shift+Enter', async () => {
    const user = userEvent.setup();
    render(<QuickCaptureDialog open clipboard={null} onClose={vi.fn()} />);

    const textarea = screen.getByPlaceholderText('What do you want to keep?');
    await user.type(textarea, 'line one');
    await user.type(textarea, '{Shift>}{Enter}{/Shift}');

    expect(quickCapture).not.toHaveBeenCalled();
  });

  it('disables Save while the textarea is blank', async () => {
    const user = userEvent.setup();
    render(<QuickCaptureDialog open clipboard={null} onClose={vi.fn()} />);

    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    await user.type(screen.getByPlaceholderText('What do you want to keep?'), 'x');
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
  });

  it('keeps the sheet open and reports the failure when saving fails', async () => {
    quickCapture.mockResolvedValue({ ok: false, error: 'disk full' });
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<QuickCaptureDialog open clipboard={null} onClose={onClose} />);

    const textarea = screen.getByPlaceholderText('What do you want to keep?');
    await user.type(textarea, 'keep this{Enter}');

    expect(toastError).toHaveBeenCalledWith("Couldn't save that", { message: 'disk full' });
    expect(onClose).not.toHaveBeenCalled();
    expect(textarea).toHaveValue('keep this');
  });
});
