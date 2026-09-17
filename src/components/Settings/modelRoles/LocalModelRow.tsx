/**
 * LocalModelRow
 *
 * One hairline row per downloaded model. Name and id on the left, a single
 * meta line in the middle, role toggles plus hover actions on the right.
 * The role toggles are generated from {@link ROLES} so adding a fourth role
 * doesn't require touching this component.
 *
 * Ollama-backed models are NOT rendered here — they live in
 * {@link OllamaMetaRow}.
 */

import { useEffect, useState } from 'react';

import { formatDistanceToNow } from 'date-fns';
import { Flame, Trash2 } from 'lucide-react';

import { useModelRoles } from './ModelRolesContext';
import { RoleButton } from './RoleButton';
import { ROLES } from './roleConfig';
import { useDownloadedModels } from '../../../hooks/useDownloadedModels';
import { VaultAPI } from '../../../lib/api';
import { useModelWarmupStore } from '../../../stores/modelWarmupStore';
import { toast } from '../../../stores/toastStore';
import { ConfirmDialog } from '../../ConfirmDialog';
import { IconButton } from '../../ui';

import type { RoleWarmupState } from '../../../stores/modelWarmupStore';
import type { DownloadedModel } from '../../../types/downloadedModels';

function formatFileSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return `${mb.toFixed(0)} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

function formatDate(iso: string | null): string | null {
  if (!iso) return null;
  try {
    return formatDistanceToNow(new Date(iso), { addSuffix: true });
  } catch {
    return null;
  }
}

interface Props {
  model: DownloadedModel;
}

export function LocalModelRow({ model }: Props) {
  const { refresh } = useModelRoles();
  const { deleteDownloadedModel } = useDownloadedModels();
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [isWarming, setIsWarming] = useState(false);
  const chatWarmup = useModelWarmupStore((state) => state.chat);
  const utilityWarmup = useModelWarmupStore((state) => state.utility);
  const setRolePhase = useModelWarmupStore((state) => state.setRolePhase);
  const claimRole = useModelWarmupStore((state) => state.claimRole);

  // Stamp this model onto the roles it holds, which drops state left by
  // whichever model held them before. A role's failure belongs to the model
  // that failed, not to the next one assigned to the slot.
  useEffect(() => {
    if (model.is_active_for_chat) claimRole('chat', model.id);
    if (model.is_active_for_utility) claimRole('utility', model.id);
  }, [claimRole, model.id, model.is_active_for_chat, model.is_active_for_utility]);

  // The backend's own reason this model didn't load for a role it holds
  // (boot prewarm or the warm-up button). Chat first: it blocks more.
  const failedFor = (role: RoleWarmupState) =>
    role.phase === 'failed' && (role.modelId === null || role.modelId === model.id)
      ? role.error
      : null;
  const loadError =
    (model.is_active_for_chat ? failedFor(chatWarmup) : null) ??
    (model.is_active_for_utility ? failedFor(utilityWarmup) : null);

  // Warm up whichever roles this model is currently active for. Each
  // role has its own LLM cache on the backend (chat / utility), so a
  // single model can be active in two slots and needs both warmed.
  // Run sequentially -- a single GGUF can't load twice in parallel
  // without thrashing memory.
  const handleWarmUp = async () => {
    setIsWarming(true);
    try {
      const targets: Array<{
        role: 'chat' | 'utility';
        call: () => Promise<{ ok: boolean; error?: string }>;
      }> = [];
      if (model.is_active_for_chat) {
        targets.push({ role: 'chat', call: VaultAPI.warmUpActiveChatModel });
      }
      if (model.is_active_for_utility) {
        targets.push({ role: 'utility', call: VaultAPI.warmUpActiveUtilityModel });
      }
      if (targets.length === 0) {
        toast.error('Nothing to warm up', {
          message: 'Assign this model to chat or utility first.',
        });
        return;
      }
      for (const target of targets) {
        setRolePhase(target.role, 'started', null, model.id);
        const result = await target.call();
        setRolePhase(target.role, result.ok ? 'ready' : 'failed', result.error ?? null, model.id);
        if (!result.ok) throw new Error(`${target.role}: ${result.error}`);
      }
      const roleSummary = targets.map((t) => t.role).join(' + ');
      toast.success(`${model.model_name} warmed up (${roleSummary})`, {
        message: 'Loaded into memory and ready for a fast first response.',
      });
    } catch (error) {
      toast.error("Couldn't warm up the model", {
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
      toast.error("Couldn't delete the model", {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsDeleting(false);
      setShowDeleteConfirm(false);
    }
  };

  const downloaded = formatDate(model.downloaded_at);
  const used = formatDate(model.last_used_at);
  const meta = [
    formatFileSize(model.file_size_bytes),
    downloaded ? `downloaded ${downloaded}` : null,
    used ? `used ${used}` : null,
  ]
    .filter(Boolean)
    .join(' · ');

  const canWarmUp = model.is_active_for_chat || model.is_active_for_utility;

  return (
    <>
      <div className="group flex items-center gap-4 border-b border-border-subtle py-3">
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-text-primary">{model.model_name}</div>
          <div className="truncate font-mono text-xs text-text-muted">{model.model_id}</div>
          {loadError ? (
            <div
              role="alert"
              title={loadError}
              className="mt-0.5 line-clamp-3 break-words text-xs text-danger-fg"
            >
              Didn&apos;t load: {loadError.split('\n')[0]}
            </div>
          ) : null}
        </div>

        <div className="hidden shrink-0 text-xs text-text-muted tabular-nums lg:block">{meta}</div>

        <div className="flex shrink-0 items-center gap-0.5">
          {ROLES.map((role) => (
            <RoleButton key={role.id} model={model} role={role} />
          ))}

          <div className="ml-1 flex items-center gap-0.5 opacity-0 transition-opacity duration-fast focus-within:opacity-100 group-hover:opacity-100">
            {canWarmUp ? (
              <IconButton
                label={isWarming ? 'Warming up…' : 'Warm up'}
                onClick={handleWarmUp}
                disabled={isWarming}
              >
                <Flame />
              </IconButton>
            ) : null}
            <IconButton
              label="Delete"
              onClick={() => setShowDeleteConfirm(true)}
              disabled={isDeleting}
              className="hover:text-danger-fg"
            >
              <Trash2 />
            </IconButton>
          </div>
        </div>
      </div>

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
