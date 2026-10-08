import { type ReactNode } from 'react';

import { cn } from '@/lib/utils';

/**
 * SettingsSection / SettingsRow
 *
 * Shared settings anatomy. The Settings workspace adds grouped surfaces;
 * embedded forms retain their lightweight rows.
 */

interface SettingsSectionProps {
  title: string;
  /** Only when the section name alone is genuinely ambiguous. One line. */
  description?: string;
  /** Optional right-aligned control for the whole section (e.g. "Add folder"). */
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function SettingsSection({ title, description, actions, children, className }: SettingsSectionProps) {
  return (
    <section className={cn('settings-section mb-10', className)}>
      <div className="settings-section-heading flex items-end justify-between gap-4 pb-2">
        <div className="min-w-0">
          <h2 className="text-base font-medium text-text-primary">{title}</h2>
          {description ? <p className="mt-0.5 text-sm text-text-tertiary">{description}</p> : null}
        </div>
        {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
      </div>
      <div className="settings-section-body border-t border-border-subtle">{children}</div>
    </section>
  );
}

interface SettingsRowProps {
  label: string;
  /** One short line under the label. Use sparingly. */
  hint?: ReactNode;
  /** Associates the label with the control for screen readers. */
  htmlFor?: string;
  /** Stack the control under the label instead of right-aligning it. Use for textareas, lists, wide inputs. */
  stacked?: boolean;
  children?: ReactNode;
  className?: string;
}

export function SettingsRow({ label, hint, htmlFor, stacked = false, children, className }: SettingsRowProps) {
  const LabelTag = htmlFor ? 'label' : 'div';
  return (
    <div
      className={cn(
        'settings-row border-b border-border-subtle py-3',
        stacked && 'settings-row-stacked',
        stacked ? 'flex flex-col gap-2' : 'flex items-center justify-between gap-6',
        className,
      )}
    >
      <div className="min-w-0">
        <LabelTag htmlFor={htmlFor} className="block text-sm font-medium text-text-primary">
          {label}
        </LabelTag>
        {hint ? <div className="mt-1 max-w-[60ch] text-xs leading-relaxed text-text-muted">{hint}</div> : null}
      </div>
      {children ? (
        <div className={cn('settings-row-control', stacked ? 'w-full' : 'flex shrink-0 items-center justify-end min-w-[220px]')}>
          {children}
        </div>
      ) : null}
    </div>
  );
}

/**
 * Field styles shared by Settings inputs so every pane looks the same.
 * Spread onto <input>, <select>, or <textarea>.
 */
export const settingsFieldClass =
  'h-8 w-full rounded-md border border-border-default bg-surface px-2.5 text-ui text-text-primary shadow-control placeholder:text-text-muted outline-hidden transition-[border-color,box-shadow] duration-fast hover:border-border-strong focus:border-accent/70 focus:shadow-[0_0_0_3px_hsl(var(--accent)/0.14)] disabled:cursor-not-allowed disabled:opacity-50';

export const settingsTextareaClass =
  'w-full rounded-md border border-border-default bg-surface px-3 py-2.5 font-mono text-xs leading-relaxed text-text-primary shadow-control placeholder:text-text-muted outline-hidden transition-[border-color,box-shadow] duration-fast hover:border-border-strong focus:border-accent/70 focus:shadow-[0_0_0_3px_hsl(var(--accent)/0.14)] disabled:cursor-not-allowed disabled:opacity-50';
