import { type FC, useEffect, useState } from 'react';

import { convertFileSrc } from '@tauri-apps/api/core';
import DOMPurify from 'dompurify';

interface ImageViewerProps {
  filePath: string;
}

export const ImageViewer: FC<ImageViewerProps> = ({ filePath }) => {
  const [safeSrc, setSafeSrc] = useState<string>('');
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  const isSVG = filePath.toLowerCase().endsWith('.svg');

  useEffect(() => {
    if (!isSVG) {
      // Non-SVG images are safe to load directly
      const src = convertFileSrc(filePath);
      setSafeSrc(src);
      setIsLoading(false);
      return;
    }

    // SVG requires sanitization
    setIsLoading(true);
    sanitizeSVG(filePath)
      .then(url => {
        setSafeSrc(url);
        setIsLoading(false);
      })
      .catch(err => {
        console.error('[ImageViewer] SVG sanitization failed:', err);
        setError('Failed to load SVG');
        setIsLoading(false);
      });

    // Cleanup blob URL on unmount
    return () => {
      if (safeSrc && safeSrc.startsWith('blob:')) {
        URL.revokeObjectURL(safeSrc);
      }
    };
  }, [filePath, isSVG, safeSrc]);

  async function sanitizeSVG(path: string): Promise<string> {
    // Fetch SVG content via Tauri
    const response = await fetch(convertFileSrc(path));
    const svgText = await response.text();

    // Sanitize with DOMPurify
    const clean = DOMPurify.sanitize(svgText, {
      USE_PROFILES: { svg: true, svgFilters: true },
      ADD_TAGS: ['use'], // Allow safe SVG tags
      FORBID_TAGS: ['script', 'iframe', 'object', 'embed', 'foreignObject'],
      FORBID_ATTR: ['onerror', 'onload', 'onclick', 'onmouseover'],
    });

    // Create blob URL from sanitized content
    const blob = new Blob([clean], { type: 'image/svg+xml' });
    return URL.createObjectURL(blob);
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center p-8">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-[var(--text-primary)] dark:border-[var(--text-primary)]" />
        <span className="ml-2 text-[var(--text-secondary)] dark:text-[var(--text-tertiary)]">Loading image...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center p-8">
        <p className="text-[var(--error)] dark:text-[var(--error-light)]">{error}</p>
      </div>
    );
  }

  return (
    <div className="flex justify-center items-center p-4">
      <img
        src={safeSrc}
        alt="Preview"
        className="max-w-full max-h-[60vh] object-contain rounded-lg"
        onError={() => setError('Failed to load image')}
      />
    </div>
  );
};
