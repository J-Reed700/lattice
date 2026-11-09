import type React from 'react';
import { useId, useRef } from 'react';
import { Lock } from 'lucide-react';

export interface FileTabItem {
  path: string;
  label?: string;
  badge?: React.ReactNode;
  readOnly?: boolean;
  changed?: boolean;
}

export type FileTab = FileTabItem;

export interface FileTabsProps {
  files: FileTabItem[];
  activePath?: string;
  active?: string;
  onSelect: (path: string) => void;
  actions?: React.ReactNode;
  className?: string;
  idBase?: string;
  panelId?: string;
  ariaLabel?: string;
  label?: string;
  children?: React.ReactNode;
}

export function FileTabs({
  files,
  activePath,
  active,
  onSelect,
  actions,
  className = '',
  idBase,
  panelId,
  ariaLabel,
  label,
  children,
}: FileTabsProps) {
  const generatedId = useId();
  const baseId = idBase ?? generatedId;
  const effectiveActive = activePath ?? active ?? files[0]?.path ?? '';
  const effectiveLabel = label ?? ariaLabel ?? 'Files';
  const effectivePanelId = panelId ?? (children ? `${baseId}-panel` : undefined);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const handleKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (!files.length) return;
    let targetIndex: number | null = null;

    if (event.key === 'ArrowRight') {
      targetIndex = (index + 1) % files.length;
    } else if (event.key === 'ArrowLeft') {
      targetIndex = (index - 1 + files.length) % files.length;
    } else if (event.key === 'Home') {
      targetIndex = 0;
    } else if (event.key === 'End') {
      targetIndex = files.length - 1;
    }

    if (targetIndex !== null) {
      event.preventDefault();
      const targetFile = files[targetIndex];
      if (targetFile) {
        onSelect(targetFile.path);
        tabRefs.current[targetIndex]?.focus();
      }
    }
  };

  const activeIndex = Math.max(0, files.findIndex((f) => f.path === effectiveActive));

  return (
    <>
      <div
        className={`flex flex-wrap items-center justify-between gap-2 border-b border-border-default bg-bg p-2 ${className}`}
      >
        <div
          role="tablist"
          aria-label={effectiveLabel}
          className="flex min-w-0 flex-wrap items-center gap-1"
        >
          {files.map((file, index) => {
            const isActive = file.path === effectiveActive;
            const displayLabel = file.label ?? file.path;

            return (
              <button
                key={file.path}
                id={`${baseId}-${index}`}
                aria-controls={effectivePanelId}
                ref={(el) => {
                  tabRefs.current[index] = el;
                }}
                type="button"
                role="tab"
                aria-selected={isActive}
                tabIndex={isActive ? 0 : -1}
                onClick={() => onSelect(file.path)}
                onKeyDown={(e) => handleKeyDown(e, index)}
                className={`inline-flex max-w-full items-center gap-2 rounded-lg px-3 py-1.5 font-mono text-xs transition focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent ${
                  isActive
                    ? 'bg-surface text-accent font-semibold shadow-xs'
                    : 'text-text-secondary hover:bg-surface-sunken hover:text-text-primary'
                }`}
              >
                <span className="truncate">{displayLabel}</span>

                {file.readOnly && (
                  <span className="inline-flex items-center gap-1 text-text-muted">
                    <Lock size={12} aria-hidden="true" />
                    <span className="font-sans text-xs">Read-only</span>
                  </span>
                )}

                {file.changed && !file.badge && (
                  <span className="rounded-full bg-surface-raised px-2 py-0.5 font-sans text-xs text-text-muted">
                    Edited
                  </span>
                )}

                {typeof file.badge === 'string' ? (
                  <span className="rounded-full bg-surface-raised px-2 py-0.5 font-sans text-xs text-text-muted">
                    {file.badge}
                  </span>
                ) : (
                  file.badge
                )}
              </button>
            );
          })}
        </div>

        {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </div>
      {children !== undefined && (
        <div
          id={effectivePanelId}
          role="tabpanel"
          aria-labelledby={`${baseId}-${activeIndex}`}
        >
          {children}
        </div>
      )}
    </>
  );
}
