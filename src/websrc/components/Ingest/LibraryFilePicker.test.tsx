import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { LibraryFilePicker } from './LibraryFilePicker';

const list = vi.hoisted(() => vi.fn());
vi.mock('@/lib/api', () => ({ default: { listAllDocuments: list } }));

describe('reuse saved PDFs', () => {
  it('filters the library and selects saved files without another upload', async () => {
    const user = userEvent.setup(); const add = vi.fn();
    list.mockResolvedValue({ ok: true, data: [
      { id: 'a', fileName: 'mpep-100.pdf', filePath: '/library/abc/mpep-100.pdf' },
      { id: 'b', fileName: 'unrelated.pdf', filePath: '/library/xyz/unrelated.pdf' },
    ] });
    render(<LibraryFilePicker onAdd={add} disabled={false} />);
    await user.click(screen.getByRole('button', { name: 'Choose files already in library' }));
    await user.type(await screen.findByRole('textbox', { name: 'Filter library files' }), 'mpep');
    await user.click(screen.getByRole('button', { name: 'Select matching files' }));
    await user.click(screen.getByRole('button', { name: 'Use 1 saved file' }));
    expect(add).toHaveBeenCalledWith(['/library/abc/mpep-100.pdf']);
  });
  it('shows library load failures and leaves selection unavailable', async () => {
    const user = userEvent.setup(); const add = vi.fn();
    list.mockResolvedValue({ ok: false, error: 'Database unavailable' });
    render(<LibraryFilePicker onAdd={add} disabled={false} />);
    await user.click(screen.getByRole('button', { name: 'Choose files already in library' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Database unavailable');
    expect(add).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: /Use .* saved/ })).not.toBeInTheDocument();
  });
});
