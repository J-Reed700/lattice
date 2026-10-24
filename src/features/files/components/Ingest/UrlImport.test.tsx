import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { UrlImport } from './UrlImport';

const h = vi.hoisted(() => ({ preview: vi.fn(), importPage: vi.fn() }));
vi.mock('@/features/files/hooks/useWebImport', () => ({ useWebImport: () => ({ preview: h.preview, importPage: h.importPage }) }));

const page = {
  title: 'How sourdough rises',
  description: 'Wild yeast and lactobacilli, explained.',
  siteName: 'Bread Lab',
  author: 'M. Baker',
  readingTimeMinutes: 6,
  wordCount: 1400,
};

const input = () => screen.getByRole('textbox', { name: 'URL to import' });
const type = (value: string) => fireEvent.change(input(), { target: { value } });
const settle = async () => { await act(async () => { await vi.advanceTimersByTimeAsync(500); }); };

describe('UrlImport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.spyOn(console, 'error').mockImplementation(() => {});
    h.preview.mockResolvedValue({ ok: true, data: page });
    h.importPage.mockResolvedValue({ ok: true, data: { documentId: 'd1' } });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('cannot import until there is a web address', () => {
    render(<UrlImport onImport={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();
    type('not a url');
    expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();
    type('ftp://files.example.org/a');
    expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();
  });

  it('previews the page once typing pauses', async () => {
    render(<UrlImport onImport={vi.fn()} />);
    type('https://breadlab.example.org/sour');
    type('https://breadlab.example.org/sourdough');
    expect(h.preview).not.toHaveBeenCalled();
    await settle();
    expect(h.preview).toHaveBeenCalledTimes(1);
    expect(h.preview).toHaveBeenCalledWith('https://breadlab.example.org/sourdough');
    expect(screen.getByText('How sourdough rises')).toBeInTheDocument();
    expect(screen.getByText('Bread Lab · M. Baker · 6 min read')).toBeInTheDocument();
    expect(screen.getByText('Wild yeast and lactobacilli, explained.')).toBeInTheDocument();
  });

  it('adds https to a bare domain', async () => {
    render(<UrlImport onImport={vi.fn()} />);
    type('  breadlab.example.org/rye  ');
    await settle();
    expect(h.preview).toHaveBeenCalledWith('https://breadlab.example.org/rye');
  });

  it('falls back to a word count, and to nothing', async () => {
    h.preview.mockResolvedValueOnce({ ok: true, data: { title: 'Rye', description: '', siteName: '', author: '', readingTimeMinutes: 0, wordCount: 2500 } });
    render(<UrlImport onImport={vi.fn()} />);
    type('https://a.example.org');
    await settle();
    expect(screen.getByText(`${(2500).toLocaleString()} words`)).toBeInTheDocument();
    h.preview.mockResolvedValueOnce({ ok: true, data: { title: 'Spelt', description: '', siteName: '', author: '', readingTimeMinutes: 0, wordCount: 0 } });
    type('https://b.example.org');
    await settle();
    expect(screen.getByText('Spelt')).toBeInTheDocument();
    expect(screen.queryByText(/words/)).not.toBeInTheDocument();
  });

  it('clears the preview when the address is cleared', async () => {
    render(<UrlImport onImport={vi.fn()} />);
    type('https://breadlab.example.org');
    await settle();
    type('');
    expect(screen.queryByText('How sourdough rises')).not.toBeInTheDocument();
  });

  it.each([
    ['a refused preview', () => h.preview.mockResolvedValueOnce({ ok: false, error: 'HTTP 404' }), 'HTTP 404'],
    ['a string rejection', () => h.preview.mockRejectedValueOnce('dns failure'), 'dns failure'],
    ['an object without a message', () => h.preview.mockRejectedValueOnce({ code: 7 }), '{"code":7}'],
  ])('shows %s', async (_label, arrange, message) => {
    arrange();
    render(<UrlImport onImport={vi.fn()} />);
    type('https://breadlab.example.org');
    await settle();
    expect(screen.getByText(message)).toBeInTheDocument();
  });

  it('shows a generic message for an error it cannot describe', async () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    h.preview.mockRejectedValueOnce(circular);
    render(<UrlImport onImport={vi.fn()} />);
    type('https://breadlab.example.org');
    await settle();
    expect(screen.getByText("Couldn't fetch preview")).toBeInTheDocument();
  });

  it('imports the page, reports it, and resets', async () => {
    const pending = deferred<{ ok: true; data: unknown }>();
    h.importPage.mockReturnValueOnce(pending.promise);
    const onImport = vi.fn();
    const onImportComplete = vi.fn();
    render(<UrlImport onImport={onImport} onImportComplete={onImportComplete} />);
    type('breadlab.example.org/rye');
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));
    expect(screen.getByRole('button', { name: 'Importing…' })).toBeDisabled();
    expect(h.importPage).toHaveBeenCalledWith('https://breadlab.example.org/rye');
    await act(async () => { pending.resolve({ ok: true, data: {} }); await pending.promise; });
    expect(onImport).toHaveBeenCalledWith('https://breadlab.example.org/rye');
    expect(onImportComplete).toHaveBeenCalledWith(true, 'https://breadlab.example.org/rye');
    expect(input()).toHaveValue('');
    expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();
  });

  it('keeps the address and shows why an import failed', async () => {
    h.importPage.mockResolvedValueOnce({ ok: false, error: 'paywalled page' });
    const onImport = vi.fn();
    const onImportComplete = vi.fn();
    render(<UrlImport onImport={onImport} onImportComplete={onImportComplete} />);
    type('https://breadlab.example.org/rye');
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Import' })); });
    expect(screen.getByText('paywalled page')).toBeInTheDocument();
    expect(onImport).not.toHaveBeenCalled();
    expect(onImportComplete).toHaveBeenCalledWith(false, 'https://breadlab.example.org/rye');
    expect(input()).toHaveValue('https://breadlab.example.org/rye');
  });

  it('works without a completion callback', async () => {
    h.importPage.mockResolvedValueOnce({ ok: false, error: 'x' });
    render(<UrlImport onImport={vi.fn()} />);
    type('https://breadlab.example.org/rye');
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Import' })); });
    expect(screen.getByText('x')).toBeInTheDocument();
  });
});
