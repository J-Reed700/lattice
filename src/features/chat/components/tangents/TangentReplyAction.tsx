import { useContext, useId, useRef, useState } from 'react';

import { GitBranch } from 'lucide-react';

import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';

import { MAX_TANGENT_PASSAGE_LENGTH, TangentSelectionContext } from './TangentSelection';

export interface TangentReplySource {
  conversationId: string;
  messageId: string;
  getText: () => string;
}

/** A visible, keyboard-accessible entry point alongside the other reply actions. */
export function TangentReplyAction({ source, className, disabled }: {
  source: TangentReplySource;
  className: string;
  disabled: boolean;
}) {
  const tangents = useContext(TangentSelectionContext);
  const [passage, setPassage] = useState<string | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const passageId = useId();
  if (!tangents) return null;

  const start = (text: string) => tangents.create({
    conversationId: source.conversationId, messageId: source.messageId, selectedText: text,
  });
  const length = Array.from(passage?.trim() ?? '').length;

  return <>
    <button ref={buttonRef} type="button" className={className} disabled={disabled || tangents.creating}
      aria-label="Start a tangent from this reply" title="Ask about this reply in a tangent"
      onClick={() => {
        const text = source.getText().trim();
        if (!text) return;
        if (Array.from(text).length <= MAX_TANGENT_PASSAGE_LENGTH) start(text);
        else setPassage(text);
      }}>
      <GitBranch aria-hidden="true" className="h-3.5 w-3.5" />Tangent
    </button>
    <Dialog open={passage !== null} onOpenChange={open => { if (!open) setPassage(null); }}>
      <DialogContent onCloseAutoFocus={event => {
        event.preventDefault();
        // A newly opened tangent focuses its own composer. Only restore the
        // trigger when dismissing the picker leaves focus on the page body.
        if (document.activeElement === document.body) buttonRef.current?.focus();
      }}>
        <DialogTitle>Choose a starting passage</DialogTitle>
        <DialogDescription>This reply is long. Keep the part you want to explore, up to 8,000 characters. The tangent will still have the conversation as context.</DialogDescription>
        <label htmlFor={passageId} className="text-sm font-medium">Passage to explore</label>
        <textarea id={passageId} value={passage ?? ''} rows={8} onChange={event => setPassage(event.target.value)}
          aria-describedby={`${passageId}-length`}
          className="w-full resize-y rounded-lg border border-border-subtle bg-surface p-3 text-sm leading-relaxed outline-none focus-visible:ring-2 focus-visible:ring-ring" />
        <div className="flex items-center justify-between gap-3">
          <span id={`${passageId}-length`} className="text-xs text-text-muted">{length.toLocaleString()} / 8,000 characters</span>
          <button type="button" disabled={!length || length > MAX_TANGENT_PASSAGE_LENGTH || disabled || tangents.creating}
            className="rounded-lg bg-accent px-3 py-2 text-sm text-[hsl(var(--accent-fg))] disabled:opacity-40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            onClick={() => { start(passage!.trim()); setPassage(null); }}>Start tangent</button>
        </div>
      </DialogContent>
    </Dialog>
  </>;
}
