import { memo } from 'react';

import { X, ExternalLink, Download, PanelRightClose, PanelRightOpen } from 'lucide-react';

import { sanitizeFileName } from '@/utils/sanitize';

import { Tooltip, TooltipTrigger, TooltipContent } from '../ui';

/**
 * ViewerHeader
 *
 * Purpose: Document viewer header with actions and controls
 *
 * Features:
 * - Display filename
 * - Action buttons (download, open external, toggle sidebar)
 * - Close button
 * - Responsive layout
 *
 * States: default
 * Accessibility: WCAG AA, keyboard navigation, ARIA labels
 * Performance: Optimized with React.memo to prevent unnecessary re-renders
 */

export interface ViewerHeaderProps {
  fileName: string;
  onClose: () => void;
  onOpenExternal: () => void;
  onDownload: () => void;
  onToggleSidebar: () => void;
  showSidebar: boolean;
}

export const ViewerHeader = memo(({
  fileName,
  onClose,
  onOpenExternal,
  onDownload,
  onToggleSidebar,
  showSidebar,
}: ViewerHeaderProps) => {
  const sanitizedFileName = sanitizeFileName(fileName);
  
  return (
    <div className="bg-[hsl(var(--surface-raised))] border-b border-[hsl(var(--border-subtle))] px-6 py-4 flex items-center justify-between shadow-sm">
      {/* Filename */}
      <div className="flex-1 min-w-0 mr-4">
        <h2 className="text-lg font-semibold text-[hsl(var(--text-primary))] truncate" title={sanitizedFileName}>
          {sanitizedFileName}
        </h2>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2">
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={onDownload}
              className="p-2 text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))]"
              aria-label="Show in folder"
            >
              <Download className="w-5 h-5" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom">Show in folder</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={onOpenExternal}
              className="p-2 text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))]"
              aria-label="Open in external application"
            >
              <ExternalLink className="w-5 h-5" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom">Open in external application</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={onToggleSidebar}
              className="p-2 text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))]"
              aria-label={showSidebar ? 'Hide info sidebar' : 'Show info sidebar'}
            >
              {showSidebar ? (
                <PanelRightClose className="w-5 h-5" />
              ) : (
                <PanelRightOpen className="w-5 h-5" />
              )}
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom">
            {showSidebar ? 'Hide info sidebar (I)' : 'Show info sidebar (I)'}
          </TooltipContent>
        </Tooltip>

        <div className="w-px h-6 bg-[hsl(var(--border-subtle))] mx-2" aria-hidden="true" />

        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={onClose}
              className="p-2 text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))] rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent))]"
              aria-label="Close viewer"
            >
              <X className="w-5 h-5" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom">Close viewer (Esc)</TooltipContent>
        </Tooltip>
      </div>
    </div>
  );
}, (prevProps, nextProps) => 
  // Only re-render if fileName or showSidebar changes
   prevProps.fileName === nextProps.fileName &&
    prevProps.showSidebar === nextProps.showSidebar
);

ViewerHeader.displayName = 'ViewerHeader';

export default ViewerHeader;
