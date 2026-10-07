import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';

import { Copy, GitBranch } from 'lucide-react';
import { createPortal } from 'react-dom';

import { toast } from '@/stores/toastStore';

export interface TangentSelectionData {
  conversationId: string;
  messageId: string;
  selectedText: string;
}

export const TangentSelectionContext = createContext<{
  create: (selection: TangentSelectionData) => void;
  creating: boolean;
} | null>(null);

export const MAX_TANGENT_PASSAGE_LENGTH = 8000;

type PassageAction = { text: string; x: number; y: number; mode: 'selection' | 'menu' };

/** Shared by every assistant transcript, including the tangent itself. */
export function TangentSelection({ conversationId, messageId, enabled, children }: {
  conversationId?: string;
  messageId?: string;
  enabled: boolean;
  children: ReactNode;
}) {
  const tangents = useContext(TangentSelectionContext);
  const bodyRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [selection, setSelection] = useState<PassageAction | null>(null);
  const pointerSelecting = useRef(false);
  const available = Boolean(tangents && enabled && conversationId && messageId);

  const readSelection = useCallback(() => {
    const range = window.getSelection();
    if (!range || range.isCollapsed || !range.rangeCount) return null;
    const selectedRange = range.getRangeAt(0);
    if (!bodyRef.current?.contains(selectedRange.startContainer) || !bodyRef.current.contains(selectedRange.endContainer)) return null;
    const text = range.toString().trim();
    return text ? { text, rect: selectedRange.getBoundingClientRect() } : null;
  }, []);

  // Selection gestures reveal the action without moving focus or stealing the
  // highlight. Wait until pointer release so the button never follows a drag.
  useEffect(() => {
    if (!available) return;
    const reveal = () => {
      if (pointerSelecting.current || menuRef.current?.contains(document.activeElement)) return;
      const passage = readSelection();
      setSelection(current => {
        if (current?.mode === 'menu') return current;
        if (!passage) return null;
        const x = passage.rect.left;
        const y = passage.rect.bottom + 8;
        return current?.text === passage.text && current.x === x && current.y === y
          ? current : { text: passage.text, x, y, mode: 'selection' };
      });
    };
    const start = (event: PointerEvent) => {
      if (event.button !== 0 || menuRef.current?.contains(event.target as Node)) return;
      pointerSelecting.current = true;
      setSelection(null);
    };
    const finish = () => { pointerSelecting.current = false; reveal(); };
    const cancel = () => { pointerSelecting.current = false; setSelection(null); };
    document.addEventListener('selectionchange', reveal);
    document.addEventListener('pointerdown', start);
    document.addEventListener('pointerup', finish);
    document.addEventListener('pointercancel', cancel);
    return () => {
      document.removeEventListener('selectionchange', reveal);
      document.removeEventListener('pointerdown', start);
      document.removeEventListener('pointerup', finish);
      document.removeEventListener('pointercancel', cancel);
    };
  }, [available, readSelection]);

  useEffect(() => {
    if (!selection) return;
    const previousFocus = document.activeElement;
    if (selection.mode === 'menu') menuRef.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    const dismiss = (event: Event) => {
      if (!menuRef.current?.contains(event.target as Node)) setSelection(null);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      if (menuRef.current?.contains(document.activeElement)) bodyRef.current?.closest('article')?.focus();
      setSelection(null);
    };
    document.addEventListener('pointerdown', dismiss);
    document.addEventListener('keydown', escape, true);
    window.addEventListener('resize', dismiss);
    document.addEventListener('scroll', dismiss, true);
    return () => {
      document.removeEventListener('pointerdown', dismiss);
      document.removeEventListener('keydown', escape, true);
      window.removeEventListener('resize', dismiss);
      document.removeEventListener('scroll', dismiss, true);
      if (previousFocus instanceof HTMLElement && document.activeElement === document.body) previousFocus.focus();
    };
  }, [selection]);

  if (!tangents || !available || !conversationId || !messageId) return <>{children}</>;

  return (
    <div ref={bodyRef} onContextMenu={(event) => {
      const passage = readSelection();
      if (!passage) return;
      event.preventDefault();
      event.stopPropagation();
      setSelection({ text: passage.text, x: event.clientX || passage.rect.left, y: event.clientY || passage.rect.bottom, mode: 'menu' });
    }}>
      {children}
      {selection && createPortal(
        <div ref={menuRef} role={selection.mode === 'menu' ? 'menu' : 'toolbar'} aria-label="Selected passage" className="fixed z-100 w-56 rounded-lg border border-border-subtle bg-surface p-1 shadow-xl"
          style={{ left: Math.max(8, Math.min(selection.x, window.innerWidth - 232)), top: Math.max(8, Math.min(selection.y, window.innerHeight - (selection.mode === 'menu' ? 96 : 52))) }}
          onPointerDown={event => event.preventDefault()}
          onKeyDown={(event) => {
            if (selection.mode !== 'menu') return;
            if (event.key === 'Escape' || event.key === 'Tab') { event.preventDefault(); setSelection(null); bodyRef.current?.closest('article')?.focus(); }
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault();
              const buttons = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? []);
              const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
              buttons[(index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length]?.focus();
            }
          }}>
          <button type="button" role={selection.mode === 'menu' ? 'menuitem' : undefined} disabled={tangents.creating} className="tangent-menu-item" onClick={() => {
            if (Array.from(selection.text).length > MAX_TANGENT_PASSAGE_LENGTH) {
              toast.error('Choose a shorter passage', { message: 'A tangent can start from up to 8,000 characters.' });
            } else {
              tangents.create({ conversationId, messageId, selectedText: selection.text });
              window.getSelection()?.removeAllRanges();
            }
            setSelection(null);
          }}><GitBranch aria-hidden="true" className="h-4 w-4" />Ask in a tangent</button>
          {selection.mode === 'menu' && <button type="button" role="menuitem" className="tangent-menu-item" onClick={() => {
            void navigator.clipboard.writeText(selection.text).catch(() => toast.error("Couldn't copy the passage"));
            setSelection(null);
          }}><Copy aria-hidden="true" className="h-4 w-4" />Copy</button>}
        </div>, document.body,
      )}
    </div>
  );
}
