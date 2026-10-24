import { type FC, useEffect, useLayoutEffect, useRef, useState } from 'react';

import { Maximize2, X } from 'lucide-react';

import { Dialog, DialogClose, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { SourceReaderBody } from '@/features/reading/components/SourceReaderBody';
import { focusReturnTarget } from '@/lib/focusReturn';
import type { PassageLocator, SourceWithMetadata } from '@/types/conversation';

interface FilePreviewModalProps {
  isOpen: boolean;
  presentation?: 'dialog' | 'reading-pane';
  onClose: () => void;
  source: SourceWithMetadata | null;
  initialLocator?: PassageLocator | null;
  citations?: SourceWithMetadata[];
  citationIndex?: number;
  onCitationIndexChange?: (_index: number) => void;
  onLocationResolved?: (_chunkId: string, _label: string) => void;
  ownerKey?: string;
  citationContent?: string;
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

  // Capture trigger element before auto-focus moves focus into the reader.
  useLayoutEffect(() => {
    if (isOpen) returnFocusRef.current = focusReturnTarget();
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
      modal={!isReadingPane}
      onOpenChange={(open) => { if (!open) onClose(); }}
    >
      <DialogContent
        unstyled
        hideClose
        overlayClassName="fixed inset-0 z-50 bg-[hsl(var(--overlay))]"
        aria-describedby={undefined}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          // Avoid scrolling container when returning focus.
          if (returnFocusRef.current?.isConnected) returnFocusRef.current.focus({ preventScroll: true });
        }}
        onInteractOutside={(event) => {
          // Do not dismiss when interacting outside while minimized as a pill.
          if (isReadingPane && isMinimized) event.preventDefault();
        }}
        className={`source-reader ${isReadingPane ? 'source-reader--pane' : 'source-reader--dialog'} ${isFocused ? 'source-reader--focused' : ''} ${isReadingPane && isMinimized ? 'source-reader--pill' : ''}`}
      >
        {isReadingPane && isMinimized ? (
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
            onToggleFocus={isReadingPane ? () => setIsFocused((value) => !value) : undefined}
            onMinimize={isReadingPane ? () => setIsMinimized(true) : undefined}
          />
        )}
      </DialogContent>
    </Dialog>
  );
};
