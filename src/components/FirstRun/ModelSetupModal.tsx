/**
 * Single-bundle first-run install. Backend recommends; modal renders.
 * Click dismisses immediately — downloads continue in the header drawer.
 */
import { useEffect, useState } from 'react';

import { invoke } from '@tauri-apps/api/core';
import { Cpu } from 'lucide-react';

import { useModelCatalog } from '../../hooks/useModelCatalog';
import { VaultAPI } from '../../lib/api';
import { getErrorMessage } from '../../lib/errorUtils';
import { router } from '../../routes';
import { toast } from '../../stores/toastStore';
import { Button } from '../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';

interface ModelSetupModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onComplete: () => void;
  /**
   * Records that the user has been offered the bundle. Owned by the gate so
   * there is exactly one writer; it persists to the settings repository, not
   * to localStorage.
   */
  onDismiss: () => void;
}

interface RecommendedModel {
  model_id: string;
  display_name: string;
  estimated_size_bytes: number;
}

interface FirstRunStatusResponse {
  needs_setup: boolean;
  // Legacy fields kept for transitional safety
  recommended_model_id: string | null;
  recommended_model_name: string | null;
  estimated_size_bytes: number | null;
  // New shape
  embedding_model: RecommendedModel | null;
  chat_model: RecommendedModel | null;
  total_estimated_size_bytes: number | null;
}

function formatGb(bytes: number): string {
  const gb = bytes / 1_073_741_824;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  const mb = bytes / 1_048_576;
  return `${mb.toFixed(0)} MB`;
}

export function ModelSetupModal({
  open,
  onOpenChange,
  onComplete,
  onDismiss,
}: ModelSetupModalProps) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<FirstRunStatusResponse | null>(null);
  const { systemCapabilities } = useModelCatalog({
    autoLoadCapabilities: true,
    autoLoadModels: false,
  });
  useEffect(() => {
    if (!open) return;
    let alive = true;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const json = await invoke<string>('plugin:model|check_first_run_status');
        if (!alive) return;
        setStatus(JSON.parse(json));
      } catch (err) {
        if (alive) setError(getErrorMessage(err));
      } finally {
        if (alive) setLoading(false);
      }
    })();
    return () => {
      alive = false;
    };
  }, [open]);

  const handleSkip = () => {
    onDismiss();
    onOpenChange(false);
    onComplete();
  };

  const handleMoreOptions = () => {
    // Dismissing prevents a re-open in Settings, but don't mark complete —
    // user is choosing manually.
    onDismiss();
    onOpenChange(false);
    // Mounted outside RouterProvider, so useNavigate() is unavailable here.
    void router.navigate('/settings');
    onComplete();
  };

  const handleInstall = async () => {
    if (!status?.embedding_model && !status?.chat_model) return;
    setError(null);

    // Embedding uses its first-run dedicated command; chat uses the
    // generic download path. Both are fire-and-forget.
    const tasks: Array<Promise<unknown>> = [];

    if (status.embedding_model) {
      tasks.push(
        invoke<string>('plugin:model|download_default_embedding_model').catch((err) => {
          console.error('embedding download failed:', err);
          toast.error('Embedding model download failed', { message: getErrorMessage(err) });
        }),
      );
    }

    if (status.chat_model) {
      tasks.push(
        VaultAPI.downloadModel(status.chat_model.model_id).then((result) => {
          if (!result.ok) {
            console.error('chat download failed:', result.error);
            toast.error('Chat model download failed', { message: result.error });
          }
        }),
      );
    }

    const sizeNote = status.total_estimated_size_bytes
      ? ` · ${formatGb(status.total_estimated_size_bytes)}`
      : '';
    toast.success(`Installing models${sizeNote}`, {
      message: 'Downloading in the background.',
    });

    onOpenChange(false);
    onComplete();
    void Promise.allSettled(tasks);
  };

  if (!open || !status?.needs_setup) return null;

  const totalSize = status.total_estimated_size_bytes ?? 0;
  const sizeLabel = totalSize > 0 ? formatGb(totalSize) : '~5 GB';

  // The backend already sized this bundle to the machine's RAM, so a memory
  // verdict here would restate its own decision. Disk is the one thing it does
  // not check, and the one thing that makes the install fail halfway.
  const totalGb = totalSize / 1_073_741_824;
  const freeGb = systemCapabilities?.available_disk_gb ?? null;
  const notEnoughDisk = freeGb != null && freeGb > 0 && totalGb > freeGb;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-5 sm:max-w-[440px]">
        <DialogHeader className="space-y-2">
          <div className="mb-1 flex h-10 w-10 items-center justify-center rounded-xl bg-accent-muted text-accent">
            <Cpu className="h-5 w-5" strokeWidth={1.6} />
          </div>
          <DialogTitle>Set up AI</DialogTitle>
          <DialogDescription>
            Lattice picked a chat model and an embedding model that fit this Mac.
            Both download once and run on this machine.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          {loading && <p className="text-sm text-text-muted">Checking your hardware…</p>}

          {!loading && status.embedding_model && status.chat_model && (
            <div className="rounded-lg border border-border-subtle bg-[hsl(var(--text-primary)/0.025)] px-3.5">
              <BundleRow
                label="Chat"
                name={status.chat_model.display_name}
                bytes={status.chat_model.estimated_size_bytes}
              />
              <BundleRow
                label="Embedding"
                name={status.embedding_model.display_name}
                bytes={status.embedding_model.estimated_size_bytes}
              />
            </div>
          )}

          {!loading && notEnoughDisk && freeGb != null && (
            <p className="text-sm text-danger-fg">
              Not enough free disk. Needs {totalGb.toFixed(1)} GB, {freeGb.toFixed(1)} GB free.
            </p>
          )}

          {error && (
            <p className="text-sm text-danger-fg">
              Couldn&apos;t prepare a recommendation. {error}
            </p>
          )}
        </div>

        <DialogFooter className="flex-col gap-2 sm:flex-row sm:items-center">
          <Button variant="ghost" onClick={handleSkip} disabled={loading} className="sm:mr-auto">
            Not now
          </Button>
          <Button variant="ghost" onClick={handleMoreOptions} disabled={loading}>
            Choose models
          </Button>
          <Button
            onClick={handleInstall}
            disabled={loading || !status.embedding_model || notEnoughDisk}
          >
            Install · {sizeLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface BundleRowProps {
  label: string;
  name: string;
  bytes: number;
}

function BundleRow({ label, name, bytes }: BundleRowProps) {
  return (
    <div className="flex items-baseline justify-between gap-3 border-b border-border-subtle py-2.5 last:border-b-0">
      <span className="w-20 shrink-0 text-xs text-text-muted">{label}</span>
      <span className="min-w-0 flex-1 truncate text-sm text-text-primary">{name}</span>
      <span className="shrink-0 whitespace-nowrap text-xs tabular-nums text-text-secondary">
        {formatGb(bytes)}
      </span>
    </div>
  );
}
