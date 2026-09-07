import { useEffect, useRef, useState } from 'react';

/**
 * Inline editor for a user turn (BRIEF rank 4).
 *
 * Two ways out, and the consequence of the default one is stated rather than
 * discovered: sending replaces the turns after this message, branching leaves
 * the original thread untouched.
 */

export interface MessageEditorProps {
  initialValue: string;
  isBusy: boolean;
  onCancel: () => void;
  onSend: (_value: string) => Promise<void>;
  onSendAsBranch: (_value: string) => Promise<void>;
}

export function MessageEditor({
  initialValue,
  isBusy,
  onCancel,
  onSend,
  onSendAsBranch,
}: MessageEditorProps) {
  const [value, setValue] = useState(initialValue);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.focus();
    textarea.setSelectionRange(textarea.value.length, textarea.value.length);
  }, []);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = 'auto';
    textarea.style.height = `${textarea.scrollHeight}px`;
  }, [value]);

  const trimmed = value.trim();
  const canSend = trimmed.length > 0 && !isBusy;

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      onCancel();
      return;
    }
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      if (canSend) void onSend(trimmed);
    }
  };

  return (
    <div className="mt-1">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={handleKeyDown}
        rows={1}
        aria-label="Edit this message"
        className="w-full resize-none rounded-md border border-border-default bg-surface px-4 py-3 font-sans text-base text-[hsl(var(--text-primary))] outline-none focus-visible:ring-2 focus-visible:ring-ring"
      />
      <div className="mt-2 flex items-center justify-between gap-3">
        <span className="text-xs text-[hsl(var(--text-muted))]">
          Sending replaces the turns after this one.
        </span>
        <div className="flex items-center gap-3 text-xs">
          <button
            type="button"
            onClick={onCancel}
            className="text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))]"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void onSendAsBranch(trimmed)}
            disabled={!canSend}
            className="text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-50"
          >
            Send as branch
          </button>
          <button
            type="button"
            onClick={() => void onSend(trimmed)}
            disabled={!canSend}
            className="text-[hsl(var(--accent))] transition-colors duration-fast hover:text-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:opacity-50"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
