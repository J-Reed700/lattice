/**
 * First-run install. The backend ranks chat models for this computer; the
 * modal lists them, the user picks one, and Install adds the embedding model.
 * Click dismisses immediately — downloads continue in the header drawer.
 * "Use an API key instead" swaps the cards for a cloud key form that sets the
 * chat and Learning model in one write, with no download.
 */
import { useState } from 'react';

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
import { useApi } from '@/contexts/ApiContext';
import { ApiKeyForm } from '@/features/model/components/FirstRun/ApiKeyForm';
import { RecommendationCardBody } from '@/features/model/components/RecommendationCardBody';
import { firstRunStatusQueryOptions, useFirstRunDownloads } from '@/features/model/hooks/useFirstRunSetup';
import { useModelCatalog } from '@/features/model/hooks/useModelCatalog';
import { useRecommendedChatModels } from '@/features/model/hooks/useRecommendedChatModels';
import { getErrorMessage } from '@/lib/errorUtils';
import { cn } from '@/lib/utils';
import { router } from '@/routes';
import { toast } from '@/stores/toastStore';
import { handleAsyncEvent } from '@/utils/promiseHandlers';
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
  const api = useApi();
  // Read again on every opening; what was read last stays up meanwhile.
  const statusQuery = useQuery({ ...firstRunStatusQueryOptions(api), enabled: open });
  const status = statusQuery.data ?? null;
  const recommendationsQuery = useRecommendedChatModels(open);
  const loading = statusQuery.isFetching || recommendationsQuery.isFetching;
  const error = statusQuery.isError
    ? getErrorMessage(statusQuery.error)
    : recommendationsQuery.isError ? getErrorMessage(recommendationsQuery.error) : null;
  // A chat model is offered only while the machine still needs one.
  const recommendations = status?.chat_model ? recommendationsQuery.data ?? [] : [];
  const [useKey, setUseKey] = useState(false);
  const [chosenId, setChosenId] = useState<string | null>(null);
  const selected = recommendations.find((entry) => entry.modelId === chosenId) ?? recommendations[0] ?? null;
  const chatModelId = selected?.modelId ?? status?.chat_model?.model_id ?? null;
  const { downloadEmbeddingModel, downloadChatModel } = useFirstRunDownloads();
  // The catalog says which of the listed models can see.
  const { systemCapabilities, compatibleModels } = useModelCatalog({
    autoLoadCapabilities: true,
    autoLoadModels: open,
  });
  const seesImages = (modelId: string) => compatibleModels.some(
    (entry) => entry.model.id === modelId && entry.model.capabilities.includes(VISION_CAPABILITY),
  );
  const embeddingBytes = status?.embedding_model?.estimated_size_bytes ?? 0;
  const totalSize = selected
    ? selected.downloadBytes + embeddingBytes
    : status?.total_estimated_size_bytes ?? 0;

  const handleSkip = () => {
    onDismiss();
    onOpenChange(false);
    onComplete();
  };

  const handleKeyDone = () => {
    onDismiss();
    onOpenChange(false);
    onComplete();
  };

  const handleSeeAll = () => {
    // Dismissing prevents a re-open in Settings, but don't mark complete —
    // user is choosing manually.
    onDismiss();
    onOpenChange(false);
    // Mounted outside RouterProvider, so useNavigate() is unavailable here.
    void router.navigate('/settings?tab=models');
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

    if (chatModelId) {
      tasks.push(
        downloadChatModel(chatModelId).then((result) => {
          if (!result.ok) {
            console.error('chat download failed:', result.error);
            toast.error('Chat model download failed', { message: result.error });
          }
        }),
      );
    }

    const sizeNote = totalSize ? ` · ${formatGb(totalSize)}` : '';
    toast.success(`Installing models${sizeNote}`, {
      message: 'Downloading in the background.',
    });

    onOpenChange(false);
    onComplete();
    void Promise.allSettled(tasks);
  };

  if (!open || !status?.needs_setup) return null;

  const sizeLabel = totalSize > 0 ? formatGb(totalSize) : '~5 GB';

  // The backend already judged every card against the machine's memory, so a
  // memory verdict here would restate its own decision. Disk is the one thing
  // it does not check, and the one thing that makes the install fail halfway.
  const totalGb = totalSize / 1_073_741_824;
  const freeGb = systemCapabilities?.available_disk_gb ?? null;
  const notEnoughDisk = freeGb != null && freeGb > 0 && totalGb > freeGb;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="gap-5 sm:max-w-[520px]">
        <DialogHeader className="space-y-2">
          <div className="mb-1 flex h-10 w-10 items-center justify-center rounded-xl bg-accent-muted text-accent">
            <Cpu className="h-5 w-5" strokeWidth={1.6} />
          </div>
          <DialogTitle>Set up AI</DialogTitle>
          <DialogDescription>
            {useKey
              ? 'Use your own API key for chats and courses. Nothing is downloaded.'
              : 'Pick a chat model that fits this computer. Lattice adds an embedding model. Both download once and run on this machine.'}
          </DialogDescription>
        </DialogHeader>

        {useKey ? <ApiKeyForm onBack={() => setUseKey(false)} onDone={handleKeyDone} /> : <>
        <div className="space-y-3">
          {loading && <p className="text-sm text-text-muted">Checking your hardware…</p>}

          {!loading && selected && (
            <div role="radiogroup" aria-label="Recommended for this computer" className="space-y-2">
              {recommendations.map((entry) => {
                const checked = entry.modelId === selected.modelId;
                return (
                  <label
                    key={entry.modelId}
                    className={cn(
                      'flex cursor-pointer items-start gap-3 rounded-lg border px-3.5 py-3 has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring',
                      checked ? 'border-accent bg-accent-muted/40' : 'border-border-subtle hover:bg-surface-hover',
                    )}
                  >
                    <input
                      type="radio"
                      name="first-run-chat-model"
                      className="mt-1 accent-[hsl(var(--accent))]"
                      checked={checked}
                      onChange={() => setChosenId(entry.modelId)}
                    />
                    <RecommendationCardBody recommendation={entry} seesImages={seesImages(entry.modelId)} />
                  </label>
                );
              })}
            </div>
          )}

          {!loading && status.embedding_model && status.chat_model && (
            <div className="rounded-lg border border-border-subtle bg-[hsl(var(--text-primary)/0.025)] px-3.5">
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
          <Button variant="ghost" onClick={() => setUseKey(true)}>
            Use an API key instead
          </Button>
          <Button variant="ghost" onClick={handleSeeAll}>
            See all models
          </Button>
          <Button
            onClick={handleAsyncEvent(handleInstall)}
            disabled={loading || !status.embedding_model || notEnoughDisk}
          >
            Install · {sizeLabel}
          </Button>
        </DialogFooter>
        </>}
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
