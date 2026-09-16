import { render, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

import { ImageViewer } from '../ImageViewer';

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => `tauri://localhost${path}`,
}));

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
