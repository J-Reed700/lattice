/**
 * HuggingFaceSettings Component
 *
 * Manages HuggingFace authentication token for downloading gated models.
 * Tokens are stored securely in the OS keyring.
 */

import { useState, useEffect } from 'react';

import { Eye, EyeOff, Check, Trash2, ExternalLink, Info } from 'lucide-react';

import VaultAPI from '../../lib/api';
import { useToastStore } from '../../stores/toastStore';
import { Button } from '../ui/button';
import Card, { CardHeader, CardTitle, CardContent } from '../ui/Card/Card';

export function HuggingFaceSettings() {
  // Hugging Face tokens are intentionally optional for public model browsing.
  // This component is retained for gated/private model downloads, but is not mounted by default.
  const [token, setToken] = useState('');
  const [isTokenSet, setIsTokenSet] = useState(false);
  const [showToken, setShowToken] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const addToast = useToastStore((state) => state.addToast);

  // Load token status on mount
  useEffect(() => {
    loadTokenStatus();
  }, []);

  const loadTokenStatus = async () => {
    const result = await VaultAPI.getHuggingFaceTokenStatus();
    if (result.ok) {
      setIsTokenSet(result.data.isSet);
    } else {
      console.error('Failed to load HuggingFace token status:', result.error);
    }
  };

  const handleSaveToken = async () => {
    if (!token.trim()) {
      addToast({
        type: 'error',
        title: 'Invalid Token',
        message: 'Please enter a HuggingFace token',
      });
      return;
    }

    setIsLoading(true);

    const result = await VaultAPI.setHuggingFaceToken(token.trim());

    if (result.ok) {
      addToast({
        type: 'success',
        title: 'Token Saved',
        message: 'HuggingFace token saved securely',
      });

      setToken('');
      setIsTokenSet(true);
      setShowToken(false);
    } else {
      console.error('Failed to save HuggingFace token:', result.error);

      addToast({
        type: 'error',
        title: 'Save Failed',
        message: result.error,
      });
    }

    setIsLoading(false);
  };

  const handleDeleteToken = async () => {
    setIsLoading(true);

    const result = await VaultAPI.deleteHuggingFaceToken();

    if (result.ok) {
      addToast({
        type: 'success',
        title: 'Token Deleted',
        message: 'HuggingFace token has been removed',
      });

      setIsTokenSet(false);
    } else {
      console.error('Failed to delete HuggingFace token:', result.error);

      addToast({
        type: 'error',
        title: 'Delete Failed',
        message: 'Failed to delete HuggingFace token',
      });
    }

    setIsLoading(false);
  };

  return (
    <Card padding="lg">
      <CardHeader>
        <CardTitle>HuggingFace Integration</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="space-y-6 mt-4">
          {/* Info Section */}
          <div className="flex items-start gap-3 p-4 bg-[hsl(var(--accent-muted))] rounded-lg border border-[hsl(var(--accent-muted))]">
            <Info className="w-5 h-5 text-[hsl(var(--accent))] mt-0.5 flex-shrink-0" />
            <div className="space-y-2 text-sm">
              <p className="text-[hsl(var(--text-primary))] font-medium">
                Why do I need a HuggingFace token?
              </p>
              <p className="text-[hsl(var(--text-secondary))]">
                Some models require authentication to download. A HuggingFace token allows you to
                download gated models that require acceptance of license terms.
              </p>
              <a
                href="https://huggingface.co/settings/tokens"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-[hsl(var(--accent))] hover:underline"
              >
                Create a token on HuggingFace
                <ExternalLink className="w-3 h-3" />
              </a>
            </div>
          </div>

          {/* Token Status */}
          {isTokenSet && (
            <div className="flex items-center justify-between p-4 bg-[hsl(var(--success-muted))] rounded-lg border border-[hsl(var(--success-muted))]">
              <div className="flex items-center gap-2">
                <Check className="w-5 h-5 text-[hsl(var(--success-fg))]" />
                <span className="text-sm font-medium text-[hsl(var(--success-fg))]">
                  Token is configured
                </span>
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleDeleteToken}
                disabled={isLoading}
              >
                <Trash2 className="w-4 h-4" />
                Remove
              </Button>
            </div>
          )}

          {/* Token Input */}
          <div className="space-y-2">
            <label htmlFor="hf-token" className="block text-sm font-medium text-[hsl(var(--text-primary))]">
              {isTokenSet ? 'Update Token' : 'HuggingFace Token'}
            </label>
            <div className="relative">
              <input
                id="hf-token"
                type={showToken ? 'text' : 'password'}
                value={token}
                onChange={(e) => setToken(e.target.value)}
                placeholder="hf_..."
                className="w-full px-3 py-2 pr-10 bg-[hsl(var(--surface))] border border-[hsl(var(--border-subtle))] rounded-md text-[hsl(var(--text-primary))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--accent))]"
                disabled={isLoading}
              />
              <button
                type="button"
                onClick={() => setShowToken(!showToken)}
                className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]"
              >
                {showToken ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
              </button>
            </div>
            <p className="text-xs text-[hsl(var(--text-secondary))]">
              Your token will be stored securely in your system keyring
            </p>
          </div>

          {/* Actions */}
          <div className="flex gap-3">
            <Button
              variant="default"
              onClick={handleSaveToken}
              disabled={isLoading || !token.trim()}
            >
              {isLoading ? 'Saving...' : isTokenSet ? 'Update Token' : 'Save Token'}
            </Button>
          </div>

          {/* Security Note */}
          <div className="p-3 bg-[hsl(var(--surface-raised))] rounded-md border border-[hsl(var(--border-subtle))]">
            <p className="text-xs text-[hsl(var(--text-secondary))]">
              🔒 <strong>Security:</strong> Your token is stored in your operating system's secure
              keyring (Keychain on macOS, Credential Manager on Windows, Secret Service on Linux).
              It is never stored in plain text.
            </p>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
