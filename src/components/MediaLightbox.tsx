import type { ReactNode } from 'react';

import { X } from 'lucide-react';

import { Dialog, DialogClose, DialogContent, DialogTitle } from '@/components/ui/dialog';

export interface MediaLightboxProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Names the dialog and heads it ("Diagram", a picture's title). */
  title: string;
  /** The enlarged figure: a drawn diagram, a picture. Scaled to the window. */
  children: ReactNode;
  caption?: ReactNode;
  /** Links or buttons beside the title (`Open source page`). */
  actions?: ReactNode;
}

/**
 * One enlarged figure over the page: diagrams from chat answers, and pictures.
 *
 * The app's dialog underneath traps focus, closes on Escape or a click on the
 * backdrop, and returns focus to whatever opened it.
 */
export function MediaLightbox({ open, onOpenChange, title, children, caption, actions }: MediaLightboxProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        unstyled
        hideClose
        aria-describedby={undefined}
        className="media-lightbox surface-pop fixed left-1/2 top-1/2 z-50 flex max-h-[90vh] w-[90vw] flex-col gap-3 rounded-xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-overlay))] p-4 shadow-lg outline-hidden [translate:-50%_-50%]"
      >
        <div className="flex items-center gap-3">
          <DialogTitle className="min-w-0 flex-1 truncate text-sm font-medium text-[hsl(var(--text-secondary))]">
            {title}
          </DialogTitle>
          {actions && <div className="flex items-center gap-2 text-sm">{actions}</div>}
          <DialogClose className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-[hsl(var(--text-primary))] focus:outline-hidden focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))]">
            <X className="h-4 w-4" strokeWidth={1.75} />
            <span className="sr-only">Close</span>
          </DialogClose>
        </div>
        <div className="media-lightbox-body grid min-h-0 flex-1 place-items-center overflow-auto">
          {children}
        </div>
        {caption && <div className="text-sm leading-relaxed text-[hsl(var(--text-secondary))]">{caption}</div>}
      </DialogContent>
    </Dialog>
  );
}
