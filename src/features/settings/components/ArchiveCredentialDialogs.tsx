import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Textarea,
} from '@/components/ui';

export const MIN_PASSPHRASE_LENGTH = 8;

interface RestoreSecretDialogProps {
  open: boolean;
  secret: string;
  error: string | null;
  isRestoring: boolean;
  onSecretChange: (_secret: string) => void;
  onDismiss: () => void;
  onUnlock: (_secret: string) => void;
}

/** Asks for the passphrase or recovery code a backup from another computer was encrypted with. */
export function RestoreSecretDialog({
  open,
  secret,
  error,
  isRestoring,
  onSecretChange,
  onDismiss,
  onUnlock,
}: RestoreSecretDialogProps) {
  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onDismiss();
        }
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Unlock this backup</DialogTitle>
          <DialogDescription>
            This backup was made on another computer, so Lattice needs the secret it was
            encrypted with.
          </DialogDescription>
        </DialogHeader>

        <div>
          <Textarea
            id="archive-restore-secret"
            label="Passphrase or recovery code"
            rows={3}
            autoComplete="off"
            spellCheck={false}
            value={secret}
            onChange={(event) => onSecretChange(event.target.value)}
            className="font-mono text-xs"
            helperText="Either works. Paste the 24 recovery words with spaces between them."
          />
        </div>

        {error ? (
          <p role="alert" className="text-sm text-danger-fg">
            {error}
          </p>
        ) : null}

        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onDismiss}>
            Cancel
          </Button>
          <Button
            type="button"
            onClick={() => onUnlock(secret.trim())}
            disabled={secret.trim().length === 0 || isRestoring}
          >
            {isRestoring ? 'Unlocking…' : 'Unlock'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface PassphraseDialogProps {
  open: boolean;
  hasPassphrase: boolean | undefined;
  newPassphrase: string;
  newPassphraseAgain: string;
  error: string | null;
  isSaving: boolean;
  onNewPassphraseChange: (_passphrase: string) => void;
  onNewPassphraseAgainChange: (_passphrase: string) => void;
  onClose: () => void;
  onRemove: () => void;
  onSubmit: () => void;
}

/** Sets, changes or removes the passphrase that unlocks off-device archives. */
export function PassphraseDialog({
  open,
  hasPassphrase,
  newPassphrase,
  newPassphraseAgain,
  error,
  isSaving,
  onNewPassphraseChange,
  onNewPassphraseAgainChange,
  onClose,
  onRemove,
  onSubmit,
}: PassphraseDialogProps) {
  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {hasPassphrase ? 'Change passphrase' : 'Set a passphrase'}
          </DialogTitle>
          <DialogDescription>
            A passphrase is a shorter thing to remember than the 24-word recovery code. Lattice
            cannot reset it — the recovery code is what gets you back in.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <Input
            id="archive-new-passphrase"
            label="Passphrase"
            type="password"
            autoComplete="new-password"
            value={newPassphrase}
            onChange={(event) => onNewPassphraseChange(event.target.value)}
          />
          <Input
            id="archive-new-passphrase-again"
            label="Passphrase again"
            type="password"
            autoComplete="new-password"
            value={newPassphraseAgain}
            onChange={(event) => onNewPassphraseAgainChange(event.target.value)}
            helperText={`At least ${MIN_PASSPHRASE_LENGTH} characters.`}
          />
        </div>

        {error ? (
          <p role="alert" className="text-sm text-danger-fg">
            {error}
          </p>
        ) : null}

        <DialogFooter>
          {hasPassphrase ? (
            <Button type="button" variant="ghost" onClick={onRemove} disabled={isSaving}>
              Remove passphrase
            </Button>
          ) : null}
          <Button type="button" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="button" onClick={onSubmit} disabled={isSaving}>
            {isSaving ? 'Saving…' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
