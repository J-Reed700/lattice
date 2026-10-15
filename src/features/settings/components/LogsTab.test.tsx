import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import type { BackendLogEntry } from '@/lib/bindings';
import { diagnostics } from '@/utils/diagnostics';

import { LogsTab } from './LogsTab';

const mockGetBackendLogs = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/plugin-fs', () => ({ writeTextFile: vi.fn() }));
vi.mock('@/lib/api', () => ({ __esModule: true, default: { getBackendLogs: mockGetBackendLogs } }));

const backendLines: BackendLogEntry[] = [
  { timestamp: '2026-10-14T09:15:02.123456Z', level: 'error', target: 'lattice::search', message: 'Index missing at /Users/josh/vault/index.db' },
  { timestamp: null, level: 'unknown', target: null, message: 'retry with token=abc123' },
];

function renderTab() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><LogsTab /></QueryClientProvider>);
}

beforeEach(() => {
  diagnostics.clear();
  vi.clearAllMocks();
  mockGetBackendLogs.mockResolvedValue({ ok: true, data: [] });
});
it('updates live, filters, and requires confirmation to clear', async () => {
  renderTab();
  await waitFor(() => expect(mockGetBackendLogs).toHaveBeenCalled());
  expect(screen.getByText('No events recorded')).toBeInTheDocument();
  act(() => { diagnostics.capture('Disk is full', 'API · save'); diagnostics.record('info', 'Started', 'Application'); });
  fireEvent.change(screen.getByLabelText('Log severity'), { target: { value: 'error' } });
  expect(screen.getByText('Disk is full')).toBeInTheDocument();
  expect(screen.queryByText('Started')).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText('Search logs'), { target: { value: 'missing' } });
  expect(screen.getByText('No matching entries')).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: /^Clear$/ }));
  expect(diagnostics.getSnapshot()).toHaveLength(2);
  fireEvent.click(screen.getByRole('button', { name: 'Clear app logs' }));
  expect(diagnostics.getSnapshot()).toHaveLength(0);
});
it('shows redacted backend lines beside app entries and filters by source', async () => {
  mockGetBackendLogs.mockResolvedValue({ ok: true, data: backendLines });
  diagnostics.record('info', 'Started', 'Application');
  renderTab();
  expect(await screen.findByText('Index missing at /[USER]/vault/index.db')).toBeInTheDocument();
  expect(screen.getByText('retry with token=[REDACTED]')).toBeInTheDocument();
  expect(screen.getByText('Started')).toBeInTheDocument();
  expect(screen.getByText(/^Backend · lattice::search/)).toBeInTheDocument();
  expect(screen.getByText(/^App · Application/)).toBeInTheDocument();
  expect(document.body).not.toHaveTextContent(/josh|abc123/);

  fireEvent.change(screen.getByLabelText('Log source'), { target: { value: 'backend' } });
  expect(screen.queryByText('Started')).not.toBeInTheDocument();
  expect(screen.getByText('retry with token=[REDACTED]')).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText('Log source'), { target: { value: 'app' } });
  expect(screen.getByText('Started')).toBeInTheDocument();
  expect(screen.queryByText(/Index missing/)).not.toBeInTheDocument();

  fireEvent.change(screen.getByLabelText('Log source'), { target: { value: 'all' } });
  fireEvent.change(screen.getByLabelText('Log severity'), { target: { value: 'error' } });
  expect(screen.getByText(/Index missing/)).toBeInTheDocument();
  expect(screen.queryByText('Started')).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText('Log severity'), { target: { value: 'all' } });
  fireEvent.change(screen.getByLabelText('Search logs'), { target: { value: 'retry' } });
  expect(screen.getByText('retry with token=[REDACTED]')).toBeInTheDocument();
  expect(screen.queryByText(/Index missing/)).not.toBeInTheDocument();

  mockGetBackendLogs.mockResolvedValueOnce({ ok: true, data: [{ timestamp: null, level: 'unknown', target: null, message: 'retry again' }] });
  fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
  expect(await screen.findByText('retry again')).toBeInTheDocument();
  expect(mockGetBackendLogs).toHaveBeenCalledTimes(2);
});
it('exports app and backend entries in one file, each with its source', async () => {
  mockGetBackendLogs.mockResolvedValue({ ok: true, data: backendLines });
  diagnostics.record('warn', 'Slow save', 'API');
  vi.mocked(save).mockResolvedValue('/tmp/logs.json');
  vi.mocked(writeTextFile).mockResolvedValue(undefined);
  renderTab();
  await screen.findByText(/Index missing/);
  fireEvent.click(screen.getByRole('button', { name: 'Export logs' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Logs exported.'));
  const written = String(vi.mocked(writeTextFile).mock.calls[0]?.[1]);
  const exported = JSON.parse(written) as { version: number; entries: { origin: string; level: string; message: string }[] };
  expect(exported.version).toBe(1);
  expect(exported.entries.map(entry => [entry.origin, entry.level, entry.message])).toEqual(expect.arrayContaining([
    ['app', 'warn', 'Slow save'],
    ['backend', 'error', 'Index missing at /[USER]/vault/index.db'],
    ['backend', 'unknown', 'retry with token=[REDACTED]'],
  ]));
  expect(written).not.toMatch(/josh|abc123/);
});
it('exports diagnostics through the native save dialog and reports failures', async () => {
  diagnostics.capture('Cannot save', 'API');
  vi.mocked(save).mockResolvedValue('/tmp/logs.json');
  vi.mocked(writeTextFile).mockRejectedValueOnce(new Error('Permission denied'));
  renderTab();
  fireEvent.click(screen.getByRole('button', { name: 'Export logs' }));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Could not export'));
  expect(screen.getByText('Permission denied')).toBeInTheDocument();
  expect(writeTextFile).toHaveBeenCalledWith('/tmp/logs.json', expect.stringContaining('Cannot save'));
});
