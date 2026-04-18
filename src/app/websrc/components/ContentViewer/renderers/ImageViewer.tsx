import { useState } from 'react';

import { Loader2, AlertCircle, ZoomIn, ZoomOut, RotateCw } from 'lucide-react';

interface ImageViewerProps {
  filePath: string;
  title?: string;
}

export function ImageViewer({ filePath, title }: ImageViewerProps) {
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [scale, setScale] = useState(1.0);
  const [rotation, setRotation] = useState(0);

  const handleLoad = () => setIsLoading(false);
  const handleError = () => {
    setError('Failed to load image');
    setIsLoading(false);
  };

  const zoomIn = () => setScale((s) => Math.min(5.0, s + 0.25));
  const zoomOut = () => setScale((s) => Math.max(0.25, s - 0.25));
  const rotate = () => setRotation((r) => (r + 90) % 360);
  const resetView = () => {
    setScale(1.0);
    setRotation(0);
  };

  if (error) {
    return (
      <div className="flex items-center justify-center h-full p-8">
        <div className="text-center space-y-2">
          <AlertCircle className="w-12 h-12 text-destructive mx-auto" />
          <p className="text-destructive font-medium">Failed to load image</p>
          <p className="text-sm text-muted-foreground">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-background">
      {/* Header */}
      {title && (
        <div className="px-6 py-4 border-b">
          <h1 className="text-xl font-semibold text-foreground">{title}</h1>
        </div>
      )}

      {/* Controls */}
      <div className="flex items-center justify-center gap-4 px-6 py-3 border-b bg-surface-elevated">
        <button
          onClick={zoomOut}
          disabled={scale <= 0.25}
          className="p-2 rounded hover:bg-surface-hover disabled:opacity-50 disabled:cursor-not-allowed"
          aria-label="Zoom out"
        >
          <ZoomOut className="w-5 h-5" />
        </button>
        <span className="text-sm text-foreground min-w-[60px] text-center">
          {Math.round(scale * 100)}%
        </span>
        <button
          onClick={zoomIn}
          disabled={scale >= 5.0}
          className="p-2 rounded hover:bg-surface-hover disabled:opacity-50 disabled:cursor-not-allowed"
          aria-label="Zoom in"
        >
          <ZoomIn className="w-5 h-5" />
        </button>
        <div className="w-px h-6 bg-border mx-2" />
        <button
          onClick={rotate}
          className="p-2 rounded hover:bg-surface-hover"
          aria-label="Rotate"
        >
          <RotateCw className="w-5 h-5" />
        </button>
        <button
          onClick={resetView}
          className="px-3 py-1 text-sm rounded hover:bg-surface-hover"
        >
          Reset
        </button>
      </div>

      {/* Image Content */}
      <div className="flex-1 overflow-auto flex items-center justify-center p-8 bg-[hsl(var(--surface))]">
        {isLoading && (
          <div className="absolute inset-0 flex items-center justify-center">
            <Loader2 className="w-8 h-8 animate-spin text-primary" />
          </div>
        )}
        <img
          src={`file://${filePath}`}
          alt={title || 'Image'}
          onLoad={handleLoad}
          onError={handleError}
          className="max-w-full max-h-full object-contain shadow-md transition-transform"
          style={{
            transform: `scale(${scale}) rotate(${rotation}deg)`,
          }}
        />
      </div>
    </div>
  );
}
