import { render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { HTMLViewer } from '../HTMLViewer';

const mocks = vi.hoisted(() => ({ readFileContent: vi.fn(), theme: 'dark' }));
vi.mock('../../../../lib/api', () => ({ default: { readFileContent: mocks.readFileContent } }));
vi.mock('../../../../hooks/useApplyTheme', () => ({ useEffectiveTheme: () => mocks.theme }));

beforeEach(() => {
  vi.clearAllMocks();
  mocks.theme = 'dark';
});

it('rethemes legacy HTML without refetching it and removes source color overrides', async () => {
  mocks.readFileContent.mockResolvedValue({ ok: true, data: '<style>p { color: black !important; }</style><p style="color: black; background: white" bgcolor="white">Article text</p><script>alert(1)</script>' });
  const { rerender } = render(<HTMLViewer htmlPath="article.html" />);
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
  render(<HTMLViewer htmlPath="/archive/article.html" />);
  const frame = await screen.findByTitle<HTMLIFrameElement>('Web Article');
  const content = new DOMParser().parseFromString(frame.srcdoc, 'text/html');
  expect(content.querySelector('pre')?.textContent).toBe('# Snapshot <script>example</script>');
  expect(content.querySelector('script')).toBeNull();
  expect(frame.srcdoc).toContain('color-scheme: dark');
  expect(frame.srcdoc).not.toContain('#0f172a');
});
