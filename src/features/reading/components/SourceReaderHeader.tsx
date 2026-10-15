import { X, Maximize2, Minimize2, PanelRightClose } from 'lucide-react';

import { DialogClose, DialogTitle } from '@/components/ui/dialog';

const TITLE_CLASS =
  'wrap-break-word text-xl font-semibold font-serif leading-tight text-[hsl(var(--text-primary))]';
const CLOSE_CLASS =
  'rounded-sm p-2 text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-surface hover:text-[hsl(var(--text-primary))]';

interface SourceReaderHeaderProps {
  /** The docked pane has no dialog around it, so its title and close are plain elements. */
  isDocked: boolean;
  title: string;
  meta: string[];
  isFocused: boolean;
  onToggleFocus?: () => void;
  onMinimize?: () => void;
  onClose: () => void;
}

/** The reader's title row: the file name, its details, and the size and close controls. */
export const SourceReaderHeader = ({
  isDocked,
  title,
  meta,
  isFocused,
  onToggleFocus,
  onMinimize,
  onClose,
}: SourceReaderHeaderProps) => (
  <div className="flex shrink-0 items-center justify-between gap-3 border-b border-subtle px-6 py-5">
    <div className="min-w-0 flex-1">
      <p className="mb-1 text-xs text-text-muted">Source reader</p>
      {isDocked ? (
        <h2 className={TITLE_CLASS}>{title}</h2>
      ) : (
        <DialogTitle asChild><h2 className={TITLE_CLASS}>{title}</h2></DialogTitle>
      )}
      {meta.length > 0 && (
        <p className="mt-2 text-sm text-[hsl(var(--text-muted))]">
          {meta.join(' · ')}
        </p>
      )}
    </div>

    {onToggleFocus && (
      <button type="button" className="rounded-md p-2 text-text-secondary hover:bg-surface-raised"
        aria-label={isFocused ? 'Return to reading pane' : 'Expand reader'}
        title={isFocused ? 'Return to reading pane' : 'Expand reader'}
        onClick={onToggleFocus}>
        {isFocused ? <Minimize2 size={18} /> : <Maximize2 size={18} />}
      </button>
    )}
    {onMinimize && (
      <button type="button" className="rounded-md p-2 text-text-secondary hover:bg-surface-raised"
        aria-label="Minimize reader"
        title="Minimize reader"
        onClick={onMinimize}>
        <PanelRightClose size={18} />
      </button>
    )}
    {isDocked ? (
      <button
        type="button"
        onClick={onClose}
        className={CLOSE_CLASS}
        aria-label="Close"
        title="Close · Esc"
      >
        <X size={20} />
      </button>
    ) : (
      <DialogClose className={CLOSE_CLASS} aria-label="Close" title="Close · Esc">
        <X size={20} />
      </DialogClose>
    )}
  </div>
);
