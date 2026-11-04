import { createContext, useCallback, useContext, useEffect, useId, useRef, useState } from 'react';
import type { ReactNode } from 'react';

import { Copy, GitBranch } from 'lucide-react';
import { createPortal } from 'react-dom';

import { selectedTextWithFormulas } from '@/components/TiptapEditor';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { LearningTangentDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';

import '@/features/chat/components/tangents/tangents.css';

export const MAX_STUDY_TANGENT_PASSAGE_LENGTH = 8000;

export interface StartStudyTangentParams {
  moduleId: string;
  lessonId: string;
  sectionIndex?: number | null;
  sectionTitle?: string | null;
  selectedText: string;
}

export interface StudyTangentsContextValue {
  programId: string;
  tangents: LearningTangentDto[];
  activeTangentId: string | null;
  isCreating: boolean;
  isPanelOpen?: boolean;
  startTangent: (params: StartStudyTangentParams) => Promise<void>;
  openTangent: (tangentId: string) => void;
  openTangentsDrawer: (filterLessonId?: string | null) => void;
  closeTangentsDrawer?: () => void;
}

export const StudyTangentsContext = createContext<StudyTangentsContextValue | null>(null);

export function useStudyTangentsContext(): StudyTangentsContextValue {
  const context = useContext(StudyTangentsContext);
  if (!context) {
    throw new Error('useStudyTangentsContext must be used within a StudyTangentsContext.Provider');
  }
  return context;
}

type PassageAction = { text: string; x: number; y: number; mode: 'selection' | 'menu' };

export interface StudyTangentSelectionProps {
  moduleId: string;
  lessonId: string;
  sectionIndex?: number | null;
  sectionTitle?: string | null;
  children: ReactNode;
}

export function StudyTangentSelection({
  moduleId,
  lessonId,
  sectionIndex = null,
  sectionTitle = null,
  children,
}: StudyTangentSelectionProps) {
  const tangentsContext = useContext(StudyTangentsContext);
  const bodyRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [selection, setSelection] = useState<PassageAction | null>(null);
  const pointerSelecting = useRef(false);

  const readSelection = useCallback(() => {
    const range = window.getSelection();
    if (!range || range.isCollapsed || !range.rangeCount) return null;
    const selectedRange = range.getRangeAt(0);
    if (
      !bodyRef.current?.contains(selectedRange.startContainer) ||
      !bodyRef.current.contains(selectedRange.endContainer)
    ) {
      return null;
    }
    // A formula reads as its source, not as KaTeX's drawing of it.
    const text = (selectedTextWithFormulas(selectedRange) ?? range.toString()).trim();
    return text ? { text, rect: selectedRange.getBoundingClientRect() } : null;
  }, []);

  useEffect(() => {
    if (!tangentsContext) return;
    const reveal = () => {
      if (pointerSelecting.current || menuRef.current?.contains(document.activeElement)) return;
      const passage = readSelection();
      setSelection((current) => {
        if (current?.mode === 'menu') return current;
        if (!passage) return null;
        const x = passage.rect.left;
        const y = passage.rect.bottom + 8;
        return current?.text === passage.text && current.x === x && current.y === y
          ? current
          : { text: passage.text, x, y, mode: 'selection' };
      });
    };
    const start = (event: PointerEvent) => {
      if (event.button !== 0 || menuRef.current?.contains(event.target as Node)) return;
      pointerSelecting.current = true;
      setSelection(null);
    };
    const finish = () => {
      pointerSelecting.current = false;
      reveal();
    };
    const cancel = () => {
      pointerSelecting.current = false;
      setSelection(null);
    };

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
  }, [readSelection, tangentsContext]);

  useEffect(() => {
    if (!selection) return;
    const previousFocus = document.activeElement;
    if (selection.mode === 'menu') {
      menuRef.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    }
    const dismiss = (event: Event) => {
      if (!(event.target instanceof Node) || !menuRef.current?.contains(event.target)) {
        setSelection(null);
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      if (menuRef.current?.contains(document.activeElement)) {
        bodyRef.current?.closest('article')?.focus();
      }
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
      if (previousFocus instanceof HTMLElement && document.activeElement === document.body) {
        previousFocus.focus();
      }
    };
  }, [selection]);

  if (!tangentsContext) return <>{children}</>;

  const isTooLong = Boolean(selection && Array.from(selection.text).length > MAX_STUDY_TANGENT_PASSAGE_LENGTH);

  return (
    <div
      ref={bodyRef}
      className="tangent-selectable"
      onContextMenu={(event) => {
        const passage = readSelection();
        if (!passage) return;
        event.preventDefault();
        event.stopPropagation();
        setSelection({
          text: passage.text,
          x: event.clientX || passage.rect.left,
          y: event.clientY || passage.rect.bottom,
          mode: 'menu',
        });
      }}
    >
      {children}
      {selection &&
        createPortal(
          <div
            ref={menuRef}
            role={selection.mode === 'menu' ? 'menu' : 'toolbar'}
            aria-label="Selected passage"
            className="fixed z-100 min-w-56 max-w-xs rounded-lg border border-border-subtle bg-surface p-1 shadow-xl"
            style={{
              left: Math.max(8, Math.min(selection.x, window.innerWidth - 240)),
              top: Math.max(8, Math.min(selection.y, window.innerHeight - (selection.mode === 'menu' ? 110 : 70))),
            }}
            onPointerDown={(event) => event.preventDefault()}
            onKeyDown={(event) => {
              if (selection.mode !== 'menu') return;
              if (event.key === 'Escape' || event.key === 'Tab') {
                event.preventDefault();
                setSelection(null);
                bodyRef.current?.closest('article')?.focus();
              }
              if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                event.preventDefault();
                const buttons = Array.from(
                  menuRef.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [],
                );
                const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
                buttons[
                  (index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length
                ]?.focus();
              }
            }}
          >
            {isTooLong && (
              <p className="px-2 py-1 text-[11px] text-amber-600 dark:text-amber-400">
                Select a shorter passage (up to 8,000 characters)
              </p>
            )}
            <button
              type="button"
              role={selection.mode === 'menu' ? 'menuitem' : undefined}
              disabled={tangentsContext.isCreating || isTooLong}
              className="tangent-menu-item"
              onClick={() => {
                if (isTooLong) {
                  toast.error('Choose a shorter passage', {
                    message: 'A tangent can start from up to 8,000 characters.',
                  });
                  return;
                }
                void tangentsContext.startTangent({
                  moduleId,
                  lessonId,
                  sectionIndex,
                  sectionTitle,
                  selectedText: selection.text,
                });
                window.getSelection()?.removeAllRanges();
                setSelection(null);
              }}
            >
              <GitBranch aria-hidden="true" className="h-3.5 w-3.5" />
              Ask in a tangent
            </button>
            {selection.mode === 'menu' && (
              <button
                type="button"
                role="menuitem"
                className="tangent-menu-item"
                onClick={() => {
                  void navigator.clipboard.writeText(selection.text).catch(() => {
                    toast.error("Couldn't copy the passage");
                  });
                  setSelection(null);
                }}
              >
                <Copy aria-hidden="true" className="h-4 w-4" />
                Copy
              </button>
            )}
          </div>,
          document.body,
        )}
    </div>
  );
}

export interface SectionTangentBarProps {
  moduleId: string;
  lessonId: string;
  sectionIndex?: number | null;
  sectionTitle?: string | null;
  sectionText: string;
  label?: string;
}

export function SectionTangentBar({
  moduleId,
  lessonId,
  sectionIndex = null,
  sectionTitle = null,
  sectionText,
  label,
}: SectionTangentBarProps) {
  const tangentsContext = useContext(StudyTangentsContext);
  const [trimPassage, setTrimPassage] = useState<string | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const passageId = useId();

  if (!tangentsContext) return null;

  const sectionTangents = tangentsContext.tangents.filter(
    (item) => item.lessonId === lessonId && (item.sectionIndex ?? null) === (sectionIndex ?? null),
  );

  const startFromText = (text: string) => {
    void tangentsContext.startTangent({
      moduleId,
      lessonId,
      sectionIndex,
      sectionTitle,
      selectedText: text,
    });
  };

  const handleStartClick = () => {
    const trimmed = sectionText.trim();
    if (!trimmed) return;
    if (Array.from(trimmed).length <= MAX_STUDY_TANGENT_PASSAGE_LENGTH) {
      startFromText(trimmed);
    } else {
      setTrimPassage(trimmed.slice(0, 4000));
    }
  };

  const testId = sectionIndex != null ? `section-tangent-btn-${sectionIndex}` : 'lesson-tangent-btn';
  const displayLabel = label ?? (sectionIndex != null ? 'Start tangent' : 'Start a tangent on this lesson');
  const trimLength = Array.from(trimPassage?.trim() ?? '').length;

  return (
    <>
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-2.5 text-xs text-text-muted">
        <div className="flex flex-wrap items-center gap-1.5">
          {sectionTangents.length > 0 && (
            <button
              type="button"
              onClick={() => tangentsContext.openTangentsDrawer(lessonId)}
              className="inline-flex items-center gap-1 rounded-full bg-accent/10 px-2.5 py-1 text-xs font-medium text-accent hover:bg-accent/20 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
            >
              <GitBranch className="h-3 w-3 text-accent" aria-hidden="true" />
              {sectionTangents.length} {sectionTangents.length === 1 ? 'tangent' : 'tangents'}
            </button>
          )}
          {sectionTangents.slice(0, 2).map((t) => (
            <button
              key={t.conversationId}
              type="button"
              title={t.title}
              onClick={() => tangentsContext.openTangent(t.conversationId)}
              className="inline-flex max-w-[180px] items-center truncate rounded-full border border-border bg-surface px-2 py-0.5 text-[11px] text-text-secondary hover:border-accent hover:text-accent focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
            >
              {t.title}
            </button>
          ))}
        </div>

        <button
          ref={buttonRef}
          type="button"
          data-testid={testId}
          disabled={tangentsContext.isCreating}
          onClick={handleStartClick}
          className="inline-flex items-center gap-1.5 rounded-full border border-border px-3 py-1 font-medium text-text-secondary hover:border-accent hover:text-accent disabled:opacity-50 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
        >
          <GitBranch className="h-3.5 w-3.5 text-accent" aria-hidden="true" />
          {displayLabel}
        </button>
      </div>

      <Dialog open={trimPassage !== null} onOpenChange={(open) => { if (!open) setTrimPassage(null); }}>
        <DialogContent
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            if (document.activeElement === document.body) buttonRef.current?.focus();
          }}
        >
          <DialogTitle>Choose a starting passage</DialogTitle>
          <DialogDescription>
            This section is long. Keep the portion you want to explore, up to 8,000 characters.
          </DialogDescription>
          <label htmlFor={passageId} className="text-sm font-medium">
            Passage to explore
          </label>
          <textarea
            id={passageId}
            value={trimPassage ?? ''}
            rows={8}
            onChange={(event) => setTrimPassage(event.target.value)}
            aria-describedby={`${passageId}-length`}
            className="w-full resize-y rounded-lg border border-border bg-surface p-3 text-sm leading-relaxed outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          />
          <div className="flex items-center justify-between gap-3">
            <span id={`${passageId}-length`} className="text-xs text-text-muted">
              {trimLength.toLocaleString()} / 8,000 characters
            </span>
            <button
              type="button"
              disabled={!trimLength || trimLength > MAX_STUDY_TANGENT_PASSAGE_LENGTH || tangentsContext.isCreating}
              className="rounded-lg bg-accent px-3 py-2 text-sm text-accent-fg disabled:opacity-40 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              onClick={() => {
                if (trimPassage) {
                  startFromText(trimPassage.trim());
                  setTrimPassage(null);
                }
              }}
            >
              Start tangent
            </button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
