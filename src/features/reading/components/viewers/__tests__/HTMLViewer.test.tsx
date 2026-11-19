import type { ReactElement } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { HTMLViewer } from '../HTMLViewer';

const mocks = vi.hoisted(() => ({ readFileContent: vi.fn(), theme: 'dark' }));
vi.mock('@/lib/api', () => ({ default: { readFileContent: mocks.readFileContent } }));
vi.mock('@/hooks/useApplyTheme', () => ({ useEffectiveTheme: () => mocks.theme }));

function renderWithClient(ui: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(ui, { wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider> });
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.theme = 'dark';
});

it('rethemes legacy HTML without refetching it and removes source color overrides', async () => {
  mocks.readFileContent.mockResolvedValue({ ok: true, data: '<style>p { color: black !important; }</style><p style="color: black; background: white" bgcolor="white">Article text</p><script>alert(1)</script>' });
  const { rerender } = renderWithClient(<HTMLViewer htmlPath="article.html" />);
  const frame = await screen.findByTitle<HTMLIFrameElement>('Web Article');
  const content = new DOMParser().parseFromString(frame.srcdoc, 'text/html');
  expect(content.body.textContent?.trim()).toBe('Article text');
  expect(content.querySelector('script, [style], [bgcolor]')).toBeNull();
  expect(frame.srcdoc).toContain('color-scheme: dark');
  expect(frame.srcdoc).not.toContain('!important');
  mocks.theme = 'light';
  rerender(<HTMLViewer htmlPath="article.html" />);
  expect(frame.srcdoc).toContain('color-scheme: light');
  expect(mocks.readFileContent).toHaveBeenCalledTimes(1);
});

it('uses the same theme for the escaped markdown fallback', async () => {
  mocks.readFileContent.mockResolvedValueOnce({ ok: false, error: 'Missing' });
  mocks.readFileContent.mockResolvedValueOnce({ ok: true, data: '# Snapshot <script>example</script>' });
  renderWithClient(<HTMLViewer article={{ textPath: '/archive/article.md', pagePath: '/archive/page.html', title: null, url: null, links: [] }} />);
  const frame = await screen.findByTitle<HTMLIFrameElement>('Web Article');
  const content = new DOMParser().parseFromString(frame.srcdoc, 'text/html');
  expect(content.querySelector('pre')?.textContent).toBe('# Snapshot <script>example</script>');
  expect(content.querySelector('script')).toBeNull();
  expect(frame.srcdoc).toContain('color-scheme: dark');
  expect(frame.srcdoc).not.toContain('#0f172a');
});

const ARTICLE = {
  textPath: '/data/web-archive/example.com/a-1/article.md',
  pagePath: '/data/web-archive/example.com/a-1/page.html',
  title: 'A saved piece',
  url: 'https://example.com/a',
  links: ['https://example.com/one', 'https://example.com/two'],
};

it("shows a saved article's address and links from the archive, reading only its page", async () => {
  mocks.readFileContent.mockResolvedValue({ ok: true, data: '<p>Saved page</p>' });
  renderWithClient(<HTMLViewer article={ARTICLE} />);

  expect(await screen.findByRole('button', { name: /Open Original URL/ })).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: /Show links \(2\)/ }));
  expect(screen.getByText('https://example.com/two')).toBeTruthy();
  expect(mocks.readFileContent).toHaveBeenCalledTimes(1);
  expect(mocks.readFileContent).toHaveBeenCalledWith(ARTICLE.pagePath);
});

it('shows the text of a saved article whose page is gone, with the notice', async () => {
  mocks.readFileContent.mockResolvedValue({ ok: true, data: '# Snapshot <b>text</b>' });
  renderWithClient(<HTMLViewer article={{ ...ARTICLE, pagePath: null }} />);

  const frame = await screen.findByTitle<HTMLIFrameElement>('A saved piece');
  const content = new DOMParser().parseFromString(frame.srcdoc, 'text/html');
  expect(content.querySelector('pre')?.textContent).toBe('# Snapshot <b>text</b>');
  expect(screen.getByText('Archived HTML missing. Showing markdown snapshot.')).toBeTruthy();
  expect(mocks.readFileContent).toHaveBeenCalledTimes(1);
  expect(mocks.readFileContent).toHaveBeenCalledWith(ARTICLE.textPath);
});

it('shows the text of a saved article whose page cannot be read, with the notice', async () => {
  mocks.readFileContent.mockImplementation(async (path: string) =>
    path === ARTICLE.pagePath ? { ok: false, error: 'Missing' } : { ok: true, data: 'Snapshot words' });
  renderWithClient(<HTMLViewer article={ARTICLE} />);

  expect(await screen.findByText('Archived HTML missing. Showing markdown snapshot.')).toBeTruthy();
  expect(mocks.readFileContent.mock.calls.map(([path]) => path)).toEqual([ARTICLE.pagePath, ARTICLE.textPath]);
});

it('reads only the file it is given for a plain HTML path, even one named page.html', async () => {
  mocks.readFileContent.mockResolvedValue({ ok: true, data: '<p>Plain page</p>' });
  renderWithClient(<HTMLViewer htmlPath="/docs/page.html" title="Plain" />);

  await screen.findByTitle('Plain');
  expect(mocks.readFileContent.mock.calls).toEqual([['/docs/page.html']]);
  expect(screen.queryByRole('button', { name: /Open Original URL/ })).toBeNull();
});
