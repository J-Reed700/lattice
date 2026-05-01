/**
 * LocalModelCard
 *
 * One card per local downloaded model in the AI Models grid. The action row
 * is generated from {@link ROLES} so adding a fourth role doesn't require
 * touching this component.
 *
 * Ollama-backed models are NOT rendered here — they live inside the
 * {@link OllamaMetaCard} meta-card. See that file for the server-level view.
 */

import { useState } from 'react';

import { formatDistanceToNow } from 'date-fns';
import { Calendar, Flame, HardDrive, Trash2, TrendingUp } from 'lucide-react';

import Card from '../../ui/Card/Card';
import { Button } from '../../ui/button';
import { Icon } from '../../ui/Icon';
import { ConfirmDialog } from '../../ConfirmDialog';
import { RoleButton } from './RoleButton';
import { ROLES } from './roleConfig';
import { useModelRoles } from './ModelRolesContext';
import { useDownloadedModels } from '../../../hooks/useDownloadedModels';
import { VaultAPI } from '../../../lib/api';
import { toast } from '../../../stores/toastStore';

import type { DownloadedModel } from '../../../types/downloadedModels';

function formatFileSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return `${mb.toFixed(2)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

function formatDate(iso: string | null): string {
  if (!iso) return 'Never';
  try {
    return formatDistanceToNow(new Date(iso), { addSuffix: true });
  } catch {
    return 'Unknown';
  }
}

interface Props {
  model: DownloadedModel;
}

export function LocalModelCard({ model }: Props) {
  const { refresh } = useModelRoles();
  const { deleteDownloadedModel } = useDownloadedModels();
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [isWarming, setIsWarming] = useState(false);

  // Warm up whichever roles this model is currently active for. Each
  // role has its own LLM cache on the backend (chat / utility), so a
  // single model can be active in two slots and needs both warmed.
  // Run sequentially -- a single GGUF can't load twice in parallel
  // without thrashing memory.
  const handleWarmUp = async () => {
    setIsWarming(true);
    try {
      const targets: Array<{ label: string; call: () => Promise<{ ok: boolean; error?: string }> }> = [];
      if (model.is_active_for_chat) {
        targets.push({ label: 'chat', call: VaultAPI.warmUpActiveChatModel });
      }
      if (model.is_active_for_utility) {
        targets.push({ label: 'utility', call: VaultAPI.warmUpActiveUtilityModel });
      }
      if (targets.length === 0) {
        toast.error('No active role to warm up', {
          message: 'Assign this model to chat or utility first.',
        });
        return;
      }
      for (const target of targets) {
        const result = await target.call();
        if (!result.ok) throw new Error(`${target.label}: ${result.error}`);
      }
      const roleSummary = targets.map((t) => t.label).join(' + ');
      toast.success(`${model.model_name} warmed up (${roleSummary})`, {
        message: 'Loaded into memory and ready for fast first response.',
      });
    } catch (error) {
      toast.error('Failed to warm up model', {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsWarming(false);
    }
  };

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await deleteDownloadedModel(model.id, true);
      toast.success(`${model.model_name} deleted`);
      await refresh();
    } catch (error) {
      toast.error('Failed to delete model', {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsDeleting(false);
      setShowDeleteConfirm(false);
    }
  };

  return (
    <>
      <Card padding="md" className="h-full">
        <div className="flex flex-col h-full">
          <div className="flex items-start justify-between gap-2 mb-3">
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                <span className="inline-flex items-center px-1.5 py-0.5 text-[10px] font-medium rounded bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-secondary))] border border-[hsl(var(--border-subtle))] uppercase tracking-wide">
                  Local
                </span>
              </div>
              <h3 className="font-semibold text-sm text-[hsl(var(--text-primary))] truncate mt-1">
                {model.model_name}
              </h3>
              <p className="text-xs text-[hsl(var(--text-tertiary))] mt-0.5 truncate">
                {model.model_id}
              </p>
            </div>
          </div>

          <div className="flex-1 space-y-1.5 mb-3 text-xs text-[hsl(var(--text-secondary))]">
            <div className="flex items-center gap-2">
              <Icon as={HardDrive} size={12} />
              <span className="tabular-nums">{formatFileSize(model.file_size_bytes)}</span>
            </div>
            <div className="flex items-center gap-2">
              <Icon as={Calendar} size={12} />
              <span className="tabular-nums">Downloaded {formatDate(model.downloaded_at)}</span>
            </div>
            {model.last_used_at && (
              <div className="flex items-center gap-2">
                <Icon as={TrendingUp} size={12} />
                <span className="tabular-nums">Last used {formatDate(model.last_used_at)}</span>
              </div>
            )}
          </div>

          <div className="flex flex-col gap-2 pt-3 border-t border-[hsl(var(--border-subtle))]">
            <div className="flex flex-wrap gap-1.5">
              {ROLES.map((role) => (
                <RoleButton key={role.id} model={model} role={role} />
              ))}
            </div>
            <div className="flex items-center justify-end gap-1">
              {(model.is_active_for_chat || model.is_active_for_utility) && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleWarmUp}
                  disabled={isWarming}
                  className="text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] text-xs gap-1"
                  title="Pre-load into memory for fast first response"
                >
                  <Icon as={Flame} size={12} />
                  {isWarming ? 'Warming…' : 'Warm up'}
                </Button>
              )}
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowDeleteConfirm(true)}
                disabled={isDeleting}
                className="text-[hsl(var(--danger-fg))] hover:bg-[hsl(var(--danger-muted))] text-xs"
                title="Delete this model"
              >
                <Icon as={Trash2} size={12} />
              </Button>
            </div>
          </div>
        </div>
      </Card>

      <ConfirmDialog
        isOpen={showDeleteConfirm}
        title="Delete model?"
        message={`Remove ${model.model_name} and its files? This cannot be undone.`}
        confirmLabel="Delete"
        variant="danger"
        onConfirm={handleDelete}
        onCancel={() => setShowDeleteConfirm(false)}
      />
    </>
  );
}
