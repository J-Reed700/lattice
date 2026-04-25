import { Command } from 'cmdk';

export interface SpotlightGroupProps {
  heading: string;
  /**
   * When true the group renders nothing. We prefer to early-return at
   * the group boundary rather than rendering an empty heading — keeps
   * the palette compact as groups come and go.
   */
  hidden?: boolean;
  children: React.ReactNode;
}

/**
 * SpotlightGroup
 *
 * Visual wrapper for a single cmdk group with the editorial-eyebrow
 * heading used across redesigned surfaces (`text-xxs uppercase
 * tracking-[0.08em]`, muted text).
 *
 * Groups with no children should pass `hidden` and render nothing —
 * cmdk itself will also hide groups whose items all get filtered out,
 * but gating on `hidden` lets us avoid the wasted map entirely.
 */
export function SpotlightGroup({ heading, hidden, children }: SpotlightGroupProps) {
  if (hidden) return null;

  return (
    <Command.Group
      heading={heading}
      className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-3 [&_[cmdk-group-heading]]:text-xxs [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-[0.08em] [&_[cmdk-group-heading]]:text-[hsl(var(--text-tertiary))]"
    >
      {children}
    </Command.Group>
  );
}
