/**
 * HuggingFaceSettings
 *
 * The Hugging Face token used for downloading gated models. The token is
 * stored in the OS keyring by the backend; the renderer never reads it back.
 *
 * Token hygiene, non-negotiable:
 *  - The plaintext lives only in this component's `useState` while the user is
 *    typing, and is cleared on success.
 *  - It never reaches the React Query cache, a toast, a log, or an error
 *    message. `mutation.reset()` after a save drops the mutation variables too.
 *  - `get_huggingface_token` (which returns the plaintext) is never called.
 *    The status query carries `{ isSet }` and nothing else.
 */

import { useEffect, useState } from 'react';

import { Eye, EyeOff } from 'lucide-react';

import { cn } from '@/lib/utils';

import { SECONDARY_BUTTON_CLASS } from './settingsStyles';
import {
  useDeleteHuggingFaceTokenMutation,
  useHuggingFaceTokenStatusQuery,
  useSetHuggingFaceTokenMutation,
} from '../../hooks/queries/useHuggingFaceTokenQuery';
import { toast } from '../../stores/toastStore';
import { SettingsRow, SettingsSection, settingsFieldClass } from '../ui';

export function HuggingFaceSettings() {
  const { data: status, isLoading: statusLoading } = useHuggingFaceTokenStatusQuery();
  const saveToken = useSetHuggingFaceTokenMutation();
  const deleteToken = useDeleteHuggingFaceTokenMutation();

  const [token, setToken] = useState('');
  const [showToken, setShowToken] = useState(false);
  /** Last four characters of the token typed *this session*. Never from the backend. */
  const [last4, setLast4] = useState<string | null>(null);

  const isSet = status?.isSet ?? false;
  const isBusy = saveToken.isPending || deleteToken.isPending;

  // Revealing is a per-entry decision; a stored token is never revealed by default.
  useEffect(() => {
    setShowToken(false);
  }, [isSet]);

  const handleSave = () => {
    const trimmed = token.trim();
    if (!trimmed) {
      toast.error('Invalid token', { message: 'Enter a Hugging Face token' });
      return;
    }

    saveToken.mutate(trimmed, {
      onSuccess: () => {
        setLast4(trimmed.slice(-4));
        setToken('');
        setShowToken(false);
        // Drop the mutation variables so the plaintext is not retained.
        saveToken.reset();
        toast.success('Token saved');
      },
      onError: (error) => {
        toast.error("Couldn't save the token", { message: error.message });
      },
    });
  };

  const handleDelete = () => {
    deleteToken.mutate(undefined, {
      onSuccess: () => {
        setLast4(null);
        setToken('');
        toast.success('Token removed');
      },
      onError: (error) => {
        toast.error("Couldn't remove the token", { message: error.message });
      },
    });
  };

  const hint = statusLoading
    ? 'Loading…'
    : isSet
      ? last4
        ? `Token set · ends in …${last4}`
        : 'Token set.'
      : 'Needed for gated models like Gemma and Mistral.';

  return (
    <SettingsSection
      title="Hugging Face"
      actions={
        isSet ? (
          <button
            type="button"
            onClick={handleDelete}
            disabled={isBusy}
            className={SECONDARY_BUTTON_CLASS}
          >
            Remove token
          </button>
        ) : null
      }
    >
      <SettingsRow
        label={isSet ? 'Replace token' : 'Token'}
        hint={hint}
        htmlFor="hf-token"
        stacked
      >
        <div className="flex gap-2">
          <div className="relative min-w-0 flex-1">
            <input
              id="hf-token"
              type={showToken ? 'text' : 'password'}
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="hf_..."
              disabled={isBusy}
              autoComplete="off"
              spellCheck={false}
              autoCorrect="off"
              data-1p-ignore
              className={cn(settingsFieldClass, 'pr-9 font-mono')}
            />
            <button
              type="button"
              onClick={() => setShowToken(!showToken)}
              aria-label={showToken ? 'Hide token' : 'Show token'}
              className="absolute right-1 top-1/2 inline-flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-sm text-text-muted transition-colors duration-fast hover:text-text-primary"
            >
              {showToken ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
            </button>
          </div>
          <button
            type="button"
            onClick={handleSave}
            disabled={isBusy || !token.trim()}
            className={SECONDARY_BUTTON_CLASS}
          >
            {saveToken.isPending ? 'Saving…' : 'Save'}
          </button>
        </div>
      </SettingsRow>

      <SettingsRow label="Create a token" stacked>
        <a
          href="https://huggingface.co/settings/tokens"
          target="_blank"
          rel="noopener noreferrer"
          className="text-sm text-accent hover:underline"
        >
          huggingface.co/settings/tokens
        </a>
      </SettingsRow>
    </SettingsSection>
  );
}
