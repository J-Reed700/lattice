import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ArchiveStatus } from '@/types/api/backup';

import { ArchiveSetupWizard } from './ArchiveSetupWizard';

type Callbacks = { onSuccess?: (value: any) => void; onError?: (error: Error) => void };
type Outcome = { ok: true; value: unknown } | { ok: false; error: string };

const h = vi.hoisted(() => {
  const fake = () => {
    const mutation = {
      outcome: null as null | { ok: true; value: unknown } | { ok: false; error: string },
      isPending: false,
      mutate: vi.fn(),
    };
    mutation.mutate.mockImplementation((_variables: unknown, callbacks: Callbacks) => {
      if (!mutation.outcome) return;
      if (mutation.outcome.ok) callbacks.onSuccess?.(mutation.outcome.value);
      else callbacks.onError?.(new Error(mutation.outcome.error));
    });
    return mutation;
  };
  return {
    begin: fake(),
    rotate: fake(),
    confirm: fake(),
    passphrase: fake(),
    destination: fake(),
    toast: { success: vi.fn(), error: vi.fn() },
  };
});

vi.mock('../hooks/useArchiveQuery', () => ({
  useBeginArchiveSetupMutation: () => h.begin,
  useRotateRecoveryCodeMutation: () => h.rotate,
  useConfirmArchiveSetupMutation: () => h.confirm,
  useSetArchivePassphraseMutation: () => h.passphrase,
  useChooseArchiveDestinationMutation: () => h.destination,
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const WORDS = Array.from({ length: 24 }, (_v, i) => `word${i + 1}`);
const setup = { recoveryWords: WORDS, confirmIndices: [0, 9, 23] };

const status = (overrides: Partial<ArchiveStatus> = {}) => ({
  configured: true, destination: null, destinationProvider: null, destinationMissing: false, keepCount: 5,
  hasPassphrase: false, recoveryConfirmed: true, lastSuccess: null, lastError: null, dataDirCloudProvider: null, archives: [],
  ...overrides,
}) as ArchiveStatus;

const ok = (value: unknown): Outcome => ({ ok: true, value });
const fail = (error: string): Outcome => ({ ok: false, error });

const click = (name: string | RegExp) => fireEvent.click(screen.getByRole('button', { name }));

const toWords = () => {
  click('Show my recovery code');
  expect(screen.getByText('word24')).toBeInTheDocument();
};

const toConfirm = () => {
  toWords();
  fireEvent.click(screen.getByLabelText('I have saved these words somewhere safe'));
  click('Next');
};

const typeConfirmations = () => {
  fireEvent.change(screen.getByLabelText('Word 1'), { target: { value: ' word1 ' } });
  fireEvent.change(screen.getByLabelText('Word 10'), { target: { value: 'word10' } });
  fireEvent.change(screen.getByLabelText('Word 24'), { target: { value: 'word24' } });
};

describe('ArchiveSetupWizard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const mutation of [h.begin, h.rotate, h.confirm, h.passphrase, h.destination]) {
      mutation.outcome = null;
      mutation.isPending = false;
    }
    h.begin.outcome = ok(setup);
    h.rotate.outcome = ok(setup);
    h.confirm.outcome = ok(status());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders nothing while closed', () => {
    render(<ArchiveSetupWizard open={false} mode="setup" onClose={vi.fn()} />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('shows the generation error on the intro step', () => {
    h.begin.outcome = fail('keyring unavailable');
    render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
    click('Show my recovery code');
    expect(screen.getByText('keyring unavailable')).toBeInTheDocument();
  });

  it('says Working… while the code is generated', () => {
    h.begin.isPending = true;
    render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Working…' })).toBeDisabled();
  });

  describe('the words step', () => {
    it('copies the words', async () => {
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toWords();
      click('Copy');
      expect(writeText).toHaveBeenCalledWith(WORDS.join(' '));
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith('Recovery code copied', expect.anything()));
    });

    it('reports a refused copy', async () => {
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) } });
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toWords();
      click('Copy');
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't copy the recovery code", expect.anything()));
    });

    it('reports a missing clipboard', () => {
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: undefined });
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toWords();
      click('Copy');
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't copy the recovery code", { message: 'Write the words down or print them instead.' });
    });

    it('prints only the words, then removes the print page', () => {
      let printed = '';
      const print = vi.fn(() => { printed = document.getElementById('lattice-recovery-print')?.textContent ?? ''; });
      vi.spyOn(window, 'print').mockImplementation(print);
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toWords();
      click('Print');
      expect(print).toHaveBeenCalled();
      expect(printed).toContain('Lattice recovery code');
      expect(printed).toContain('word17');
      expect(document.getElementById('lattice-recovery-print')).toBeNull();
    });

    it('reports a missing print dialog', () => {
      const original = window.print;
      Object.defineProperty(window, 'print', { configurable: true, value: undefined });
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toWords();
      click('Print');
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't open the print dialog", { message: 'Copy the words instead.' });
      Object.defineProperty(window, 'print', { configurable: true, value: original });
    });

    it('needs the words acknowledged before going on', () => {
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toWords();
      expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();
    });
  });

  describe('confirming', () => {
    it('needs all three words before checking', () => {
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toConfirm();
      expect(screen.getByRole('button', { name: 'Confirm' })).toBeDisabled();
      typeConfirmations();
      click('Confirm');
      expect(h.confirm.mutate).toHaveBeenCalledWith(
        { confirmations: [{ index: 0, word: 'word1' }, { index: 9, word: 'word10' }, { index: 23, word: 'word24' }], passphrase: null },
        expect.anything(),
      );
      expect(screen.getByLabelText('Passphrase')).toBeInTheDocument();
    });

    it('says Checking… while the words are verified', () => {
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toConfirm();
      typeConfirmations();
      h.confirm.isPending = true;
      fireEvent.change(screen.getByLabelText('Word 1'), { target: { value: 'word1' } });
      expect(screen.getByRole('button', { name: 'Checking…' })).toBeDisabled();
    });
  });

  describe('passphrase step', () => {
    const toPassphrase = () => {
      toConfirm();
      typeConfirmations();
      click('Confirm');
    };

    it('checks length and agreement', () => {
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toPassphrase();
      fireEvent.change(screen.getByLabelText('Passphrase'), { target: { value: 'short' } });
      click('Set passphrase');
      expect(screen.getByText('The passphrase has to be at least 8 characters.')).toBeInTheDocument();
      fireEvent.change(screen.getByLabelText('Passphrase'), { target: { value: 'tide pools at dawn' } });
      fireEvent.change(screen.getByLabelText('Passphrase again'), { target: { value: 'tide pools at dusk' } });
      click('Set passphrase');
      expect(screen.getByText("The two passphrases don't match.")).toBeInTheDocument();
      expect(h.passphrase.mutate).not.toHaveBeenCalled();
    });

    it('saves the passphrase and moves to the folder', () => {
      h.passphrase.outcome = ok(status({ hasPassphrase: true }));
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toPassphrase();
      fireEvent.change(screen.getByLabelText('Passphrase'), { target: { value: 'tide pools at dawn' } });
      fireEvent.change(screen.getByLabelText('Passphrase again'), { target: { value: 'tide pools at dawn' } });
      click('Set passphrase');
      expect(h.passphrase.mutate).toHaveBeenCalledWith('tide pools at dawn', expect.anything());
      expect(screen.getByText('Step 5 of 5 — where the archives go.')).toBeInTheDocument();
    });

    it('shows a refused passphrase', () => {
      h.passphrase.outcome = fail('keyring locked');
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toPassphrase();
      fireEvent.change(screen.getByLabelText('Passphrase'), { target: { value: 'tide pools at dawn' } });
      fireEvent.change(screen.getByLabelText('Passphrase again'), { target: { value: 'tide pools at dawn' } });
      click('Set passphrase');
      expect(screen.getByText('keyring locked')).toBeInTheDocument();
    });

    it('can be skipped, and says Saving… while saving', () => {
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toPassphrase();
      h.passphrase.isPending = true;
      fireEvent.change(screen.getByLabelText('Passphrase'), { target: { value: 'x' } });
      expect(screen.getByRole('button', { name: 'Saving…' })).toBeDisabled();
      click('Skip');
      expect(screen.getByText('No folder chosen yet.')).toBeInTheDocument();
    });
  });

  describe('destination step', () => {
    const toDestination = () => {
      toConfirm();
      typeConfirmations();
      click('Confirm');
      click('Skip');
    };

    it('chooses a folder and shows it with its provider', () => {
      h.destination.outcome = ok(status({ destination: '/Users/ada/Dropbox/Lattice', destinationProvider: 'Dropbox' }));
      const onClose = vi.fn();
      render(<ArchiveSetupWizard open mode="setup" onClose={onClose} />);
      toDestination();
      click('Choose folder');
      expect(screen.getByText('/Users/ada/Dropbox/Lattice')).toBeInTheDocument();
      expect(screen.getByText('Dropbox')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Change folder' })).toBeInTheDocument();
      click('Done');
      expect(onClose).toHaveBeenCalled();
    });

    it('shows a folder that could not be used', () => {
      h.destination.outcome = fail('folder is not writable');
      render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
      toDestination();
      click('Choose folder');
      expect(screen.getByText('folder is not writable')).toBeInTheDocument();
    });

    it('starts from the folder the parent already knows', () => {
      h.confirm.outcome = ok(null);
      render(<ArchiveSetupWizard open mode="setup" status={status({ destination: '/Volumes/Backup' })} onClose={vi.fn()} />);
      toDestination();
      expect(screen.getByText('/Volumes/Backup')).toBeInTheDocument();
    });
  });

  describe('rotating the code', () => {
    it('reissues the code and goes straight to the folder after confirming', () => {
      render(<ArchiveSetupWizard open mode="rotate" onClose={vi.fn()} />);
      expect(screen.getByRole('heading', { name: 'New recovery code' })).toBeInTheDocument();
      expect(screen.getByText('The old recovery code stops working as soon as you confirm the new one.')).toBeInTheDocument();
      toConfirm();
      expect(h.rotate.mutate).toHaveBeenCalled();
      expect(h.begin.mutate).not.toHaveBeenCalled();
      typeConfirmations();
      click('Confirm');
      expect(screen.getByText('Step 5 of 5 — where the archives go.')).toBeInTheDocument();
    });

    it('shows a wrong word and stays on the step', () => {
      h.confirm.outcome = fail("That isn't the right word.");
      render(<ArchiveSetupWizard open mode="rotate" onClose={vi.fn()} />);
      toConfirm();
      typeConfirmations();
      click('Confirm');
      expect(screen.getByText("That isn't the right word.")).toBeInTheDocument();
      expect(screen.getByLabelText('Word 1')).toBeInTheDocument();
    });
  });

  it('starts over when reopened', () => {
    const view = render(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
    toWords();
    view.rerender(<ArchiveSetupWizard open={false} mode="setup" onClose={vi.fn()} />);
    view.rerender(<ArchiveSetupWizard open mode="setup" onClose={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Show my recovery code' })).toBeInTheDocument();
  });

  it('closes from Cancel and Escape', () => {
    const onClose = vi.fn();
    render(<ArchiveSetupWizard open mode="setup" onClose={onClose} />);
    click('Cancel');
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
