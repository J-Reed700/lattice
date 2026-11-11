import type React from 'react';
import { useRef } from 'react';

import { Pencil } from 'lucide-react';

import { cn } from '@/lib/utils';

export function FileTabs({
  files,
  active,
  onSelect,
  label,
}: {
  files: { path: string; label?: string; modified?: boolean }[];
  active: string;
  onSelect: (path: string) => void;
  label: string;
}) {
  const refs = useRef(new Map<string, HTMLButtonElement>());

  const move = (index: number) => {
    const target = files[(index + files.length) % files.length];
    if (!target) return;
    onSelect(target.path);
    refs.current.get(target.path)?.focus();
  };

  const onKeyDown = (event: React.KeyboardEvent, index: number) => {
    if (event.key === 'ArrowRight') move(index + 1);
    else if (event.key === 'ArrowLeft') move(index - 1);
    else if (event.key === 'Home') move(0);
    else if (event.key === 'End') move(files.length - 1);
    else return;
    event.preventDefault();
  };

  return (
    <div role="tablist" aria-label={label} className="flex flex-wrap gap-1">
      {files.map((file, index) => {
        const selected = file.path === active;
        return (
          <button
            key={file.path}
            ref={(node) => {
              if (node) refs.current.set(file.path, node);
              else refs.current.delete(file.path);
            }}
            type="button"
            role="tab"
            aria-selected={selected}
            tabIndex={selected ? 0 : -1}
            onClick={() => onSelect(file.path)}
            onKeyDown={(event) => onKeyDown(event, index)}
            className={cn(
              'inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium focus:outline-hidden focus:ring-2 focus:ring-ring',
              selected ? 'bg-accent-muted text-accent' : 'bg-surface-raised text-text-secondary hover:text-text-primary',
            )}
          >
            {file.label ?? file.path}
            {file.modified && (
              <span className="inline-flex items-center gap-1 text-text-muted">
                <Pencil size={11} aria-hidden="true" />
                Edited
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
