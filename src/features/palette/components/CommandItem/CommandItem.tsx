import { Command } from 'cmdk'
import { type LucideIcon } from 'lucide-react'

export interface CommandItemProps {
  icon: LucideIcon
  label: string
  shortcut?: string
  description?: string
  onSelect: () => void
  value?: string
}

/**
 * CommandItem — one row in the command palette: icon, label, optional
 * shortcut at the right. Descriptions are rendered small and muted and
 * should be rare.
 */
export function CommandItem({ icon: Icon, label, shortcut, description, onSelect, value }: CommandItemProps) {
  return (
    <Command.Item
      value={value || label}
      onSelect={onSelect}
      className="command-item group rounded-lg outline-hidden transition-colors duration-75 aria-selected:!bg-accent-muted dark:aria-selected:!bg-accent-muted/70"
    >
      <div className="flex min-h-[34px] items-center gap-2.5 px-2.5 py-1.5">
        <Icon
          className="h-4 w-4 shrink-0 text-text-tertiary transition-colors duration-75 group-aria-selected:!text-accent"
          strokeWidth={1.75}
        />
        <div className="min-w-0 flex-1">
          <div className="truncate text-ui text-text-primary group-aria-selected:font-medium">{label}</div>
          {description ? <div className="truncate text-xs text-text-muted">{description}</div> : null}
        </div>
        {shortcut ? (
          <kbd className="command-shortcut inline-flex h-5 min-w-[20px] items-center justify-center rounded border border-border-default bg-surface px-1.5 font-sans text-[11px] font-medium !tracking-normal !text-text-secondary tabular-nums shadow-2xs group-aria-selected:border-accent/25 group-aria-selected:!text-accent">
            {shortcut}
          </kbd>
        ) : null}
      </div>
    </Command.Item>
  )
}
