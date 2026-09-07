import { useMemo, useState } from 'react';

import { Pause, Play, RefreshCw, Square } from 'lucide-react';

import { cn } from '@/lib/utils';

import { formatIndexingAriaLabel, shouldShowIndexingRail, shouldSpin } from './formatIndexingStatus';
import { IndexingStatusPopover } from './IndexingStatusPopover';
import {
  useIndexingControlMutation,
  useIndexingStatusQuery,
} from '../../hooks/queries/useIndexingStatusQuery';
import { useRegisterPaletteCommands } from '../../hooks/useRegisterPaletteCommands';
import { Popover, PopoverContent, PopoverTrigger } from '../ui/popover';


import type { PaletteCommand } from '../../stores/paletteCommandsStore';

/**
 * The rail's indexing affordance. Renders nothing on an idle vault with no
 * failures, exactly like `HeaderDownloadsIndicator` — the rail earns its pixels
 * only while there is something to say.
 */
export function IndexingStatusRail() {
  const { data } = useIndexingStatusQuery();
  const control = useIndexingControlMutation();
  const [open, setOpen] = useState(false);

  const running = data?.status === 'scanning' || data?.status === 'processing';
  const paused = data?.paused ?? false;
  const mutate = control.mutate;

  const commands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'indexing.pause',
        label: 'Pause indexing',
        group: 'Vault',
        icon: Pause,
        enabled: running && !paused,
        run: () => mutate('pause'),
      },
      {
        id: 'indexing.resume',
        label: 'Resume indexing',
        group: 'Vault',
        icon: Play,
        enabled: paused,
        run: () => mutate('resume'),
      },
      {
        id: 'indexing.stop',
        label: 'Stop indexing',
        group: 'Vault',
        icon: Square,
        enabled: running || paused,
        run: () => mutate('cancel'),
      },
    ],
    [running, paused, mutate],
  );
  useRegisterPaletteCommands(commands);

  if (!shouldShowIndexingRail(data) || !data) return null;

  const label = formatIndexingAriaLabel(data);
  const hasFailures = data.failures.length > 0;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          title={label}
          aria-label={label}
          aria-expanded={open}
          className={cn(
            'relative flex h-9 w-9 items-center justify-center rounded-md transition-colors duration-fast',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-surface',
            open
              ? 'bg-accent-muted text-accent'
              : 'text-text-muted hover:bg-surface-raised hover:text-text-primary',
          )}
        >
          <RefreshCw
            className={cn('h-[18px] w-[18px]', shouldSpin(data) && 'animate-spin')}
            strokeWidth={1.75}
          />
          {hasFailures && !shouldSpin(data) ? (
            <span
              aria-hidden="true"
              className="absolute right-1 top-1 h-2 w-2 rounded-full bg-danger"
            />
          ) : null}
        </button>
      </PopoverTrigger>
      <PopoverContent side="right" align="end" sideOffset={10} className="w-[320px] p-0">
        <IndexingStatusPopover snapshot={data} open={open} onClose={() => setOpen(false)} />
      </PopoverContent>
    </Popover>
  );
}
