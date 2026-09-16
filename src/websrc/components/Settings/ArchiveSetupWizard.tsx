/**
 * ArchiveSetupWizard
 *
 * Five steps to turn on off-device backup, or three of them to issue a
 * replacement recovery code.
 *
 * The recovery code is the only part of this the user has to keep. Lattice
 * holds no copy of it and cannot reset it, so step 2 shows the words, step 3
 * makes the user prove three of them were written down, and nothing is
 * persisted until step 3 succeeds — closing before then throws away a key that
 * only ever existed in the backend's memory.
 */

import { useEffect, useMemo, useState } from 'react';

import type { ArchiveStatus, WordConfirmation } from '@/types/api/backup';

import { GHOST_BUTTON_CLASS, PRIMARY_BUTTON_CLASS, SECONDARY_BUTTON_CLASS } from './settingsStyles';
import {
  useBeginArchiveSetupMutation,
  useChooseArchiveDestinationMutation,
  useConfirmArchiveSetupMutation,
  useRotateRecoveryCodeMutation,
  useSetArchivePassphraseMutation,
} from '../../hooks/queries/useArchiveQuery';
import { toast } from '../../stores/toastStore';
import { Badge } from '../ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';

/** Setup runs all five steps; rotate reissues the code and skips the passphrase step. */
export type ArchiveWizardMode = 'setup' | 'rotate';

type Step = 'intro' | 'words' | 'confirm' | 'passphrase' | 'destination';

const MIN_PASSPHRASE_LENGTH = 8;
const PRINT_CONTAINER_ID = 'lattice-recovery-print';

interface ArchiveSetupWizardProps {
  open: boolean;
  mode: ArchiveWizardMode;
  /** The status as the parent knows it, so the last step can show the folder already set. */
  status?: ArchiveStatus | null;
  onClose: () => void;
}

/**
 * Prints the words on their own, without the app around them. Everything else
 * on the page is hidden for the duration of the print, then put back.
 */
function printRecoveryWords(words: string[]): boolean {
  if (typeof window.print !== 'function') return false;

  const container = document.createElement('div');
  container.id = PRINT_CONTAINER_ID;

  const heading = document.createElement('h1');
  heading.textContent = 'Lattice recovery code';

  const note = document.createElement('p');
  note.textContent =
    'These 24 words unlock your encrypted Lattice backups. Keep this page somewhere safe: anyone who has it can read your backups, and Lattice cannot issue a replacement for a code it never stored.';

  const list = document.createElement('ol');
  for (const word of words) {
    const item = document.createElement('li');
    item.textContent = word;
    list.appendChild(item);
  }

  container.append(heading, note, list);

  const style = document.createElement('style');
  style.textContent = `
    #${PRINT_CONTAINER_ID} { display: none; }
    @media print {
      body > *:not(#${PRINT_CONTAINER_ID}) { display: none !important; }
      #${PRINT_CONTAINER_ID} { display: block; padding: 24px; color: #000; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
      #${PRINT_CONTAINER_ID} h1 { margin: 0 0 8px; font-size: 18px; }
      #${PRINT_CONTAINER_ID} p { margin: 0 0 16px; max-width: 32em; font-size: 12px; line-height: 1.5; }
      #${PRINT_CONTAINER_ID} ol { columns: 2; font-size: 14px; line-height: 2; }
    }
  `;

  document.body.append(style, container);
  try {
    window.print();
  } finally {
    style.remove();
    container.remove();
  }
  return true;
}

export function ArchiveSetupWizard({ open, mode, status, onClose }: ArchiveSetupWizardProps) {
  const isRotate = mode === 'rotate';

  const [step, setStep] = useState<Step>('intro');
  const [words, setWords] = useState<string[]>([]);
  const [confirmIndices, setConfirmIndices] = useState<number[]>([]);
  const [typedWords, setTypedWords] = useState<Record<number, string>>({});
  const [savedAcknowledged, setSavedAcknowledged] = useState(false);
  const [passphrase, setPassphrase] = useState('');
  const [passphraseAgain, setPassphraseAgain] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [liveStatus, setLiveStatus] = useState<ArchiveStatus | null>(null);

  const beginSetup = useBeginArchiveSetupMutation();
  const rotateCode = useRotateRecoveryCodeMutation();
  const confirmSetup = useConfirmArchiveSetupMutation();
  const setPassphraseMutation = useSetArchivePassphraseMutation();
  const chooseDestination = useChooseArchiveDestinationMutation();

  const shownStatus = liveStatus ?? status ?? null;

  // A fresh open is a fresh key. Nothing from the previous run survives.
  useEffect(() => {
    if (!open) return;
    setStep('intro');
    setWords([]);
    setConfirmIndices([]);
    setTypedWords({});
    setSavedAcknowledged(false);
    setPassphrase('');
    setPassphraseAgain('');
    setError(null);
    setLiveStatus(null);
  }, [open, mode]);

  const isGenerating = beginSetup.isPending || rotateCode.isPending;

  const startCode = () => {
    setError(null);
    const mutation = isRotate ? rotateCode : beginSetup;
    mutation.mutate(undefined, {
      onSuccess: (setup) => {
        setWords(setup.recoveryWords);
        setConfirmIndices(setup.confirmIndices);
        setTypedWords({});
        setStep('words');
      },
      onError: (mutationError) => setError(mutationError.message),
    });
  };

  const copyWords = () => {
    const text = words.join(' ');
    const clipboard = navigator.clipboard;
    if (!clipboard?.writeText) {
      toast.error("Couldn't copy the recovery code", {
        message: 'Write the words down or print them instead.',
      });
      return;
    }
    void clipboard.writeText(text).then(
      () => {
        toast.success('Recovery code copied', {
          message: 'Paste it somewhere safe, then clear your clipboard.',
        });
      },
      () => {
        toast.error("Couldn't copy the recovery code", {
          message: 'Write the words down or print them instead.',
        });
      },
    );
  };

  const printWords = () => {
    if (!printRecoveryWords(words)) {
      toast.error("Couldn't open the print dialog", {
        message: 'Copy the words instead.',
      });
    }
  };

  const submitConfirmation = () => {
    setError(null);
    const confirmations: WordConfirmation[] = confirmIndices.map((index) => ({
      index,
      word: (typedWords[index] ?? '').trim(),
    }));
    // The passphrase is set in its own step, against the key this call stores.
    confirmSetup.mutate(
      { confirmations, passphrase: null },
      {
        onSuccess: (nextStatus) => {
          setLiveStatus(nextStatus);
          setStep(isRotate ? 'destination' : 'passphrase');
        },
        onError: (mutationError) => setError(mutationError.message),
      },
    );
  };

  const submitPassphrase = () => {
    if (passphrase.length < MIN_PASSPHRASE_LENGTH) {
      setError(`The passphrase has to be at least ${MIN_PASSPHRASE_LENGTH} characters.`);
      return;
    }
    if (passphrase !== passphraseAgain) {
      setError("The two passphrases don't match.");
      return;
    }
    setError(null);
    setPassphraseMutation.mutate(passphrase, {
      onSuccess: (nextStatus) => {
        setLiveStatus(nextStatus);
        setStep('destination');
      },
      onError: (mutationError) => setError(mutationError.message),
    });
  };

  const pickDestination = () => {
    setError(null);
    chooseDestination.mutate(undefined, {
      onSuccess: (nextStatus) => setLiveStatus(nextStatus),
      onError: (mutationError) => setError(mutationError.message),
    });
  };

  const confirmationsComplete = useMemo(
    () => confirmIndices.every((index) => (typedWords[index] ?? '').trim().length > 0),
    [confirmIndices, typedWords],
  );

  const title = isRotate ? 'New recovery code' : 'Set up off-device backup';

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
    >
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>
            {step === 'intro' && !isRotate
              ? 'An encrypted copy of your library, written to a folder you choose.'
              : null}
            {step === 'intro' && isRotate
              ? 'The old recovery code stops working as soon as you confirm the new one.'
              : null}
            {step === 'words' ? 'Step 2 of 5 — write these down.' : null}
            {step === 'confirm' ? 'Step 3 of 5 — prove you wrote them down.' : null}
            {step === 'passphrase' ? 'Step 4 of 5 — an optional second way in.' : null}
            {step === 'destination' ? 'Step 5 of 5 — where the archives go.' : null}
          </DialogDescription>
        </DialogHeader>

        {error ? (
          <p role="alert" className="text-sm text-danger">
            {error}
          </p>
        ) : null}

        {step === 'intro' ? (
          <div className="space-y-3 text-sm text-text-secondary">
            <p>
              Lattice writes one encrypted file each time it backs up, into a folder you pick. Put
              that folder in iCloud Drive, Dropbox, OneDrive, Google Drive, on a NAS, or on a USB
              stick and your library survives a dead drive.
            </p>
            <p>
              First you get a 24-word recovery code. It is the only way back into your backups if
              you forget the passphrase. Lattice never stores it and cannot reset it, so a lost
              recovery code with a forgotten passphrase means the backups cannot be read — by you or
              by anyone else.
            </p>
            {isRotate ? (
              <p>
                Archives written with the old code can still be opened with it. New archives use the
                new code.
              </p>
            ) : null}
          </div>
        ) : null}

        {step === 'words' ? (
          <div className="space-y-4">
            <ol className="grid grid-cols-2 gap-x-6 gap-y-1.5 sm:grid-cols-4">
              {words.map((word, index) => (
                <li
                  key={`${index}-${word}`}
                  className="flex items-baseline gap-2 border-b border-border-subtle py-1"
                >
                  <span className="w-5 shrink-0 text-right text-xs tabular-nums text-text-muted">
                    {index + 1}
                  </span>
                  <span className="font-mono text-sm text-text-primary">{word}</span>
                </li>
              ))}
            </ol>

            <div className="flex items-center gap-2">
              <button type="button" onClick={copyWords} className={SECONDARY_BUTTON_CLASS}>
                Copy
              </button>
              <button type="button" onClick={printWords} className={SECONDARY_BUTTON_CLASS}>
                Print
              </button>
            </div>

            <label className="flex items-start gap-2 text-sm text-text-secondary">
              <input
                type="checkbox"
                checked={savedAcknowledged}
                onChange={(event) => setSavedAcknowledged(event.target.checked)}
                className="mt-0.5 h-4 w-4 rounded-sm border-border-default bg-bg accent-accent"
              />
              <span>I have saved these words somewhere safe</span>
            </label>
          </div>
        ) : null}

        {step === 'confirm' ? (
          <div className="space-y-3">
            <p className="text-sm text-text-secondary">
              Type these three words from your recovery code.
            </p>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              {confirmIndices.map((index) => {
                const inputId = `archive-confirm-word-${index}`;
                return (
                  <div key={index}>
                    <label htmlFor={inputId} className="block text-xs text-text-secondary">
                      Word {index + 1}
                    </label>
                    <input
                      id={inputId}
                      type="text"
                      autoComplete="off"
                      autoCapitalize="none"
                      spellCheck={false}
                      value={typedWords[index] ?? ''}
                      onChange={(event) =>
                        setTypedWords((prev) => ({ ...prev, [index]: event.target.value }))
                      }
                      className="mt-1 h-8 w-full rounded-sm border border-border-default bg-bg px-2.5 font-mono text-sm text-text-primary outline-none transition-colors duration-fast focus:border-accent"
                    />
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}

        {step === 'passphrase' ? (
          <div className="space-y-3">
            <p className="text-sm text-text-secondary">
              A passphrase is a shorter thing to remember than 24 words. It is optional: without
              one, the recovery code is the only way into your archives. Lattice cannot reset a
              forgotten passphrase — the recovery code is what gets you back in.
            </p>
            <div className="space-y-2">
              <div>
                <label htmlFor="archive-passphrase" className="block text-xs text-text-secondary">
                  Passphrase
                </label>
                <input
                  id="archive-passphrase"
                  type="password"
                  autoComplete="new-password"
                  value={passphrase}
                  onChange={(event) => setPassphrase(event.target.value)}
                  className="mt-1 h-8 w-full rounded-sm border border-border-default bg-bg px-2.5 text-sm text-text-primary outline-none transition-colors duration-fast focus:border-accent"
                />
              </div>
              <div>
                <label
                  htmlFor="archive-passphrase-again"
                  className="block text-xs text-text-secondary"
                >
                  Passphrase again
                </label>
                <input
                  id="archive-passphrase-again"
                  type="password"
                  autoComplete="new-password"
                  value={passphraseAgain}
                  onChange={(event) => setPassphraseAgain(event.target.value)}
                  className="mt-1 h-8 w-full rounded-sm border border-border-default bg-bg px-2.5 text-sm text-text-primary outline-none transition-colors duration-fast focus:border-accent"
                />
              </div>
              <p className="text-xs text-text-muted">At least {MIN_PASSPHRASE_LENGTH} characters.</p>
            </div>
          </div>
        ) : null}

        {step === 'destination' ? (
          <div className="space-y-3">
            <p className="text-sm text-text-secondary">
              Pick the folder the archives are written to. Lattice creates a “Lattice Backups”
              folder inside it.
            </p>
            {shownStatus?.destination ? (
              <div className="flex items-center gap-2 border-b border-border-subtle py-2">
                <span className="min-w-0 flex-1 truncate font-mono text-xs text-text-primary">
                  {shownStatus.destination}
                </span>
                {shownStatus.destinationProvider ? (
                  <Badge variant="outline">{shownStatus.destinationProvider}</Badge>
                ) : null}
              </div>
            ) : (
              <p className="text-xs text-text-muted">No folder chosen yet.</p>
            )}
            <button
              type="button"
              onClick={pickDestination}
              disabled={chooseDestination.isPending}
              className={SECONDARY_BUTTON_CLASS}
            >
              {shownStatus?.destination ? 'Change folder' : 'Choose folder'}
            </button>
          </div>
        ) : null}

        <DialogFooter>
          <button type="button" onClick={onClose} className={GHOST_BUTTON_CLASS}>
            {step === 'destination' ? 'Close' : 'Cancel'}
          </button>

          {step === 'intro' ? (
            <button
              type="button"
              onClick={startCode}
              disabled={isGenerating}
              className={PRIMARY_BUTTON_CLASS}
            >
              {isGenerating ? 'Working…' : 'Show my recovery code'}
            </button>
          ) : null}

          {step === 'words' ? (
            <button
              type="button"
              onClick={() => setStep('confirm')}
              disabled={!savedAcknowledged}
              className={PRIMARY_BUTTON_CLASS}
            >
              Next
            </button>
          ) : null}

          {step === 'confirm' ? (
            <button
              type="button"
              onClick={submitConfirmation}
              disabled={!confirmationsComplete || confirmSetup.isPending}
              className={PRIMARY_BUTTON_CLASS}
            >
              {confirmSetup.isPending ? 'Checking…' : 'Confirm'}
            </button>
          ) : null}

          {step === 'passphrase' ? (
            <>
              <button
                type="button"
                onClick={() => {
                  setError(null);
                  setStep('destination');
                }}
                className={GHOST_BUTTON_CLASS}
              >
                Skip
              </button>
              <button
                type="button"
                onClick={submitPassphrase}
                disabled={setPassphraseMutation.isPending}
                className={PRIMARY_BUTTON_CLASS}
              >
                {setPassphraseMutation.isPending ? 'Saving…' : 'Set passphrase'}
              </button>
            </>
          ) : null}

          {step === 'destination' ? (
            <button type="button" onClick={onClose} className={PRIMARY_BUTTON_CLASS}>
              Done
            </button>
          ) : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
