import type { CSSProperties } from 'react';

import { cn } from '@/lib/utils';

import type { IrisMood, IrisTone } from '../model/irisReading';

import './iris.css';

interface IrisOrbProps {
  mood: IrisMood;
  tone: IrisTone;
  /** 0–1 for the ring while working; null spins an open arc instead. */
  progress: number | null;
  /** Lens diameter in pixels. */
  size?: number;
  className?: string;
}

/** The light itself. Decorative: whoever renders it says what it means. */
export function IrisOrb({ mood, tone, progress, size = 18, className }: IrisOrbProps) {
  const ringing = mood === 'working' || mood === 'thinking';
  return (
    <span
      aria-hidden="true"
      data-testid="iris-orb"
      data-mood={mood}
      data-tone={tone}
      className={cn('iris', className)}
      style={{ '--iris-size': `${size}px` } as CSSProperties}
    >
      <svg className="iris-ring" viewBox="0 0 36 36" data-indeterminate={ringing && progress === null ? '' : undefined}>
        <circle className="iris-ring-track" cx="18" cy="18" r="16.5" />
        {ringing && (
          <circle
            className="iris-ring-fill"
            cx="18"
            cy="18"
            r="16.5"
            pathLength={100}
            style={progress === null ? undefined : { strokeDasharray: `${Math.max(2, progress * 100)} 100` }}
          />
        )}
      </svg>
      <span className="iris-lens">
        <span className="iris-light" />
        <span className="iris-glint" />
      </span>
    </span>
  );
}
