import { BookOpen, CornerDownRight, Database, Globe, Search, Telescope, Wrench, X } from 'lucide-react';

import { formatToolLabel } from './toolNames';

import type { TurnMode } from '../../../types';
import type { LucideIcon } from 'lucide-react';

/**
 * Every mode that is not the default, shown in the composer's bottom bar and
 * removable there.
 *
 * A turn that will take two hours should not be a checkbox behind a gear. The
 * gear popover stays for the whole list and for custom tools; what is actually
 * switched on is on screen beside the space and the model, where the question
 * is typed.
 */

export type ModeChipId = 'turn' | 'docs' | 'web' | 'wiki' | 'deep';

export interface ModeChipsProps {
  turnMode: TurnMode;
  knowledgeBase: boolean;
  webSearch: boolean;
  wikipedia: boolean;
  deepResearch: boolean;
  /** Names of the custom tools this turn would run. */
  customTools: string[];
  onRemove: (_id: ModeChipId) => void;
  onRemoveTool: (_name: string) => void;
}

interface Chip {
  key: string;
  label: string;
  icon: LucideIcon;
  /** What taking it off does, said plainly. */
  title: string;
  remove: () => void;
}

export function ModeChips({
  turnMode,
  knowledgeBase,
  webSearch,
  wikipedia,
  deepResearch,
  customTools,
  onRemove,
  onRemoveTool,
}: ModeChipsProps) {
  const chips: Chip[] = [];

  if (turnMode !== 'auto') {
    chips.push({
      key: 'turn',
      label: turnMode === 'followup' ? 'Follow-up' : 'Query',
      icon: turnMode === 'followup' ? CornerDownRight : Search,
      title: 'Back to Auto',
      remove: () => onRemove('turn'),
    });
  }
  if (deepResearch) {
    chips.push({
      key: 'deep',
      label: 'Deep research',
      icon: Telescope,
      title: 'Turn off deep research',
      remove: () => onRemove('deep'),
    });
  }
  if (knowledgeBase) {
    chips.push({
      key: 'docs',
      label: 'Your documents',
      icon: Database,
      title: 'Stop always searching your documents',
      remove: () => onRemove('docs'),
    });
  }
  if (webSearch) {
    chips.push({
      key: 'web',
      label: 'Web',
      icon: Globe,
      title: 'Turn off web search',
      remove: () => onRemove('web'),
    });
  }
  if (wikipedia) {
    chips.push({
      key: 'wiki',
      label: 'Wikipedia',
      icon: BookOpen,
      title: 'Turn off Wikipedia',
      remove: () => onRemove('wiki'),
    });
  }
  for (const name of customTools) {
    chips.push({
      key: `tool:${name}`,
      label: formatToolLabel(name),
      icon: Wrench,
      title: `Turn off ${formatToolLabel(name)}`,
      remove: () => onRemoveTool(name),
    });
  }

  if (chips.length === 0) return null;

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-1" aria-label="Modes this turn will use">
      {chips.map((chip) => (
        <span
          key={chip.key}
          className="inline-flex h-7 min-w-0 items-center gap-1 rounded-md bg-[hsl(var(--text-primary)/0.06)] pl-2 pr-1 text-xs text-[hsl(var(--text-secondary))]"
        >
          <chip.icon className="h-3 w-3 shrink-0 opacity-70" strokeWidth={1.75} aria-hidden="true" />
          <span className="truncate">{chip.label}</span>
          <button
            type="button"
            onClick={chip.remove}
            title={chip.title}
            aria-label={`${chip.label}: ${chip.title}`}
            className="pressable inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.08)] hover:text-[hsl(var(--text-primary))]"
          >
            <X className="h-3 w-3" strokeWidth={2} />
          </button>
        </span>
      ))}
    </div>
  );
}
