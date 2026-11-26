import { useEffect, useRef, useState, type ReactNode } from 'react';

import { Button } from '@/components/ui/button';

import { useEditMode } from './editMode';

const BLOCKING_ROLES = '[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"]';

export function EditModeBar(): ReactNode {
  const on = useEditMode((state) => state.on);
  const set = useEditMode((state) => state.set);
  const prevOnRef = useRef(on);
  const [announcement, setAnnouncement] = useState('');

  useEffect(() => {
    if (prevOnRef.current === on) return;
    prevOnRef.current = on;
    setAnnouncement(
      on ? 'Edit mode on. Each piece you can correct has an Edit button.' : 'Edit mode off.',
    );
  }, [on]);

  useEffect(() => {
    if (!on) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented) return;
      if (document.querySelector(BLOCKING_ROLES)) return;
      set(false);
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [on, set]);

  return (
    <>
      <p role="status" aria-live="polite" className="sr-only">
        {announcement}
      </p>
      {on && (
        <div
          role="region"
          aria-label="Edit mode"
          className="fixed inset-x-0 top-0 z-40 flex items-center justify-between gap-3 border-b border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-overlay))] px-4 py-2 text-ui text-[hsl(var(--text-primary))] shadow-sm"
        >
          <span>Edit mode: choose a piece to correct it.</span>
          <Button type="button" size="sm" variant="outline" onClick={() => set(false)}>
            Done
          </Button>
        </div>
      )}
    </>
  );
}
