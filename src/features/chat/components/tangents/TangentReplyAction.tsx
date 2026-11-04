import { useContext } from 'react';

import { GitBranch } from 'lucide-react';

import { TangentSelectionContext } from './TangentSelection';

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
  if (!tangents) return null;

  return (
    <button type="button" className={className} disabled={disabled || tangents.creating}
      aria-label="Start a tangent from this reply" title="Ask about this reply in a tangent"
      onClick={() => {
        const text = source.getText().trim();
        if (!text) return;
        tangents.create({ conversationId: source.conversationId, messageId: source.messageId, selectedText: text });
      }}>
      <GitBranch aria-hidden="true" className="h-3.5 w-3.5" />Tangent
    </button>
  );
}
