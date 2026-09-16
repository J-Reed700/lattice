/**
 * AudioViewer
 *
 * Plays an indexed recording and lists the transcript passages a citation
 * pointed at. Clicking a row moves the playhead; opening from a citation starts
 * at the cited moment.
 */

import { type FC, useCallback, useEffect, useRef, useState } from 'react';

import { convertFileSrc } from '@tauri-apps/api/core';

import { cn } from '@/lib/utils';

/** One transcript passage, positioned in the recording. */
export interface AudioSegment {
  startMs: number;
  endMs: number;
  text: string;
  chunkId?: string;
}

interface AudioViewerProps {
  filePath: string;
  segments?: AudioSegment[];
  seekToMs?: number;
}

/** How often the playhead updates the highlighted row (4 Hz). */
const HIGHLIGHT_INTERVAL_MS = 250;

/** `"12:40"` or `"1:02:40"`. Mirrors the backend's `format_timestamp`. */
export function formatTimestamp(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const seconds = totalSeconds % 60;
  const minutes = Math.floor(totalSeconds / 60) % 60;
  const hours = Math.floor(totalSeconds / 3600);

  const paddedSeconds = String(seconds).padStart(2, '0');
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, '0')}:${paddedSeconds}`;
  }
  return `${minutes}:${paddedSeconds}`;
}

function parseClock(clock: string): number | undefined {
  const parts = clock.split(':');
  if (parts.length < 2 || parts.length > 3) return undefined;

  let total = 0;
  for (const part of parts) {
    if (!/^\d+$/.test(part)) return undefined;
    total = total * 60 + Number(part);
  }
  return total * 1000;
}

/** `"12:40"` or `"1:02:40"` → 760000. Accepts the `"mm:ss–mm:ss"` section. */
export function parseSectionStartMs(section?: string): number | undefined {
  if (!section) return undefined;
  const [start] = section.split('–');
  return start ? parseClock(start.trim()) : undefined;
}

/** The end of a `"mm:ss–mm:ss"` section, when it has one. */
export function parseSectionEndMs(section?: string): number | undefined {
  if (!section) return undefined;
  const parts = section.split('–');
  return parts.length > 1 ? parseClock((parts[1] ?? '').trim()) : undefined;
}

/**
 * Build AudioViewer props from a citation source: the cited chunk excerpts
 * become the segment list (each carries its own `section`), and the top-level
 * `section` sets the seek.
 */
export function audioViewerPropsFromSource(source: {
  section?: string;
  content?: string;
  chunkExcerpts?: Array<{ chunkId: string; excerpt: string; section?: string }>;
}): { segments?: AudioSegment[]; seekToMs?: number } {
  const seekToMs = parseSectionStartMs(source.section);

  const fromExcerpts: AudioSegment[] = [];
  for (const excerpt of source.chunkExcerpts ?? []) {
    const startMs = parseSectionStartMs(excerpt.section);
    if (startMs === undefined) continue;
    fromExcerpts.push({
      startMs,
      endMs: parseSectionEndMs(excerpt.section) ?? startMs + 1,
      text: excerpt.excerpt,
      chunkId: excerpt.chunkId,
    });
  }
  fromExcerpts.sort((a, b) => a.startMs - b.startMs);

  if (fromExcerpts.length > 0) {
    return { segments: fromExcerpts, seekToMs };
  }

  if (seekToMs !== undefined && source.content) {
    return {
      segments: [
        {
          startMs: seekToMs,
          endMs: parseSectionEndMs(source.section) ?? seekToMs + 1,
          text: source.content,
        },
      ],
      seekToMs,
    };
  }

  return { seekToMs };
}

export const AudioViewer: FC<AudioViewerProps> = ({ filePath, segments, seekToMs }) => {
  const audioRef = useRef<HTMLAudioElement>(null);
  const lastHighlightAt = useRef(0);
  const [currentMs, setCurrentMs] = useState(0);
  const [failed, setFailed] = useState(false);

  // The modal keeps one AudioViewer mounted and swaps `filePath` as the reader
  // travels citations, so a failure on one recording must not stick to the next.
  useEffect(() => {
    setFailed(false);
  }, [filePath]);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || seekToMs === undefined) return;

    const seek = () => {
      audio.currentTime = seekToMs / 1000;
    };

    if (audio.readyState > 0) seek();
    audio.addEventListener('loadedmetadata', seek);
    return () => audio.removeEventListener('loadedmetadata', seek);
  }, [seekToMs, filePath]);

  const handleTimeUpdate = useCallback(() => {
    const now = Date.now();
    if (now - lastHighlightAt.current < HIGHLIGHT_INTERVAL_MS) return;
    lastHighlightAt.current = now;
    setCurrentMs((audioRef.current?.currentTime ?? 0) * 1000);
  }, []);

  const seekTo = useCallback((ms: number) => {
    const audio = audioRef.current;
    if (!audio) return;
    audio.currentTime = ms / 1000;
    setCurrentMs(ms);
  }, []);

  const hasSegments = (segments?.length ?? 0) > 0;

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 p-4">
      <audio
        ref={audioRef}
        controls
        preload="metadata"
        src={convertFileSrc(filePath)}
        className="w-full shrink-0"
        aria-label="Recording playback"
        onTimeUpdate={handleTimeUpdate}
        onError={() => setFailed(true)}
      />

      {failed && (
        <p className="text-sm text-danger-fg">Couldn&apos;t play this audio file.</p>
      )}

      {!hasSegments && seekToMs !== undefined && (
        <p className="text-sm text-text-muted">Cited at {formatTimestamp(seekToMs)}</p>
      )}

      {hasSegments && (
        <div className="flex min-h-0 flex-1 flex-col gap-2">
          <h3 className="shrink-0 text-sm font-medium text-text-primary">Transcript</h3>
          <ul className="min-h-0 flex-1 divide-y divide-border-subtle overflow-y-auto border-t border-border-subtle">
            {segments?.map((segment) => {
              const isPlaying = currentMs >= segment.startMs && currentMs < segment.endMs;
              const isCited =
                seekToMs !== undefined &&
                seekToMs >= segment.startMs &&
                seekToMs < Math.max(segment.endMs, segment.startMs + 1);

              return (
                <li key={segment.chunkId ?? `${segment.startMs}-${segment.endMs}`}>
                  <button
                    type="button"
                    onClick={() => seekTo(segment.startMs)}
                    title={segment.text}
                    className={cn(
                      'flex w-full items-start gap-3 px-2 py-2 text-left transition-colors duration-fast hover:bg-surface-raised',
                      isPlaying && 'bg-surface-raised'
                    )}
                  >
                    <span
                      className={cn(
                        'shrink-0 font-mono text-xs tabular-nums text-text-tertiary',
                        isCited && 'text-accent'
                      )}
                    >
                      {formatTimestamp(segment.startMs)}
                    </span>
                    <span className="line-clamp-2 text-sm text-text-secondary">{segment.text}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
};
