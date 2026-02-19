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
    <div className="bg-[var(--surface-elevated)] border-b border-[var(--border-color)] px-6 py-4 flex items-center justify-between shadow-sm">
      {/* Filename */}
      <div className="flex-1 min-w-0 mr-4">
        <h2 className="text-lg font-semibold text-[var(--text-primary)] truncate" title={sanitizedFileName}>
          {sanitizedFileName}
        </h2>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2">
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={onDownload}
              className="p-2 text-[var(--text-secondary)] hover:text-[var(--text-primary)] rounded-lg hover:bg-[var(--bg-tertiary)] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
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
              className="p-2 text-[var(--text-secondary)] hover:text-[var(--text-primary)] rounded-lg hover:bg-[var(--bg-tertiary)] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
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
              className="p-2 text-[var(--text-secondary)] hover:text-[var(--text-primary)] rounded-lg hover:bg-[var(--bg-tertiary)] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
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

        <div className="w-px h-6 bg-[var(--border-color)] mx-2" aria-hidden="true" />

        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={onClose}
              className="p-2 text-[var(--text-secondary)] hover:text-[var(--text-primary)] rounded-lg hover:bg-[var(--bg-tertiary)] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)]"
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
