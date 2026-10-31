import { Loader2, Pause, Play, Square, Volume2 } from 'lucide-react';

import { cn } from '@/lib/utils';

import { useListen } from '../hooks/useSpeech';

interface ListenButtonProps {
  /** Which text this is, so only its own button shows it speaking. */
  sourceId: string;
  /** Markdown; read-aloud says only its prose. */
  markdown: string;
  /** What is being read, for the buttons' labels. */
  subject?: string;
  className?: string;
}

/** Listen, then pause or resume and stop, for one answer or lesson section. */
export function ListenButton({ sourceId, markdown, subject = 'this', className }: ListenButtonProps) {
  const { status, play, pause, resume, stop } = useListen(sourceId, markdown);

  if (status === 'idle') {
    return (
      <button type="button" onClick={play} aria-label={`Listen to ${subject}`} title="Listen" className={className}>
        <Volume2 className="h-3.5 w-3.5" aria-hidden="true" />
        Listen
      </button>
    );
  }

  return (
    <span className="inline-flex items-center gap-0.5" role="group" aria-label={`Reading ${subject} aloud`}>
      {status === 'loading' ? (
        <span role="status" className={cn(className, 'cursor-default')}>
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          Starting…
        </span>
      ) : status === 'playing' ? (
        <button type="button" onClick={pause} aria-label={`Pause reading ${subject}`} title="Pause" className={className}>
          <Pause className="h-3.5 w-3.5" aria-hidden="true" />
          Pause
        </button>
      ) : (
        <button type="button" onClick={resume} aria-label={`Resume reading ${subject}`} title="Resume" className={className}>
          <Play className="h-3.5 w-3.5" aria-hidden="true" />
          Resume
        </button>
      )}
      <button type="button" onClick={stop} aria-label={`Stop reading ${subject}`} title="Stop" className={className}>
        <Square className="h-3 w-3" aria-hidden="true" />
        Stop
      </button>
    </span>
  );
}
