import type { ReactElement } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { TextViewer } from '../TextViewer';

const readFileContent = vi.hoisted(() => vi.fn());
vi.mock('@/lib/api', () => ({ default: { readFileContent } }));
vi.mock('@/hooks/useApplyTheme', () => ({ useEffectiveTheme: () => 'light' }));
vi.mock('../prism', () => ({ PrismSyntaxHighlighter: ({ children }: { children: string }) => <pre>{children}</pre> }));

function renderWithClient(ui: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(ui, { wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider> });
}

beforeEach(() => vi.clearAllMocks());

it('renders empty direct content and updates when its props change', () => {
  const { rerender } = renderWithClient(<TextViewer content="" />);
  expect(screen.getByText('1 lines')).toBeVisible();
  rerender(<TextViewer content="new content" language="python" />);
  expect(screen.getByText('new content')).toBeVisible();
  expect(screen.getByText('PYTHON')).toBeVisible();
});

it('ignores a stale file response after another file is selected', async () => {
  let finishFirst!: (value: unknown) => void;
  readFileContent.mockImplementationOnce(() => new Promise((resolve) => { finishFirst = resolve; }));
  readFileContent.mockResolvedValueOnce({ ok: true, data: 'second file' });
  const { rerender } = renderWithClient(<TextViewer filePath="first.txt" />);
  await waitFor(() => expect(readFileContent).toHaveBeenCalledWith('first.txt'));
  rerender(<TextViewer filePath="second.txt" />);
  await screen.findByText('second file');
  await act(async () => { finishFirst({ ok: true, data: 'stale first file' }); });
  expect(screen.getByText('second file')).toBeVisible();
  expect(screen.queryByText('stale first file')).not.toBeInTheDocument();
});

it('shows a failed read at once instead of retrying it', async () => {
  readFileContent.mockResolvedValue({ ok: false, error: 'Permission denied' });
  renderWithClient(<TextViewer filePath="locked.txt" />);
  expect(await screen.findByText('Permission denied')).toBeVisible();
  expect(readFileContent).toHaveBeenCalledTimes(1);
});
