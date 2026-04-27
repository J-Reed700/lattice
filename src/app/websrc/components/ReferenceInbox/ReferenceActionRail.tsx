import { useState } from 'react';

import {
  ArrowUpRight,
  Check,
  Copy,
  Loader2,
  NotebookPen,
  Sparkles,
  Trash2,
} from 'lucide-react';

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
 * Quiet bottom rail for the reader. Ghost-variant buttons, text+icon pairs.
 * Spec §5.6.
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
    <div className="mt-6 flex items-center justify-between border-t border-[hsl(var(--border-subtle))] pt-4">
      <div className="flex items-center gap-4">
        <button
          type="button"
          onClick={onOpenInChat}
          disabled={originDisabled}
          title={originDisabled ? originDisabledReason : 'Open in Chat'}
          className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))] transition-colors duration-fast disabled:cursor-not-allowed disabled:text-[hsl(var(--text-disabled))] disabled:hover:text-[hsl(var(--text-disabled))]"
        >
          <ArrowUpRight className="h-3.5 w-3.5" strokeWidth={1.75} />
          Open in Chat
        </button>
        <button
          type="button"
          onClick={() => void handleCopy()}
          className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))] transition-colors duration-fast"
          title="Copy reference content"
        >
          {copiedTick ? (
            <Check className="h-3.5 w-3.5 text-[hsl(var(--accent))]" strokeWidth={2} />
          ) : (
            <Copy className="h-3.5 w-3.5" strokeWidth={1.75} />
          )}
          Copy
        </button>
        <button
          type="button"
          onClick={() => void handleCapture()}
          disabled={capturing}
          title={captureDestinationLabel}
          className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))] transition-colors duration-fast disabled:cursor-not-allowed"
        >
          {capturing ? (
            <Loader2 className="h-3.5 w-3.5 text-[hsl(var(--text-muted))]" strokeWidth={1.75} />
          ) : (
            <Sparkles className="h-3.5 w-3.5" strokeWidth={1.75} />
          )}
          {isCaptured ? 'Re-capture' : 'Capture'}
        </button>
        {isCaptured && (
          <button
            type="button"
            onClick={onOpenCapturedNote}
            className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))] transition-colors duration-fast"
            title="Open the note this reference was captured to"
          >
            <NotebookPen className="h-3.5 w-3.5" strokeWidth={1.75} />
            Open captured note
          </button>
        )}
      </div>
      <button
        type="button"
        onClick={onDelete}
        className="inline-flex items-center gap-1.5 text-xs text-[hsl(var(--text-muted))] hover:text-[hsl(var(--danger-fg))] transition-colors duration-fast"
        title="Delete reference"
      >
        <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
        Delete
      </button>
    </div>
  );
}
