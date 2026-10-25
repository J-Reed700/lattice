import { lazy, Suspense, useCallback, useMemo, useState } from 'react';

import { Eye, EyeOff } from 'lucide-react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';
import { useRegisterPaletteCommands } from '@/features/palette/hooks/useRegisterPaletteCommands';
import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';

import { IrisOrb } from './IrisOrb';
import { useIrisReading } from '../hooks/useIrisReading';
import { irisLabel, type IrisDestination } from '../model/irisReading';
import { useIrisStore } from '../stores/irisStore';

const KEYWORDS = ['iris', 'assistant', 'companion', 'status', 'light'];

// Only shown on a click, so it stays out of the startup bundle.
const IrisCard = lazy(async () => ({ default: (await import('./IrisCard')).IrisCard }));

interface IrisProps {
  /** The rail is kept to icons on a wide window too. */
  compact: boolean;
}

/**
 * Iris's place in the rail. Always mounted so ⌘K can bring her back; reads
 * nothing while she is hidden.
 */
export function Iris({ compact }: IrisProps) {
  const enabled = useIrisStore((state) => state.enabled);
  const setEnabled = useIrisStore((state) => state.setEnabled);

  const commands = useMemo<PaletteCommand[]>(
    () => [
      { id: 'iris.show', label: 'Show Iris', group: 'Vault', icon: Eye, keywords: KEYWORDS, enabled: !enabled, run: () => setEnabled(true) },
      { id: 'iris.hide', label: 'Hide Iris', group: 'Vault', icon: EyeOff, keywords: KEYWORDS, enabled, run: () => setEnabled(false) },
    ],
    [enabled, setEnabled],
  );
  useRegisterPaletteCommands(commands);

  return enabled ? <IrisPresence compact={compact} /> : null;
}

function IrisPresence({ compact }: IrisProps) {
  const reading = useIrisReading();
  const markSeen = useIrisStore((state) => state.markSeen);
  const setEnabled = useIrisStore((state) => state.setEnabled);
  const navigate = useAppNavigate();
  const { openDrawer } = useDownloadState();
  const [open, setOpen] = useState(false);
  const label = irisLabel(reading);

  const noteIds = useMemo(() => reading.notes.map((note) => note.id), [reading.notes]);
  const changeOpen = useCallback(
    (next: boolean) => {
      // What the card showed has been seen once it closes, so the light
      // stays as it was while you read.
      if (!next) markSeen(noteIds);
      setOpen(next);
    },
    [markSeen, noteIds],
  );
  const close = useCallback(() => changeOpen(false), [changeOpen]);

  const go = (to: IrisDestination) => {
    if (to === 'downloads') openDrawer();
    else if (to === 'indexing') navigate('/settings?tab=indexing');
    else navigate('/studio?section=flashcards');
    close();
  };

  const hide = () => {
    close();
    setEnabled(false);
    toast.info('Iris is hidden', { message: 'Bring her back from ⌘K or Settings → Display.' });
  };

  return (
    <Popover open={open} onOpenChange={changeOpen}>
      <Tooltip delayDuration={500}>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <button
              type="button"
              aria-label={label}
              data-testid="iris"
              className={cn(
                'group relative flex h-9 w-full items-center justify-center gap-2.5 rounded-md px-2.5 transition-colors duration-fast',
                !compact && 'xl:justify-start',
                'focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                open ? 'bg-[hsl(var(--text-primary)/0.06)]' : 'hover:bg-[hsl(var(--text-primary)/0.04)]',
              )}
            >
              {/* The orb's row is 16px of icon in the other rail items; the lens sits a touch wider. */}
              <span className="relative flex h-4 w-4 items-center justify-center">
                <IrisOrb mood={reading.mood} tone={reading.tone} progress={reading.progress} size={16} />
              </span>
              <span className={cn('relative hidden min-w-0 flex-1 items-baseline justify-between gap-2', !compact && 'xl:flex')}>
                <span className="text-ui text-text-secondary group-hover:text-text-primary">Iris</span>
                <span className="truncate text-[11px] tabular-nums text-text-muted">{reading.short}</span>
              </span>
            </button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="right" sideOffset={10} className={compact ? undefined : 'xl:hidden'}>
          {reading.headline}
        </TooltipContent>
      </Tooltip>
      <PopoverContent side="right" align="end" sideOffset={12} className="w-[320px] p-0">
        <Suspense fallback={<div className="h-24" aria-busy="true" />}>
          <IrisCard reading={reading} onGo={go} onHide={hide} onClose={close} />
        </Suspense>
      </PopoverContent>
    </Popover>
  );
}
