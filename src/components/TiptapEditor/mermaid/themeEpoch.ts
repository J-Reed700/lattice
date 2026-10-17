import { useSyncExternalStore } from 'react';

/**
 * A number that changes whenever the app switches between light and dark.
 *
 * Diagrams are drawn with colours read from the theme tokens, so a drawn SVG
 * belongs to one theme. `useApplyTheme` sets `data-theme` and the `dark` class
 * on `<html>`; watching those is enough, and the epoch moves only when the
 * resulting theme differs, not on every unrelated class change.
 */

let epoch = 0;
let signature = '';
let observer: MutationObserver | null = null;
const listeners = new Set<() => void>();

function themeSignature(): string {
  const root = document.documentElement;
  return `${root.getAttribute('data-theme') ?? ''}:${root.classList.contains('dark') ? 'dark' : 'light'}`;
}

function onMutation() {
  const next = themeSignature();
  if (next === signature) return;
  signature = next;
  epoch += 1;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (!observer) {
    signature = themeSignature();
    observer = new MutationObserver(onMutation);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class'] });
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      observer?.disconnect();
      observer = null;
    }
  };
}

function snapshot(): number {
  // Between the last unsubscribe and the next subscribe nothing is watching;
  // catch up on a switch that happened meanwhile.
  if (!observer && themeSignature() !== signature) onMutation();
  return epoch;
}

export function useThemeEpoch(): number {
  return useSyncExternalStore(subscribe, snapshot, () => 0);
}
