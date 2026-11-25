import { type ReactNode, useEffect, useReducer, useState } from 'react';

import { CheckSquare, List, Quote } from 'lucide-react';

import { cn } from '@/lib/utils';

import type { Editor } from '@tiptap/react';

type DomWithEditor = HTMLElement & { editor?: Editor };

function resolveEditorFromContainer(container: HTMLElement | null): Editor | null {
  if (!container) return null;
  const dom = container.querySelector<DomWithEditor>('.tiptap-editor, .ProseMirror');
  const instance = dom?.editor ?? null;
  return instance && !instance.isDestroyed ? instance : null;
}

/**
 * Resolves the live TipTap `Editor` instance mounted inside `container`
 * (passed via a callback-ref state value so delayed mounts after `isLoadingNote`
 * or `loadError` transitions always trigger synchronization) without altering
 * the shared `TiptapEditor` contract.
 */
export function useDomTiptapEditor(
  container: HTMLElement | null,
  resetKey?: string | null,
): Editor | null {
  const [editor, setEditor] = useState<Editor | null>(null);

  useEffect(() => {
    if (!container) {
      setEditor(null);
      return;
    }

    let resolved = false;
    const sync = () => {
      const next = resolveEditorFromContainer(container);
      resolved = Boolean(next);
      setEditor((prev) => (prev === next ? prev : next));
    };

    sync();

    const rafId = window.requestAnimationFrame(() => {
      if (!resolved) sync();
    });
    const timeoutId = window.setTimeout(() => {
      if (!resolved) sync();
    }, 0);

    const observer = new MutationObserver(sync);
    observer.observe(container, { childList: true, subtree: true, attributes: true });
    return () => {
      window.cancelAnimationFrame(rafId);
      window.clearTimeout(timeoutId);
      observer.disconnect();
    };
  }, [container, resetKey]);

  return editor;
}

interface PillButtonProps {
  label: string;
  isActive: boolean;
  disabled: boolean;
  onRun: () => void;
  className?: string;
  children: ReactNode;
}

function PillButton({ label, isActive, disabled, onRun, className, children }: PillButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={isActive}
      title={label}
      disabled={disabled}
      onMouseDown={(event) => {
        event.preventDefault();
      }}
      onClick={onRun}
      className={cn(
        'inline-flex h-6 items-center justify-center gap-1 rounded-md px-1.5 text-[11.5px] leading-none transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-45',
        isActive
          ? 'bg-accent-muted font-medium text-accent'
          : 'text-text-secondary hover:bg-surface-sunken hover:text-text-primary',
        className,
      )}
    >
      {children}
    </button>
  );
}

interface JournalFormattingPillProps {
  editor: Editor | null;
}

/**
 * Compact inline formatting pill bar in the Journal top header bar,
 * wired directly to TipTap's `editor` instance.
 */
export function JournalFormattingPill({ editor }: JournalFormattingPillProps) {
  const [, forceUpdate] = useReducer((tick: number) => tick + 1, 0);

  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    const onChange = () => forceUpdate();
    editor.on('transaction', onChange);
    editor.on('selectionUpdate', onChange);
    editor.on('update', onChange);
    return () => {
      editor.off('transaction', onChange);
      editor.off('selectionUpdate', onChange);
      editor.off('update', onChange);
    };
  }, [editor]);

  const live = editor && !editor.isDestroyed ? editor : null;
  const disabled = !live;

  const isH1 = Boolean(live?.isActive('heading', { level: 1 }));
  const isH2 = Boolean(live?.isActive('heading', { level: 2 }));
  const isBold = Boolean(live?.isActive('bold'));
  const isItalic = Boolean(live?.isActive('italic'));
  const isCode = Boolean(live?.isActive('code'));
  const isBulletList = Boolean(live?.isActive('bulletList'));
  const isTaskList = Boolean(live?.isActive('taskList'));
  const isBlockquote = Boolean(live?.isActive('blockquote'));

  return (
    <div
      role="toolbar"
      aria-label="Text formatting"
      className="inline-flex items-center gap-0.5 rounded-lg border border-border-subtle bg-surface px-1.5 py-0.5 shadow-sm dark:bg-surface-overlay"
    >
      <PillButton
        label="Heading 1"
        isActive={isH1}
        disabled={disabled}
        onRun={() => {
          live?.chain().focus().toggleHeading({ level: 1 }).run();
        }}
        className="font-semibold tracking-tight"
      >
        H1
      </PillButton>
      <PillButton
        label="Heading 2"
        isActive={isH2}
        disabled={disabled}
        onRun={() => {
          live?.chain().focus().toggleHeading({ level: 2 }).run();
        }}
        className="font-medium tracking-tight"
      >
        H2
      </PillButton>

      <span aria-hidden="true" className="mx-0.5 h-3.5 w-px bg-border-subtle" />

      <PillButton
        label="Bold"
        isActive={isBold}
        disabled={disabled}
        onRun={() => {
          live?.chain().focus().toggleBold().run();
        }}
        className="min-w-6 font-bold"
      >
        B
      </PillButton>
      <PillButton
        label="Italic"
        isActive={isItalic}
        disabled={disabled}
        onRun={() => {
          live?.chain().focus().toggleItalic().run();
        }}
        className="min-w-6 font-serif text-[12px] italic"
      >
        I
      </PillButton>
      <PillButton
        label="Inline code"
        isActive={isCode}
        disabled={disabled}
        onRun={() => {
          live?.chain().focus().toggleCode().run();
        }}
        className="font-mono text-[10.5px] tracking-tighter"
      >
        {'</>'}
      </PillButton>

      <span aria-hidden="true" className="mx-0.5 h-3.5 w-px bg-border-subtle" />

      <PillButton
        label="Bullet list"
        isActive={isBulletList}
        disabled={disabled}
        onRun={() => {
          live?.chain().focus().toggleBulletList().run();
        }}
      >
        <List className="h-3 w-3 shrink-0" strokeWidth={1.85} aria-hidden="true" />
        <span className="hidden md:inline">List</span>
      </PillButton>
      <PillButton
        label="Task list"
        isActive={isTaskList}
        disabled={disabled}
        onRun={() => {
          live?.chain().focus().toggleTaskList().run();
        }}
      >
        <CheckSquare className="h-3 w-3 shrink-0" strokeWidth={1.75} aria-hidden="true" />
        <span className="hidden md:inline">Task</span>
      </PillButton>
      <PillButton
        label="Blockquote"
        isActive={isBlockquote}
        disabled={disabled}
        onRun={() => {
          live?.chain().focus().toggleBlockquote().run();
        }}
      >
        <Quote className="h-3 w-3 shrink-0" strokeWidth={1.75} aria-hidden="true" />
        <span className="hidden md:inline">Quote</span>
      </PillButton>
    </div>
  );
}
