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
 * CommandItem
 *
 * Purpose: Individual command item with icon, label, and optional keyboard shortcut
 *
 * Features:
 * - Icon display using lucide-react
 * - Keyboard shortcut badge
 * - Hover and selected states
 * - Optional description text
 */
export function CommandItem({
  icon: Icon,
  label,
  shortcut,
  description,
  onSelect,
  value,
}: CommandItemProps) {
  return (
    <Command.Item
      value={value || label}
      onSelect={onSelect}
      className="command-item"
    >
      <div className="flex items-center flex-1 gap-3 px-4 py-3">
        <Icon className="w-4 h-4 text-[hsl(var(--text-tertiary))]" strokeWidth={1.75} />
        <div className="flex-1">
          <div className="text-sm font-medium text-[hsl(var(--text-primary))]">
            {label}
          </div>
          {description && (
            <div className="text-xs text-[hsl(var(--text-secondary))] mt-0.5">
              {description}
            </div>
          )}
        </div>
        {shortcut && (
          <kbd className="command-shortcut">
            {shortcut}
          </kbd>
        )}
      </div>
    </Command.Item>
  )
}
