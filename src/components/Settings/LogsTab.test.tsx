import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { LogsTab } from './LogsTab';
import { diagnostics } from '../../utils/diagnostics';

vi.mock('@tauri-apps/plugin-fs', () => ({ writeTextFile: vi.fn() }));
beforeEach(() => { diagnostics.clear(); vi.clearAllMocks(); });
it('updates live, filters, and requires confirmation to clear', () => {
  render(<LogsTab />);
  expect(screen.getByText('No events recorded')).toBeInTheDocument();
  act(() => { diagnostics.capture('Disk is full', 'API · save'); diagnostics.record('info', 'Started', 'Application'); });
  fireEvent.change(screen.getByLabelText('Log severity'), { target: { value: 'error' } });
  expect(screen.getByText('Disk is full')).toBeInTheDocument();
  expect(screen.queryByText('Started')).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText('Search logs'), { target: { value: 'missing' } });
  expect(screen.getByText('No matching entries')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: /^Clear$/ }));
  expect(diagnostics.getSnapshot()).toHaveLength(2);
  fireEvent.click(screen.getByRole('button', { name: 'Clear all logs' }));
  expect(diagnostics.getSnapshot()).toHaveLength(0);
});
it('exports diagnostics through the native save dialog and reports failures', async () => {
  diagnostics.capture('Cannot save', 'API');
  vi.mocked(save).mockResolvedValue('/tmp/logs.json');
  vi.mocked(writeTextFile).mockRejectedValueOnce(new Error('Permission denied'));
  render(<LogsTab />);
  fireEvent.click(screen.getByRole('button', { name: 'Export logs' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Could not export'));
  expect(screen.getByText('Permission denied')).toBeInTheDocument();
  expect(writeTextFile).toHaveBeenCalledWith('/tmp/logs.json', expect.stringContaining('Cannot save'));
});
