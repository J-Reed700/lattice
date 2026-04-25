import { FileUp } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

import { type CorpusIdentity } from './hooks/useCorpusIdentity';
import Button from '../ui/Button/Button';

interface CorpusIdentityBandProps {
  identity: CorpusIdentity;
}

const tonalClassForIndex = (index: number, total: number): string => {
  if (total <= 1) return 'bg-[hsl(var(--text-secondary))]';
  const palette = [
    'bg-[hsl(var(--text-secondary))]',
    'bg-[hsl(var(--text-tertiary))]',
    'bg-[hsl(var(--text-muted))]',
    'bg-[hsl(var(--text-muted))]',
    'bg-[hsl(var(--text-muted))]',
  ];
  return palette[Math.min(index, palette.length - 1)];
};

/**
 * Corpus Identity Band — §4 + §15.5 (Block 3 deleted) + §15.6 (adaptive by N).
 *
 * - N = 0     → empty-vault state with CTA.
 * - N = 1-9   → nascent-vault line only, no Block 2.
 * - N >= 10   → full band: lede + subline + type-breakdown bar + legend.
 */
export function CorpusIdentityBand({ identity }: CorpusIdentityBandProps) {
  const navigate = useNavigate();

  if (identity.mode === 'empty') {
    return (
      <section
        aria-label="Empty corpus"
        className="px-8 pt-10 pb-10"
      >
        <div className="flex flex-col items-start gap-2">
          <h1 className="font-serif text-2xl font-semibold tracking-[-0.015em] text-[hsl(var(--text-primary))]">
            An empty vault — waiting.
          </h1>
          <p className="text-sm text-[hsl(var(--text-tertiary))]">
            Add your first folder or import a URL to begin.
          </p>
          <Button
            onClick={() => navigate('/ingest')}
            className="mt-4 inline-flex items-center gap-2"
          >
            <FileUp className="h-4 w-4" strokeWidth={1.75} />
            Add files
          </Button>
        </div>
      </section>
    );
  }

  if (identity.mode === 'nascent') {
    return (
      <section
        aria-label="Corpus identity"
        className="px-8 pt-10 pb-8"
      >
        <h1 className="font-serif text-2xl font-semibold tracking-[-0.015em] text-[hsl(var(--text-primary))]">
          {identity.ledeCount}
        </h1>
      </section>
    );
  }

  return (
    <section
      aria-label="Corpus identity"
      className="px-8 pt-10 pb-8"
    >
      <div className="grid items-baseline gap-x-12 gap-y-6 md:grid-cols-2">
        <div>
          <h1 className="font-serif text-2xl font-semibold tracking-[-0.015em] text-[hsl(var(--text-primary))]">
            {identity.ledeCount}
          </h1>
          {identity.subline && (
            <p className="mt-1 font-serif text-lg italic text-[hsl(var(--text-secondary))]">
              {identity.subline}
            </p>
          )}
        </div>

        <div>
          <TypeBreakdownBar identity={identity} />
          {identity.legend && (
            <p className="mt-2 text-xs text-[hsl(var(--text-tertiary))]">
              {identity.legend}
            </p>
          )}
        </div>
      </div>
    </section>
  );
}

function TypeBreakdownBar({ identity }: { identity: CorpusIdentity }) {
  const total = identity.totalCount;
  if (total === 0 || identity.buckets.length === 0) {
    return null;
  }

  return (
    <TooltipProvider delayDuration={120}>
      <div
        className="flex h-2.5 w-full overflow-hidden rounded-[var(--radius-sm)]"
        role="img"
        aria-label={`Type breakdown: ${identity.legend}`}
      >
        {identity.buckets.map((bucket, index) => {
          const width = `${(bucket.count / total) * 100}%`;
          const isLast = index === identity.buckets.length - 1;
          return (
            <Tooltip key={bucket.key}>
              <TooltipTrigger asChild>
                <div
                  style={{ width }}
                  className={`${tonalClassForIndex(index, identity.buckets.length)} ${
                    isLast ? '' : 'border-r border-[hsl(var(--bg))]'
                  } transition-colors duration-fast`}
                  aria-hidden="true"
                />
              </TooltipTrigger>
              <TooltipContent side="top">
                <span className="text-xs">
                  {bucket.label} · {bucket.count.toLocaleString()} {bucket.count === 1 ? 'document' : 'documents'} · {bucket.percentage}%
                </span>
              </TooltipContent>
            </Tooltip>
          );
        })}
      </div>
    </TooltipProvider>
  );
}
