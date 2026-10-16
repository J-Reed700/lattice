import { Link } from 'react-router';

/**
 * The one line above the composer that says what is wrong and what to do about
 * it (UX: "every degraded state is actionable").
 *
 * Replaces a placeholder that claimed "Chat model is downloading…" for every
 * absent model — including one that was never downloaded, and including a
 * working Ollama-only install, where the send button sat dead forever.
 */

export interface ChatModelNoticeProps {
  /** A chat model is reachable: a local active model, or a configured Ollama. */
  hasChatModel: boolean;
  /** Warmup phase from `modelWarmupStore`. */
  warmupPhase: 'idle' | 'started' | 'ready' | 'skipped' | 'failed';
  /**
   * The backend's own reason the knowledge base could not be searched, from
   * the retrieval trace. Never inferred in the frontend.
   */
  retrievalUnavailableReason?: string | null;
  /**
   * This chat chose a model it cannot use now (deleted, or its connection
   * removed): the reason, and the way back to the app default. The chat is
   * never moved to another model by itself.
   */
  unavailableModel?: { reason: string; onUseDefault: () => void } | null;
}

const ROW_CLASS =
  'flex items-center justify-between gap-3 border-t border-subtle px-6 py-2 text-xs text-[hsl(var(--text-secondary))]';
const ACTION_CLASS =
  'shrink-0 text-[hsl(var(--accent))] underline-offset-2 hover:underline';

export function ChatModelNotice({
  hasChatModel,
  warmupPhase,
  retrievalUnavailableReason,
  unavailableModel,
}: ChatModelNoticeProps) {
  // First match wins: the most blocking problem is the one worth naming.
  if (unavailableModel) {
    return (
      <div className={ROW_CLASS}>
        <span>{unavailableModel.reason}</span>
        <span className="flex shrink-0 items-center gap-3">
          <button type="button" onClick={unavailableModel.onUseDefault} className={ACTION_CLASS}>
            Use default
          </button>
          <Link to="/settings" className={ACTION_CLASS}>
            Open model settings
          </Link>
        </span>
      </div>
    );
  }

  if (warmupPhase === 'failed') {
    return (
      <div className={ROW_CLASS}>
        <span>The chat model didn&apos;t load.</span>
        <Link to="/settings" className={ACTION_CLASS}>
          Open model settings
        </Link>
      </div>
    );
  }

  if (!hasChatModel) {
    return (
      <div className={ROW_CLASS}>
        <span>No chat model yet.</span>
        <Link to="/settings" className={ACTION_CLASS}>
          Choose a model
        </Link>
      </div>
    );
  }

  if (warmupPhase === 'started') {
    return (
      <div className={ROW_CLASS}>
        <span>Warming up the model…</span>
      </div>
    );
  }

  if (retrievalUnavailableReason) {
    return (
      <div className={ROW_CLASS}>
        <span>Document search was unavailable for the last answer: {retrievalUnavailableReason}.</span>
        <Link to={retrievalUnavailableReason.includes('model') ? '/settings' : '/files'} className={ACTION_CLASS}>
          {retrievalUnavailableReason.includes('model') ? 'Open model settings' : 'Review documents'}
        </Link>
      </div>
    );
  }

  return null;
}
