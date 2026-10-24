import type { ReactElement } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

import { DocxViewer } from '../DocxViewer';

const mocks = vi.hoisted(() => ({ readFileBytes: vi.fn(), convertToHtml: vi.fn(), theme: 'dark' }));
vi.mock('@/lib/api', () => ({ default: { readFileBytes: mocks.readFileBytes } }));
vi.mock('@/hooks/useApplyTheme', () => ({ useEffectiveTheme: () => mocks.theme }));
vi.mock('mammoth', () => ({ convertToHtml: mocks.convertToHtml }));

function renderWithClient(ui: ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(ui, { wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider> });
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.theme = 'dark';
});

it('converts only the file’s own bytes and shows sanitized HTML in a sandboxed, themed frame', async () => {
  // A view into a larger buffer: only bytes 2–4 belong to the document.
  const backing = new Uint8Array([9, 9, 1, 2, 3, 9]);
  mocks.readFileBytes.mockResolvedValue({ ok: true, data: backing.subarray(2, 5) });
  mocks.convertToHtml.mockResolvedValue({ value: '<h2>Crop rotation</h2><p onclick="steal()">Three fields.</p><script>alert(1)</script>' });
  renderWithClient(<DocxViewer filePath="/farm/rotation.docx" title="Rotation notes" />);

  const frame = await screen.findByTitle<HTMLIFrameElement>('Rotation notes');
  expect(screen.getByRole('heading', { name: 'Rotation notes' })).toBeInTheDocument();
  expect(frame).toHaveAttribute('sandbox', 'allow-same-origin');
  expect(frame.srcdoc).toContain(':root { color-scheme: dark; }');
  const content = new DOMParser().parseFromString(frame.srcdoc, 'text/html');
  expect(content.body.textContent?.trim()).toBe('Crop rotationThree fields.');
  expect(content.querySelector('script, [onclick]')).toBeNull();

  expect(mocks.readFileBytes).toHaveBeenCalledWith('/farm/rotation.docx');
  const { arrayBuffer } = mocks.convertToHtml.mock.calls[0][0] as { arrayBuffer: ArrayBuffer };
  expect([...new Uint8Array(arrayBuffer)]).toEqual([1, 2, 3]);
});

it('uses a generic frame title without a document title', async () => {
  mocks.theme = 'light';
  mocks.readFileBytes.mockResolvedValue({ ok: true, data: new Uint8Array([1]) });
  mocks.convertToHtml.mockResolvedValue({ value: '<p>Untitled</p>' });
  renderWithClient(<DocxViewer filePath="/a.docx" />);
  const frame = await screen.findByTitle<HTMLIFrameElement>('DOCX Document');
  expect(frame.srcdoc).toContain('color-scheme: light');
  expect(screen.queryByRole('heading')).not.toBeInTheDocument();
});

it.each([
  [{ ok: false, error: 'permission denied' }, 'permission denied'],
  [{ ok: false, error: '' }, 'Failed to read DOCX bytes'],
])('explains a file that could not be read (%#)', async (result, message) => {
  mocks.readFileBytes.mockResolvedValue(result);
  renderWithClient(<DocxViewer filePath="/locked.docx" />);
  expect(await screen.findByText('Failed to load DOCX')).toBeInTheDocument();
  expect(screen.getByText(message)).toBeInTheDocument();
  expect(mocks.convertToHtml).not.toHaveBeenCalled();
});

it('explains a document the converter rejected', async () => {
  mocks.readFileBytes.mockResolvedValue({ ok: true, data: new Uint8Array([1]) });
  mocks.convertToHtml.mockRejectedValue(new Error('Could not find main document part'));
  renderWithClient(<DocxViewer filePath="/broken.docx" />);
  expect(await screen.findByText('Could not find main document part')).toBeInTheDocument();
});
