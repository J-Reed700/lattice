import { act, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { TextViewer } from '../TextViewer';

const readFileContent = vi.hoisted(() => vi.fn());
vi.mock('../../../../lib/api', () => ({ default: { readFileContent } }));
vi.mock('../../../../hooks/useApplyTheme', () => ({ useEffectiveTheme: () => 'light' }));
vi.mock('../prism', () => ({ PrismSyntaxHighlighter: ({ children }: { children: string }) => <pre>{children}</pre> }));

beforeEach(() => vi.clearAllMocks());

it('renders empty direct content and updates when its props change', () => {
  const { rerender } = render(<TextViewer content="" />);
  expect(screen.getByText('1 lines')).toBeVisible();
  rerender(<TextViewer content="new content" language="python" />);
  expect(screen.getByText('new content')).toBeVisible();
  expect(screen.getByText('PYTHON')).toBeVisible();
});

it('ignores a stale file response after another file is selected', async () => {
  let finishFirst!: (value: unknown) => void;
  readFileContent.mockImplementationOnce(() => new Promise((resolve) => { finishFirst = resolve; }));
  readFileContent.mockResolvedValueOnce({ ok: true, data: 'second file' });
  const { rerender } = render(<TextViewer filePath="first.txt" />);
  await waitFor(() => expect(readFileContent).toHaveBeenCalledWith('first.txt'));
  rerender(<TextViewer filePath="second.txt" />);
  await screen.findByText('second file');
  await act(async () => { finishFirst({ ok: true, data: 'stale first file' }); });
  expect(screen.getByText('second file')).toBeVisible();
  expect(screen.queryByText('stale first file')).not.toBeInTheDocument();
});
