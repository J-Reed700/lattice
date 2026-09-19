import { type FC, useEffect, useRef, useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';

import type { PassageLocator, SourceWithMetadata } from '@/types/conversation';

import { SourceReaderBody } from './reader/SourceReaderBody';

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
}) => {
  const [isFocused, setIsFocused] = useState(false);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const isReadingPane = presentation === 'reading-pane';

  useEffect(() => {
    if (isOpen) {
      returnFocusRef.current = document.activeElement as HTMLElement;
    } else {
      setIsFocused(false);
    }
  }, [isOpen]);

  if (!source) return null;

  return (
    <Dialog.Root open={isOpen} onOpenChange={(open) => { if (!open) onClose(); }}>
      <Dialog.Portal>
        {!isReadingPane && <Dialog.Overlay className="fixed inset-0 z-50 bg-[hsl(var(--overlay))]" />}
        <Dialog.Content
          aria-describedby={undefined}
          onInteractOutside={isReadingPane ? (event) => event.preventDefault() : undefined}
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            if (returnFocusRef.current?.isConnected) returnFocusRef.current.focus();
          }}
          className={`source-reader ${isReadingPane ? 'source-reader--pane' : 'source-reader--dialog'} ${isFocused ? 'source-reader--focused' : ''}`}
        >
          <SourceReaderBody
            presentation={presentation}
            source={source}
            onClose={onClose}
            initialLocator={initialLocator}
            citations={citations}
            citationIndex={citationIndex}
            onCitationIndexChange={onCitationIndexChange}
            onLocationResolved={onLocationResolved}
            isFocused={isFocused}
            // Only the pane has somewhere to expand into; the dialog already
            // fills the window.
            onToggleFocus={isReadingPane ? () => setIsFocused((value) => !value) : undefined}
          />
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
};
