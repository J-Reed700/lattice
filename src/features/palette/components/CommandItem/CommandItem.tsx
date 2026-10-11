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
    <Command.Item value={value || label} onSelect={onSelect} className="command-item">
      <div className="flex h-9 items-center gap-3 px-2.5">
        <Icon className="h-4 w-4 shrink-0 text-text-tertiary" strokeWidth={1.6} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13.5px] text-text-primary">{label}</div>
          {description ? <div className="truncate text-xs text-text-muted">{description}</div> : null}
        </div>
        {shortcut ? <kbd className="command-shortcut">{shortcut}</kbd> : null}
      </div>
    </Command.Item>
  )
}
