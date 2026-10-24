import type { KeyboardEvent } from 'react';

const STEP: Record<string, number> = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 };

/**
 * Roving focus inside a group (radiogroup, question rail): arrows move, Home
 * and End jump. Returns the index moved to, or null when the key is not ours.
 */
export function moveRoving(event: KeyboardEvent<HTMLElement>, selector: string, keys: 'both' | 'horizontal' = 'both'): number | null {
  const horizontal = event.key === 'ArrowLeft' || event.key === 'ArrowRight';
  if (keys === 'horizontal' && !horizontal && event.key in STEP) return null;
  if (!(event.key in STEP) && event.key !== 'Home' && event.key !== 'End') return null;
  if (event.altKey || event.metaKey || event.ctrlKey) return null;
  const group = event.currentTarget.closest('[data-roving]') ?? event.currentTarget.parentElement;
  const items = Array.from(group?.querySelectorAll<HTMLElement>(selector) ?? []);
  const index = items.indexOf(event.currentTarget);
  if (index < 0 || items.length === 0) return null;
  event.preventDefault();
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + STEP[event.key] + items.length) % items.length;
  items[next]?.focus();
  return next;
}
