import { useEffect, useMemo, useRef, useState } from 'react';

import { clipboardUrl } from './clipboard';
import { VaultAPI } from '../../lib/api';
import { toast } from '../../stores/toastStore';
import { Button } from '../ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';


interface QuickCaptureDialogProps {
  open: boolean;
  /** Clipboard text read at the moment the sheet was opened, if any. */
  clipboard: string | null;
  onClose: () => void;
  /** Opens the page a capture landed on. Omitted outside the router. */
  onOpenPage?: (noteId: string) => void;
}

/**
 * The ⌘⇧N capture sheet. One textarea, Enter saves, Escape cancels.
 *
 * When the clipboard holds a URL and nothing has been typed, the sheet offers
 * to import it instead — the same gesture covers "keep this thought" and "keep
 * this page".
 */
export function QuickCaptureDialog({
  open,
  clipboard,
  onClose,
  onOpenPage,
}: QuickCaptureDialogProps) {
  const [text, setText] = useState('');
  const [isBusy, setIsBusy] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const importUrl = useMemo(() => clipboardUrl(clipboard), [clipboard]);

  useEffect(() => {
    if (open) {
      setText('');
      setIsBusy(false);
    }
  }, [open]);

  const handleSave = async () => {
    const content = text.trim();
    if (!content || isBusy) return;
    setIsBusy(true);
    const result = await VaultAPI.quickCapture(content);
    setIsBusy(false);
    if (result.ok) {
      const { noteId, noteTitle } = result.data;
      // The toast names the page and, where we can navigate, opens it.
      toast.success(
        `Saved to ${noteTitle}`,
        onOpenPage ? { action: { label: 'Open', onClick: () => onOpenPage(noteId) } } : undefined,
      );
      setText('');
      onClose();
      return;
    }
    toast.error("Couldn't save that", { message: result.error });
  };

  const handleImportUrl = async () => {
    if (!importUrl || isBusy) return;
    setIsBusy(true);
    const result = await VaultAPI.ingestWebUrl(importUrl.href);
    setIsBusy(false);
    if (result.ok) {
      toast.success(`Importing ${importUrl.host}`);
      onClose();
      return;
    }
    toast.error(`Couldn't import ${importUrl.host}`, { message: result.error });
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      void handleSave();
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
    >
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Quick capture</DialogTitle>
        </DialogHeader>

        <textarea
          ref={textareaRef}
          value={text}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={handleKeyDown}
          rows={5}
          autoFocus
          placeholder="What do you want to keep?"
          className="w-full resize-none rounded-sm border border-border-default bg-bg px-3 py-2 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-2 focus:ring-ring"
        />

        {importUrl && text.trim() === '' ? (
          <button
            type="button"
            onClick={() => void handleImportUrl()}
            disabled={isBusy}
            className="self-start text-sm text-accent transition-colors duration-fast hover:underline disabled:opacity-50"
          >
            Import {importUrl.host}
          </button>
        ) : null}

        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="button" onClick={() => void handleSave()} disabled={text.trim() === '' || isBusy}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
