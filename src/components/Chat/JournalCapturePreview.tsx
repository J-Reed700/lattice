import { useRef, useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { Check, NotebookPen, X } from 'lucide-react';

import VaultAPI from '@/lib/api';

interface JournalCapturePreviewProps {
  content: string;
  onClose: () => void;
  onOpenNote: (_id: string) => void;
}

export function JournalCapturePreview({ content, onClose, onOpenNote }: JournalCapturePreviewProps) {
  const pendingRef = useRef(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<{ noteId: string; noteTitle: string } | null>(null);

  const save = async () => {
    if (pendingRef.current || saved) return;
    pendingRef.current = true;
    setIsSaving(true);
    setError(null);
    try {
      const result = await VaultAPI.quickCapture(content);
      if (result.ok) setSaved(result.data);
      else setError(result.error);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Please try again.');
    } finally {
      pendingRef.current = false;
      setIsSaving(false);
    }
  };

  return (
    <Dialog.Root open onOpenChange={(open) => { if (!open && !pendingRef.current) onClose(); }}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[70] bg-[hsl(var(--overlay))]" />
        <Dialog.Content className="capture-preview fixed left-1/2 top-1/2 z-[71] w-[calc(100vw-32px)] max-w-lg -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border-default bg-surface p-6 shadow-md">
          <div className="flex items-start justify-between gap-4">
            <div>
              <NotebookPen className="mb-4 h-6 w-6 text-accent" strokeWidth={1.5} />
              <Dialog.Title className="font-serif text-2xl text-text-primary">{saved ? 'A thought worth keeping.' : 'Keep this passage'}</Dialog.Title>
              <Dialog.Description className="mt-2 text-sm text-text-secondary">
                {saved ? `Saved to ${saved.noteTitle || 'your journal'}.` : 'Add the passage and its source attribution to your journal using your capture destination.'}
              </Dialog.Description>
            </div>
            <Dialog.Close aria-label="Close capture preview" disabled={isSaving} className="rounded-md p-2 text-text-secondary hover:bg-surface-raised disabled:opacity-40"><X size={18} /></Dialog.Close>
          </div>
          <blockquote className="my-6 max-h-[35vh] overflow-auto whitespace-pre-wrap border-l-2 border-accent pl-4 font-serif text-base leading-relaxed text-text-primary">{content.replace(/^> ?/gm, '')}</blockquote>
          {error && <p role="alert" className="mb-4 text-sm text-danger-fg">Couldn't save this passage. {error}</p>}
          <div className="flex items-center justify-end gap-3">
            {saved ? <>
              <span role="status" className="mr-auto flex items-center gap-2 text-sm text-success-fg"><Check size={16} />Saved</span>
              <button type="button" onClick={onClose} className="px-3 py-2 text-sm text-text-secondary">Keep reading</button>
              <button type="button" onClick={() => onOpenNote(saved.noteId)} className="rounded-md bg-[hsl(var(--action))] px-4 py-2 text-sm text-[hsl(var(--action-fg))] hover:bg-[hsl(var(--action-hover))]">Open entry</button>
            </> : <>
              <button type="button" disabled={isSaving} onClick={onClose} className="px-3 py-2 text-sm text-text-secondary disabled:opacity-40">Cancel</button>
              <button type="button" disabled={isSaving} onClick={() => void save()} className="rounded-md bg-[hsl(var(--action))] px-4 py-2 text-sm text-[hsl(var(--action-fg))] hover:bg-[hsl(var(--action-hover))] disabled:opacity-60">{isSaving ? 'Saving…' : 'Add to journal'}</button>
            </>}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
