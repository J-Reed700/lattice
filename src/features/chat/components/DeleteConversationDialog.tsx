import { useState } from 'react';

import { Loader2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';

/** Asks before a conversation goes, and stays open with the reason if it cannot. */
export function DeleteConversationDialog({ conversation, onDelete, onClose }: {
  conversation: { id: string; title: string };
  onDelete: (_id: string) => Promise<boolean>;
  onClose: () => void;
}) {
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const deleteConversation = async () => {
    if (deleting) return;
    setDeleting(true);
    setError(null);
    try {
      if (await onDelete(conversation.id)) onClose();
      else setError('The conversation could not be deleted. Try again.');
    } finally {
      setDeleting(false);
    }
  };

  return (
    <Dialog open onOpenChange={(next) => !next && !deleting && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Delete this conversation?</DialogTitle>
          <DialogDescription>
            &ldquo;{conversation.title}&rdquo; and all of its messages will be permanently deleted. This can&apos;t be undone.
          </DialogDescription>
        </DialogHeader>
        {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
        <DialogFooter>
          <Button type="button" variant="outline" disabled={deleting} onClick={onClose}>Cancel</Button>
          <Button type="button" variant="destructive" disabled={deleting} onClick={() => void deleteConversation()}>
            {deleting ? <><Loader2 className="h-4 w-4 animate-spin" />Deleting…</> : 'Delete conversation'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
