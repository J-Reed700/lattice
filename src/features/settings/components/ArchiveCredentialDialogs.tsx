import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

import { GHOST_BUTTON_CLASS, PRIMARY_BUTTON_CLASS } from './settingsStyles';

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
          <label htmlFor="archive-restore-secret" className="block text-xs text-text-secondary">
            Passphrase or recovery code
          </label>
          <textarea
            id="archive-restore-secret"
            rows={3}
            autoComplete="off"
            spellCheck={false}
            value={secret}
            onChange={(event) => onSecretChange(event.target.value)}
            className="mt-1 w-full rounded-sm border border-border-default bg-bg px-2.5 py-2 font-mono text-xs leading-relaxed text-text-primary outline-hidden transition-colors duration-fast focus:border-accent"
          />
          <p className="mt-1 text-xs text-text-muted">
            Either works. Paste the 24 recovery words with spaces between them.
          </p>
        </div>

        {error ? (
          <p role="alert" className="text-sm text-danger">
            {error}
          </p>
        ) : null}

        <DialogFooter>
          <button
            type="button"
            onClick={onDismiss}
            className={GHOST_BUTTON_CLASS}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => onUnlock(secret.trim())}
            disabled={secret.trim().length === 0 || isRestoring}
            className={PRIMARY_BUTTON_CLASS}
          >
            {isRestoring ? 'Unlocking…' : 'Unlock'}
          </button>
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

        <div className="space-y-2">
          <div>
            <label
              htmlFor="archive-new-passphrase"
              className="block text-xs text-text-secondary"
            >
              Passphrase
            </label>
            <input
              id="archive-new-passphrase"
              type="password"
              autoComplete="new-password"
              value={newPassphrase}
              onChange={(event) => onNewPassphraseChange(event.target.value)}
              className="mt-1 h-8 w-full rounded-sm border border-border-default bg-bg px-2.5 text-sm text-text-primary outline-hidden transition-colors duration-fast focus:border-accent"
            />
          </div>
          <div>
            <label
              htmlFor="archive-new-passphrase-again"
              className="block text-xs text-text-secondary"
            >
              Passphrase again
            </label>
            <input
              id="archive-new-passphrase-again"
              type="password"
              autoComplete="new-password"
              value={newPassphraseAgain}
              onChange={(event) => onNewPassphraseAgainChange(event.target.value)}
              className="mt-1 h-8 w-full rounded-sm border border-border-default bg-bg px-2.5 text-sm text-text-primary outline-hidden transition-colors duration-fast focus:border-accent"
            />
          </div>
          <p className="text-xs text-text-muted">At least {MIN_PASSPHRASE_LENGTH} characters.</p>
        </div>

        {error ? (
          <p role="alert" className="text-sm text-danger">
            {error}
          </p>
        ) : null}

        <DialogFooter>
          {hasPassphrase ? (
            <button
              type="button"
              onClick={onRemove}
              disabled={isSaving}
              className={GHOST_BUTTON_CLASS}
            >
              Remove passphrase
            </button>
          ) : null}
          <button
            type="button"
            onClick={onClose}
            className={GHOST_BUTTON_CLASS}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onSubmit}
            disabled={isSaving}
            className={PRIMARY_BUTTON_CLASS}
          >
            {isSaving ? 'Saving…' : 'Save'}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
