import { type FC, useEffect, useLayoutEffect, useRef, useState } from 'react';

import { Maximize2, X } from 'lucide-react';

import { Dialog, DialogClose, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { SourceReaderBody } from '@/features/reading/components/SourceReaderBody';
import type { PassageLocator, SourceWithMetadata } from '@/types/conversation';


/**
 * The source reader as an overlay.
 *
 * Two shapes: `dialog` is the full-screen reader the Library, Compare and the
 * reference inbox open, and `reading-pane` is the sheet on the right edge —
 * what chat falls back to when the window is too narrow to dock the reader
 * beside the thread. Everything inside is `SourceReaderBody`, shared with the
 * docked pane, so there is one reader and not three.
 */

interface FilePreviewModalProps {
  isOpen: boolean;
  presentation?: 'dialog' | 'reading-pane';
  onClose: () => void;
  source: SourceWithMetadata | null;
  /** Where in the file to land. Omit for "open the whole file". */
  initialLocator?: PassageLocator | null;
  /** Every citation on the message, for `[` / `]` travel. */
  citations?: SourceWithMetadata[];
  /** Index of `source` within `citations`. */
  citationIndex?: number;
  onCitationIndexChange?: (_index: number) => void;
  /** Called when a viewer resolves a real location (e.g. a PDF page). */
  onLocationResolved?: (_chunkId: string, _label: string) => void;
  /** The message these citations belong to, where they came from one. */
  ownerKey?: string;
  /** Citation-bearing text when the source belongs to a journal page. */
  citationContent?: string;
  /** Which of the answer's marks for this source was clicked, if one was. */
  occurrence?: number | null;
}

export const FilePreviewModal: FC<FilePreviewModalProps> = ({
  isOpen,
  presentation = 'dialog',
  onClose,
  source,
  initialLocator = null,
  citations,
  citationIndex,
  onCitationIndexChange,
  onLocationResolved,
  ownerKey,
  citationContent,
  occurrence = null,
}) => {
  const [isFocused, setIsFocused] = useState(false);
  const [isMinimized, setIsMinimized] = useState(false);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const isReadingPane = presentation === 'reading-pane';

  // Capture the element to return focus to during the commit phase, before
  // Radix's passive auto-focus effect moves focus into the reader. A passive
  // useEffect here would run after that and capture a soon-unmounted element
  // inside the reader instead of the trigger, so closing would drop focus.
  useLayoutEffect(() => {
    if (isOpen) returnFocusRef.current = document.activeElement as HTMLElement;
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) {
      setIsFocused(false);
      setIsMinimized(false);
    }
  }, [isOpen]);

  if (!source) return null;

  return (
    <Dialog
      open={isOpen}
      // The reading pane is a sheet, not a modal: the page underneath stays
      // interactive (so it can be read or minimized away), and a click outside
      // it dismisses it. The full-screen dialog keeps its modal overlay.
      modal={!isReadingPane}
      onOpenChange={(open) => { if (!open) onClose(); }}
    >
      {/* A non-modal dialog draws no overlay. */}
      <DialogContent
        unstyled
        hideClose
        overlayClassName="fixed inset-0 z-50 bg-[hsl(var(--overlay))]"
        aria-describedby={undefined}
        // The reading pane is a sheet, not a modal: a click anywhere outside it
        // (or Escape) dismisses it, so the page underneath is always one click
        // away. The full-screen dialog keeps its own overlay for that job.
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          // preventScroll: the refocused element (often the journal editor's
          // scroller) must not be scrolled into view, or closing the reader
          // would yank the page back to the top.
          if (returnFocusRef.current?.isConnected) returnFocusRef.current.focus({ preventScroll: true });
        }}
        onInteractOutside={(event) => {
          // The minimized pill is sticky: reading the page underneath (clicks,
          // selection, focus) must not dismiss it, or there would be no point
          // in minimizing to keep reading. Only the pill's own buttons or
          // Escape close it. Expanded, a click outside dismisses as usual.
          if (isReadingPane && isMinimized) event.preventDefault();
        }}
        className={`source-reader ${isReadingPane ? 'source-reader--pane' : 'source-reader--dialog'} ${isFocused ? 'source-reader--focused' : ''} ${isReadingPane && isMinimized ? 'source-reader--pill' : ''}`}
      >
        {isReadingPane && isMinimized ? (
          // Minimized to a pill in the corner: the page is fully readable
          // underneath, and the pill is the only thing left of the reader.
          <div className="flex min-w-0 flex-1 items-center gap-2 px-3 py-2">
            <DialogTitle asChild>
              <h2 className="min-w-0 flex-1 truncate font-serif text-sm text-[hsl(var(--text-primary))]">
                {source.fileName}
              </h2>
            </DialogTitle>
            <button
              type="button"
              onClick={() => setIsMinimized(false)}
              aria-label="Restore reader"
              title="Restore reader"
              className="shrink-0 rounded-md p-1.5 text-text-secondary transition-colors hover:bg-surface-raised hover:text-[hsl(var(--text-primary))]"
            >
              <Maximize2 size={16} />
            </button>
            <DialogClose
              aria-label="Close reader"
              title="Close reader"
              className="shrink-0 rounded-md p-1.5 text-text-secondary transition-colors hover:bg-surface-raised hover:text-[hsl(var(--text-primary))]"
            >
              <X size={16} />
            </DialogClose>
          </div>
        ) : (
          <SourceReaderBody
            presentation={presentation}
            source={source}
            onClose={onClose}
            initialLocator={initialLocator}
            citations={citations}
            citationIndex={citationIndex}
            onCitationIndexChange={onCitationIndexChange}
            onLocationResolved={onLocationResolved}
            ownerKey={ownerKey}
            citationContent={citationContent}
            occurrence={occurrence}
            isFocused={isFocused}
            // Only the pane has somewhere to expand into; the dialog already
            // fills the window.
            onToggleFocus={isReadingPane ? () => setIsFocused((value) => !value) : undefined}
            // Only the pane can be tucked away to a pill; the dialog already
            // fills the window and the docked pane is the reader's home.
            onMinimize={isReadingPane ? () => setIsMinimized(true) : undefined}
          />
        )}
      </DialogContent>
    </Dialog>
  );
};
