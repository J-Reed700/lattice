/**
 * Single-bundle first-run install. Backend recommends; modal renders.
 * Click dismisses immediately — downloads continue in the header drawer.
 */
import { useQuery } from '@tanstack/react-query';
import { Cpu } from 'lucide-react';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { SeesImagesMark } from '@/features/model/components/SeesImagesMark';
import { firstRunStatusQueryOptions, useFirstRunDownloads } from '@/features/model/hooks/useFirstRunSetup';
import { useModelCatalog } from '@/features/model/hooks/useModelCatalog';
import { getErrorMessage } from '@/lib/errorUtils';
import { router } from '@/routes';
import { toast } from '@/stores/toastStore';
import { VISION_CAPABILITY } from '@/utils/visionGeometry';

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
  // Read again on every opening; what was read last stays up meanwhile.
  const statusQuery = useQuery({ ...firstRunStatusQueryOptions(), enabled: open });
  const status = statusQuery.data ?? null;
  const loading = statusQuery.isFetching;
  const error = statusQuery.isError ? getErrorMessage(statusQuery.error) : null;
  const { downloadEmbeddingModel, downloadChatModel } = useFirstRunDownloads();
  // The catalog says which of the picked models can see; the recommendation
  // itself carries names and sizes only.
  const { systemCapabilities, compatibleModels } = useModelCatalog({
    autoLoadCapabilities: true,
    autoLoadModels: open,
  });
  const chatModelSees = Boolean(status?.chat_model && compatibleModels.some(
    (entry) => entry.model.id === status.chat_model?.model_id && entry.model.capabilities.includes(VISION_CAPABILITY),
  ));

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

    // Embedding uses its first-run dedicated command; chat uses the
    // generic download path. Both are fire-and-forget.
    const tasks: Array<Promise<unknown>> = [];

    if (status.embedding_model) {
      tasks.push(
        downloadEmbeddingModel().then((result) => {
          if (!result.ok) {
            console.error('embedding download failed:', result.error);
            toast.error('Embedding model download failed', { message: result.error });
          }
        }),
      );
    }

    if (status.chat_model) {
      tasks.push(
        downloadChatModel(status.chat_model.model_id).then((result) => {
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
            Lattice picked a chat model and an embedding model that fit this computer.
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
                seesImages={chatModelSees}
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
          {/* Skipping never depends on the status, so a re-read never blocks
              it; only Install waits for the fresh recommendation. */}
          <Button variant="ghost" onClick={handleSkip} className="sm:mr-auto">
            Not now
          </Button>
          <Button variant="ghost" onClick={handleMoreOptions}>
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
  seesImages?: boolean;
}

function BundleRow({ label, name, bytes, seesImages = false }: BundleRowProps) {
  return (
    <div className="flex items-baseline justify-between gap-3 border-b border-border-subtle py-2.5 last:border-b-0">
      <span className="w-20 shrink-0 text-xs text-text-muted">{label}</span>
      <span className="min-w-0 flex-1 truncate text-sm text-text-primary">{name}</span>
      {seesImages ? <SeesImagesMark /> : null}
      <span className="shrink-0 whitespace-nowrap text-xs tabular-nums text-text-secondary">
        {formatGb(bytes)}
      </span>
    </div>
  );
}
