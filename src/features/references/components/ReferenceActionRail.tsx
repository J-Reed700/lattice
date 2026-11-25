import { useState } from 'react';

import {
  ArrowUpRight,
  Check,
  Copy,
  Loader2,
  NotebookPen,
  Trash2,
} from 'lucide-react';

import { Button } from '@/components/ui/button';

interface ReferenceActionRailProps {
  isCaptured: boolean;
  captureDestinationLabel: string;
  onOpenInChat: () => void;
  onCopy: () => Promise<boolean>;
  onCapture: () => Promise<void>;
  onOpenCapturedNote: () => void;
  onDelete: () => void;
  originDisabled?: boolean;
  originDisabledReason?: string;
}

/**
 * Unified bottom action rail for the reference reader. Compact ghost buttons
 * with crisp spinning loader feedback and consistent h-8 rounded-lg rhythm.
 */
export function ReferenceActionRail({
  isCaptured,
  captureDestinationLabel,
  onOpenInChat,
  onCopy,
  onCapture,
  onOpenCapturedNote,
  onDelete,
  originDisabled = false,
  originDisabledReason,
}: ReferenceActionRailProps) {
  const [copiedTick, setCopiedTick] = useState(false);
  const [capturing, setCapturing] = useState(false);

  const handleCopy = async () => {
    const ok = await onCopy();
    if (!ok) return;
    setCopiedTick(true);
    window.setTimeout(() => setCopiedTick(false), 1300);
  };

  const handleCapture = async () => {
    if (capturing) return;
    setCapturing(true);
    try {
      await onCapture();
    } finally {
      setCapturing(false);
    }
  };

  return (
    <div className="mt-6 flex flex-wrap items-center justify-between gap-2 border-t border-border-subtle pt-4">
      <div className="flex flex-wrap items-center gap-1">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onOpenInChat}
          disabled={originDisabled}
          title={originDisabled ? originDisabledReason : 'Open in Chat'}
          className="h-8 rounded-lg px-2.5 text-xs text-text-secondary hover:text-text-primary"
        >
          <ArrowUpRight className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
          Open in Chat
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => void handleCopy()}
          title="Copy reference content"
          className="h-8 rounded-lg px-2.5 text-xs text-text-secondary hover:text-text-primary"
        >
          {copiedTick ? (
            <Check className="h-3.5 w-3.5 text-accent" strokeWidth={2} aria-hidden="true" />
          ) : (
            <Copy className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
          )}
          Copy
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => void handleCapture()}
          disabled={capturing}
          title={captureDestinationLabel}
          className="h-8 rounded-lg px-2.5 text-xs text-text-secondary hover:text-text-primary"
        >
          {capturing ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin text-text-muted" strokeWidth={1.75} aria-hidden="true" />
          ) : (
            <NotebookPen className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
          )}
          {isCaptured ? 'Re-capture' : 'Capture'}
        </Button>
        {isCaptured && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onOpenCapturedNote}
            title="Open the note this reference was captured to"
            className="h-8 rounded-lg px-2.5 text-xs text-text-secondary hover:text-text-primary"
          >
            <NotebookPen className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
            Open captured note
          </Button>
        )}
      </div>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={onDelete}
        title="Delete reference"
        className="h-8 rounded-lg px-2.5 text-xs text-text-muted hover:bg-danger-muted hover:text-danger-fg"
      >
        <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
        Delete
      </Button>
    </div>
  );
}
