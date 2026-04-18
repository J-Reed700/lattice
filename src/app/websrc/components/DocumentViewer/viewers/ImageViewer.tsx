import { useState } from 'react';

import { convertFileSrc } from '@tauri-apps/api/core';
import { ZoomIn, ZoomOut, RotateCw, Maximize2, Minimize2 } from 'lucide-react';

/**
 * ImageViewer
 *
 * Purpose: Display image files with zoom and rotation controls
 *
 * Features:
 * - Zoom in/out
 * - Rotate image
 * - Fit to screen
 * - Reset to original size
 * - Smooth transitions
 *
 * States: default, zoomed
 * Accessibility: WCAG AA, keyboard navigation
 */

export interface ImageViewerProps {
  filePath: string;
  fileName: string;
}

export function ImageViewer({ filePath, fileName }: ImageViewerProps) {
  const [zoom, setZoom] = useState(1);
  const [rotation, setRotation] = useState(0);
  const [fitToScreen, setFitToScreen] = useState(true);
  const [error, setError] = useState(false);

  const imageUrl = convertFileSrc(filePath);

  const zoomIn = () => {
    setFitToScreen(false);
    setZoom((prev) => Math.min(5, prev + 0.25));
  };

  const zoomOut = () => {
    setFitToScreen(false);
    setZoom((prev) => Math.max(0.1, prev - 0.25));
  };

  const rotate = () => {
    setRotation((prev) => (prev + 90) % 360);
  };

  const toggleFit = () => {
    setFitToScreen((prev) => !prev);
    if (!fitToScreen) {
      setZoom(1);
    }
  };

  const resetView = () => {
    setZoom(1);
    setRotation(0);
    setFitToScreen(true);
  };

  if (error) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center max-w-md p-8">
          <div className="w-16 h-16 bg-[hsl(var(--danger-muted))]/30 rounded-full flex items-center justify-center mx-auto mb-4">
            <svg
              className="w-8 h-8 text-[hsl(var(--danger-fg))]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          </div>
          <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-2">
            Image Load Error
          </h3>
          <p className="text-[hsl(var(--text-secondary))]">Failed to load image file.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-[hsl(var(--bg))]">
      {/* Controls */}
      <div className="bg-[hsl(var(--surface-raised))] border-b border-[hsl(var(--border-subtle))] px-4 py-3 flex items-center justify-center gap-4 shadow-sm">
        <button
          onClick={zoomOut}
          className="p-2 rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))]"
          aria-label="Zoom out"
        >
          <ZoomOut className="w-5 h-5" />
        </button>

        <span className="text-sm font-medium text-[hsl(var(--text-primary))] min-w-[60px] text-center">
          {Math.round(zoom * 100)}%
        </span>

        <button
          onClick={zoomIn}
          className="p-2 rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))]"
          aria-label="Zoom in"
        >
          <ZoomIn className="w-5 h-5" />
        </button>

        <div className="w-px h-6 bg-[hsl(var(--border-subtle))]" aria-hidden="true" />

        <button
          onClick={rotate}
          className="p-2 rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))]"
          aria-label="Rotate 90 degrees"
        >
          <RotateCw className="w-5 h-5" />
        </button>

        <button
          onClick={toggleFit}
          className={`p-2 rounded-lg transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))] ${
            fitToScreen
              ? 'bg-[hsl(var(--accent-muted))]/30 text-[hsl(var(--accent))]'
              : 'hover:bg-[hsl(var(--surface-raised))]'
          }`}
          aria-label={fitToScreen ? 'Show actual size' : 'Fit to screen'}
        >
          {fitToScreen ? <Minimize2 className="w-5 h-5" /> : <Maximize2 className="w-5 h-5" />}
        </button>

        <div className="w-px h-6 bg-[hsl(var(--border-subtle))]" aria-hidden="true" />

        <button
          onClick={resetView}
          className="px-3 py-2 text-sm rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))]"
          aria-label="Reset view"
        >
          Reset
        </button>
      </div>

      {/* Image */}
      <div className="flex-1 overflow-auto flex items-center justify-center p-8">
        <img
          src={imageUrl}
          alt={fileName}
          onError={() => setError(true)}
          className="transition-transform duration-200"
          style={{
            transform: `scale(${zoom}) rotate(${rotation}deg)`,
            maxWidth: fitToScreen ? '100%' : 'none',
            maxHeight: fitToScreen ? '100%' : 'none',
            objectFit: 'contain',
            imageRendering: zoom > 2 ? 'pixelated' : 'auto',
          }}
        />
      </div>
    </div>
  );
}

export default ImageViewer;
