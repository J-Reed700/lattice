import { type ReactNode, useMemo, useState } from 'react';

import * as Popover from '@radix-ui/react-popover';
import { Check, Library } from 'lucide-react';

import { normalizeHexColor } from './sidebar/sidebarUtils';
import { useConversationsStore } from '../../stores/conversationsStore';

/**
 * Picks the space a chat belongs to — which is to say, the documents it can
 * search. Used by the label above the composer and wherever a chat is started
 * from outside Chat.
 *
 * A chat's space used to be decided by whatever the Chat sidebar last had
 * selected, including for chats started from the Journal, where that selection
 * is not even on screen. The answer a chat gives depends on this more than on
 * anything else, so it is shown where the question is typed and chosen where
 * the chat is made.
 */

export const GENERAL_SPACE_ID = 'space_general';

export interface SpacePickerPopoverProps {
  /** The space to mark as current, when there is a current one. */
  activeSpaceId?: string | null;
  onSelect: (_spaceId: string, _spaceName: string) => void | Promise<void>;
  /** One line above the list saying what is being chosen. */
  heading: string;
  children: ReactNode;
  align?: 'start' | 'end';
  side?: 'top' | 'bottom';
}

/** The spaces a chat can be put in: archived ones take no new chats. */
export function useOpenSpaces() {
  const spaces = useConversationsStore((s) => s.spaces);
  return useMemo(() => spaces.filter((space) => !space.isArchived), [spaces]);
}

export function SpacePickerPopover({
  activeSpaceId,
  onSelect,
  heading,
  children,
  align = 'start',
  side = 'top',
}: SpacePickerPopoverProps) {
  const [open, setOpen] = useState(false);
  const spaces = useOpenSpaces();

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>{children}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side={side}
          sideOffset={6}
          align={align}
          className="surface-pop z-50 w-[300px] rounded-xl bg-surface-overlay p-2 shadow-lg outline-none"
        >
          <p className="px-2 pb-1.5 pt-1 text-xs text-[hsl(var(--text-muted))]">{heading}</p>
          {spaces.map((space) => {
            const isActive = space.id === activeSpaceId;
            const accent = normalizeHexColor(space.accentColor);
            return (
              <button
                key={space.id}
                type="button"
                onClick={() => {
                  setOpen(false);
                  if (!isActive) void onSelect(space.id, space.name);
                }}
                className="flex w-full items-start gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)]"
              >
                <span className="flex h-5 w-5 shrink-0 items-center justify-center text-sm text-[hsl(var(--text-tertiary))]">
                  {space.icon ||
                    (accent ? (
                      <span
                        className="h-2 w-2 rounded-full"
                        style={{ backgroundColor: accent }}
                        aria-hidden="true"
                      />
                    ) : (
                      <Library className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
                    ))}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm text-[hsl(var(--text-primary))]">
                    {space.name}
                  </span>
                  <span className="line-clamp-2 text-xs leading-snug text-[hsl(var(--text-muted))]">
                    {space.id === GENERAL_SPACE_ID
                      ? 'Filed in General, or not filed anywhere'
                      : space.description || `Only documents filed in ${space.name}`}
                  </span>
                </span>
                {isActive && (
                  <Check className="mt-1 h-3.5 w-3.5 shrink-0 text-[hsl(var(--accent))]" aria-hidden="true" />
                )}
              </button>
            );
          })}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
