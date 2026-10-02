import { act, cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, describe, it, expect, vi } from 'vitest';

import { ImageViewer } from '../ImageViewer';

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => `tauri://localhost${path}`,
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const svgResponse = (svg = '<svg xmlns="http://www.w3.org/2000/svg" />') => ({
  text: async () => svg,
});

describe('ImageViewer', () => {
  it('should load non-SVG images directly', async () => {
    const { container } = render(<ImageViewer filePath="/path/to/image.png" />);
    
    await waitFor(() => {
      const img = container.querySelector('img[alt="Preview"]');
      expect(img).toBeTruthy();
      expect(img?.getAttribute('src')).toContain('image.png');
    });
  });

  it('should show loading state initially for SVG', () => {
    const { container } = render(<ImageViewer filePath="/path/to/image.svg" />);
    expect(container.textContent).toContain('Loading image');
  });

  it('fetches an SVG only once and revokes its object URL on unmount', async () => {
    const fetchMock = vi.fn().mockResolvedValue(svgResponse());
    vi.stubGlobal('fetch', fetchMock);
    vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:loaded-svg');
    const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});

    const view = render(<ImageViewer filePath="/path/to/image.svg" />);

    await waitFor(() => {
      expect(view.container.querySelector('img')?.getAttribute('src')).toBe('blob:loaded-svg');
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);

    view.unmount();

    expect(revoke).toHaveBeenCalledExactlyOnceWith('blob:loaded-svg');
  });

  it('revokes stale SVG URLs after a rapid path switch', async () => {
    const resolveFetch = new Map<string, (response: ReturnType<typeof svgResponse>) => void>();
    const fetchMock = vi.fn((src: string) => new Promise(resolve => { resolveFetch.set(src, resolve); }));
    vi.stubGlobal('fetch', fetchMock);
    vi.spyOn(URL, 'createObjectURL')
      .mockReturnValueOnce('blob:second-svg')
      .mockReturnValueOnce('blob:first-svg');
    const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});

    const view = render(<ImageViewer filePath="/path/first.svg" />);
    view.rerender(<ImageViewer filePath="/path/second.svg" />);
    await waitFor(() => {
      expect(resolveFetch.has('tauri://localhost/path/second.svg')).toBe(true);
    });

    await act(async () => {
      resolveFetch.get('tauri://localhost/path/second.svg')!(svgResponse());
    });
    await waitFor(() => {
      expect(view.container.querySelector('img')?.getAttribute('src')).toBe('blob:second-svg');
    });

    await act(async () => {
      resolveFetch.get('tauri://localhost/path/first.svg')!(svgResponse());
    });

    expect(revoke).toHaveBeenCalledWith('blob:first-svg');
    expect(view.container.querySelector('img')?.getAttribute('src')).toBe('blob:second-svg');

    view.unmount();
    expect(revoke).toHaveBeenCalledWith('blob:second-svg');
  });

  it('aborts a pending fetch on path replacement and unmount', () => {
    const signals: AbortSignal[] = [];
    vi.stubGlobal('fetch', vi.fn((_url: string, options: RequestInit) => {
      signals.push(options.signal as AbortSignal);
      return new Promise(() => {});
    }));
    const view = render(<ImageViewer filePath="/path/first.svg" />);
    view.rerender(<ImageViewer filePath="/path/second.svg" />);
    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
    view.unmount();
    expect(signals[1].aborted).toBe(true);
  });

  it('revokes an SVG URL created after the viewer unmounts', async () => {
    let resolveFetch!: (response: ReturnType<typeof svgResponse>) => void;
    vi.stubGlobal('fetch', vi.fn(() => new Promise(resolve => { resolveFetch = resolve; })));
    const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:late-svg');
    const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});

    const view = render(<ImageViewer filePath="/path/to/pending.svg" />);
    view.unmount();

    await act(async () => {
      resolveFetch(svgResponse());
    });

    expect(create).toHaveBeenCalledTimes(1);
    expect(revoke).toHaveBeenCalledExactlyOnceWith('blob:late-svg');
  });

  it('should detect SVG files case-insensitively', () => {
    const { container } = render(<ImageViewer filePath="/path/to/IMAGE.SVG" />);
    expect(container.textContent).toContain('Loading image');
  });

  it('should show error state if image fails to load', async () => {
    const { container } = render(<ImageViewer filePath="/nonexistent.png" />);
    
    await waitFor(() => {
      const img = container.querySelector('img');
      expect(img).toBeTruthy();
    });

    const img = container.querySelector('img');
    if (img) {
      // Trigger error
      img.dispatchEvent(new Event('error'));
    }

    await waitFor(() => {
      expect(container.textContent).toContain('Failed to load image');
    });
  });

  it('should handle non-SVG JPG files', async () => {
    const { container } = render(<ImageViewer filePath="/path/to/image.jpg" />);
    
    await waitFor(() => {
      const img = container.querySelector('img[alt="Preview"]');
      expect(img).toBeTruthy();
      expect(img?.getAttribute('src')).toContain('image.jpg');
    });
  });

  it('should handle non-SVG JPEG files', async () => {
    const { container } = render(<ImageViewer filePath="/path/to/image.jpeg" />);
    
    await waitFor(() => {
      const img = container.querySelector('img[alt="Preview"]');
      expect(img).toBeTruthy();
      expect(img?.getAttribute('src')).toContain('image.jpeg');
    });
  });

  it('should handle non-SVG GIF files', async () => {
    const { container } = render(<ImageViewer filePath="/path/to/image.gif" />);
    
    await waitFor(() => {
      const img = container.querySelector('img[alt="Preview"]');
      expect(img).toBeTruthy();
      expect(img?.getAttribute('src')).toContain('image.gif');
    });
  });
});
