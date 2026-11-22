import { useState } from 'react';

import { Check } from 'lucide-react';

import { ConfirmDialog } from '@/components/ConfirmDialog';
import { toast } from '@/stores/toastStore';

const REMOVE_BUTTON_CLASS =
  'text-xs text-text-secondary underline transition-colors duration-fast hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-50';

interface InstalledDownloadProps {
  /** How it is named in the button label, the dialog and the toasts, e.g. "the reranker" or "Kokoro 82M". */
  name: string;
  /** One sentence on what stops working once it is gone. */
  consequence: string;
  /** What removing it frees, formatted as the row shows its download size, e.g. "639 MB". */
  size?: string;
  /** Deletes it. A rejection's message is shown to the user. */
  onRemove: () => Promise<unknown>;
}

/**
 * The settled end of a download row: it is here, and can be removed after a
 * confirmation. Every on-demand download uses it so they all read the same.
 */
export function InstalledDownload({ name, consequence, size, onRemove }: InstalledDownloadProps) {
  const [confirming, setConfirming] = useState(false);
  const [removing, setRemoving] = useState(false);

  const remove = async () => {
    setConfirming(false);
    setRemoving(true);
    try {
      await onRemove();
      toast.success(`Removed ${name}`);
    } catch (error) {
      toast.error(`Couldn't remove ${name}`, { message: error instanceof Error ? error.message : String(error) });
    } finally {
      setRemoving(false);
    }
  };

  return (
    <>
      <span className="inline-flex items-center gap-3">
        <span className="inline-flex items-center gap-1.5 text-sm text-text-secondary">
          <Check className="h-4 w-4 text-accent" aria-hidden="true" />
          Downloaded
        </span>
        <button
          type="button"
          className={REMOVE_BUTTON_CLASS}
          aria-label={`Remove ${name}`}
          disabled={removing}
          onClick={() => setConfirming(true)}
        >
          {removing ? 'Removing…' : 'Remove'}
        </button>
      </span>
      <ConfirmDialog
        isOpen={confirming}
        title={`Remove ${name}?`}
        message={`${consequence} ${size ? `This frees ${size}. ` : ''}You can download it again here.`}
        confirmLabel="Remove"
        variant="danger"
        onConfirm={() => void remove()}
        onCancel={() => setConfirming(false)}
      />
    </>
  );
}
