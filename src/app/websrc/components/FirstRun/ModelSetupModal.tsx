/**
 * ModelSetupModal — opinionated first-run UX
 *
 * Single bundle install: one giant button that downloads BOTH the embedding
 * model (for indexing) and a hardware-sized chat model (for Q&A). The
 * backend does the hardware probe + recommendation; the modal just renders
 * what it gets.
 *
 * The instant the user clicks Install, we dismiss the modal and let them
 * use the app immediately. Downloads continue in the header progress
 * drawer (already built — `useDownloads`). The chat input shows a
 * "Warming up AI…" skeleton until the prewarm event fires.
 *
 * Notes / BM25 search / Daily Note are fully usable during this window.
 */
import { useEffect, useState } from 'react';

import { invoke } from '@tauri-apps/api/core';
import { Download, Settings as SettingsIcon, Sparkles, XCircle } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { getErrorMessage } from '../../lib/errorUtils';
import { VaultAPI } from '../../lib/api';
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

export function ModelSetupModal({ open, onOpenChange, onComplete }: ModelSetupModalProps) {
  const navigate = useNavigate();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<FirstRunStatusResponse | null>(null);

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
    localStorage.setItem('lattice:first-run-skipped', 'true');
    onOpenChange(false);
    onComplete();
  };

  const handleMoreOptions = () => {
    // Persist the skip so we don't reopen on the way to settings, but
    // don't mark setup complete — user is choosing a model manually.
    localStorage.setItem('lattice:first-run-skipped', 'true');
    onOpenChange(false);
    navigate('/settings');
    onComplete();
  };

  const handleInstall = async () => {
    if (!status?.embedding_model && !status?.chat_model) return;
    setError(null);

    // Fire both downloads. Embedding has its own dedicated command (special-
    // cased on the backend for first-run telemetry); chat goes through the
    // generic download path. Don't await — the app should usable before
    // either finishes. Failures surface as toasts via the existing
    // download event listeners.
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

    // Show one ack so the user understands the dismiss isn't a bug.
    const sizeNote = status.total_estimated_size_bytes
      ? ` (${formatGb(status.total_estimated_size_bytes)})`
      : '';
    toast.success(`Installing recommended AI${sizeNote}`, {
      message: 'Downloading in the background — you can start using Lattice now.',
    });

    // Dismiss IMMEDIATELY — the parallel-experience choreography from the
    // 60-day plan. Don't await tasks; let them keep running.
    onOpenChange(false);
    onComplete();
    void Promise.allSettled(tasks);
  };

  if (!open || !status || !status.needs_setup) return null;

  const totalSize = status.total_estimated_size_bytes ?? 0;
  const sizeLabel = totalSize > 0 ? formatGb(totalSize) : '~5 GB';

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="w-5 h-5 text-[hsl(var(--accent))]" />
            Welcome to Lattice
          </DialogTitle>
          <DialogDescription>
            We picked an AI bundle that fits your machine. Install it now and we&apos;ll
            drop you into your Daily Note while it downloads.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {loading && (
            <div className="flex items-center justify-center py-8">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-[hsl(var(--accent))]" />
            </div>
          )}

          {!loading && status.embedding_model && status.chat_model && (
            <div className="rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] p-4 space-y-3">
              <BundleRow
                label="Chat model"
                name={status.chat_model.display_name}
                bytes={status.chat_model.estimated_size_bytes}
              />
              <div className="border-t border-[hsl(var(--border-subtle))]" />
              <BundleRow
                label="Embedding model"
                name={status.embedding_model.display_name}
                bytes={status.embedding_model.estimated_size_bytes}
              />
            </div>
          )}

          {error && (
            <div className="bg-[hsl(var(--danger-muted))] p-4 rounded-lg border border-[hsl(var(--danger-fg))]">
              <div className="flex items-start gap-3">
                <XCircle className="w-5 h-5 text-[hsl(var(--danger-fg))] mt-0.5" />
                <div className="flex-1">
                  <h3 className="font-semibold text-sm text-[hsl(var(--danger-fg))] mb-1">
                    Couldn&apos;t prepare recommendation
                  </h3>
                  <p className="text-xs text-[hsl(var(--danger-fg))]">{error}</p>
                </div>
              </div>
            </div>
          )}
        </div>

        <DialogFooter className="flex-col sm:flex-row gap-2">
          <Button variant="ghost" onClick={handleSkip} disabled={loading}>
            Skip for now
          </Button>
          <Button
            variant="ghost"
            onClick={handleMoreOptions}
            disabled={loading}
            className="text-[hsl(var(--text-secondary))]"
          >
            <SettingsIcon className="w-4 h-4 mr-2" />
            More options
          </Button>
          <Button onClick={handleInstall} disabled={loading || !status.embedding_model}>
            <Download className="w-4 h-4 mr-2" />
            Install Recommended AI ({sizeLabel})
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
    <div className="flex items-start justify-between gap-3">
      <div className="flex-1 min-w-0">
        <p className="text-[10px] uppercase tracking-wide text-[hsl(var(--text-tertiary))]">
          {label}
        </p>
        <p className="text-sm font-medium text-[hsl(var(--text-primary))] truncate">{name}</p>
      </div>
      <span className="text-xs tabular-nums text-[hsl(var(--text-secondary))] whitespace-nowrap">
        {formatGb(bytes)}
      </span>
    </div>
  );
}
