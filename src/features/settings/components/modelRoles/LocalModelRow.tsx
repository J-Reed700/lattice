import { useEffect, useState } from 'react';

import { formatDistanceToNow } from 'date-fns';
import { Box, Flame, SlidersHorizontal, Trash2 } from 'lucide-react';

import { ConfirmDialog } from '@/components/ConfirmDialog';
import { IconButton } from '@/components/ui';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { degradedReasonFor, useLoadedModels } from '@/features/model/hooks/useLoadedModels';
import { useModelWarmupStore } from '@/features/model/stores/modelWarmupStore';
import type { RoleWarmupState, WarmupRole } from '@/features/model/stores/modelWarmupStore';
import { toast } from '@/stores/toastStore';
import type { DownloadedModel } from '@/types/downloadedModels';
import { modelRefForRow, tokensThatFit } from '@/utils/chatModelSelection';

import { LearningWindowNotice } from './LearningWindowNotice';
import { LocalModelConfigModal } from './LocalModelConfigModal';
import { LocalVisionControl } from './LocalVisionControl';
import { useModelRoles } from './ModelRolesContext';
import { ActiveRoleBadge } from './RoleButton';
import { canModelFulfillRole, isChatModelSelected, isLearningModel, isUtilityModel, ROLES } from './roleConfig';

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
  const { refresh, chatProvider, llmSettings } = useModelRoles();
  const chatActive = isChatModelSelected(model, chatProvider);
  const utilityActive = isUtilityModel(model, llmSettings);
  const learningActive = isLearningModel(model, llmSettings);
  const roles = ROLES.filter((role) => canModelFulfillRole(model, role));
  const activeRoles = roles.filter((role) => role.isActive(model, llmSettings));
  const { deleteDownloadedModel, warmUpActiveChatModel, warmUpActiveUtilityModel, warmUpLearningModel } =
    useDownloadedModels();
  const loadedModels = useLoadedModels();
  const degraded = degradedReasonFor(modelRefForRow(model, llmSettings), loadedModels);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [showConfigModal, setShowConfigModal] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [isWarming, setIsWarming] = useState(false);
  const chatWarmup = useModelWarmupStore((state) => state.chat);
  const utilityWarmup = useModelWarmupStore((state) => state.utility);
  const learningWarmup = useModelWarmupStore((state) => state.learning);
  const setRolePhase = useModelWarmupStore((state) => state.setRolePhase);
  const claimRole = useModelWarmupStore((state) => state.claimRole);

  // Stamp this model onto the roles it holds, which drops state left by
  // whichever model held them before. A role's failure belongs to the model
  // that failed, not to the next one assigned to the slot.
  useEffect(() => {
    if (chatActive) claimRole('chat', model.id);
    if (utilityActive) claimRole('utility', model.id);
    if (learningActive) claimRole('learning', model.id);
  }, [claimRole, model.id, chatActive, utilityActive, learningActive]);

  // The backend's own reason this model didn't load for a role it holds
  // (boot prewarm or the warm-up button). Chat first: it blocks more.
  const failedFor = (role: RoleWarmupState) =>
    role.phase === 'failed' && (role.modelId === null || role.modelId === model.id)
      ? role.error
      : null;
  const loadError =
    (chatActive ? failedFor(chatWarmup) : null) ??
    (utilityActive ? failedFor(utilityWarmup) : null) ??
    (learningActive ? failedFor(learningWarmup) : null);
  const isReady = [
    chatActive ? chatWarmup : null,
    utilityActive ? utilityWarmup : null,
    learningActive ? learningWarmup : null,
  ].some((role) => role?.phase === 'ready' && role.modelId === model.id);

  // Warm up whichever roles this model is currently active for. Roles that
  // share this model and its launch settings share one loaded instance, so
  // the later ones find it ready. Run sequentially -- a single GGUF can't
  // load twice in parallel without thrashing memory. A failure reports and
  // changes no assignment.
  const handleWarmUp = async () => {
    setIsWarming(true);
    try {
      const targets: Array<{ role: WarmupRole; call: () => Promise<void> }> = [];
      if (chatActive) {
        targets.push({ role: 'chat', call: warmUpActiveChatModel });
      }
      if (utilityActive) {
        targets.push({ role: 'utility', call: warmUpActiveUtilityModel });
      }
      if (learningActive) {
        targets.push({ role: 'learning', call: warmUpLearningModel });
      }
      if (targets.length === 0) {
        toast.error('Nothing to warm up', {
          message: 'Assign this model to chat, utility or learning first.',
        });
        return;
      }
      for (const target of targets) {
        setRolePhase(target.role, 'started', null, model.id);
        try {
          await target.call();
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          setRolePhase(target.role, 'failed', message, model.id);
          throw new Error(`${target.role}: ${message}`);
        }
        setRolePhase(target.role, 'ready', null, model.id);
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
  const maxTokens = model.trained_context_length ?? null;
  // The window it loads with: the user's, else its maximum.
  const window = model.configured_context_length ?? llmSettings?.localContextWindow ?? maxTokens;
  const fits = window
    ? tokensThatFit(
        model,
        window,
        llmSettings?.localModelConfigs?.[model.model_id]?.kvCacheQuantization ?? false,
      )
    : null;
  const meta = [
    formatFileSize(model.file_size_bytes),
    maxTokens ? `Max ${maxTokens.toLocaleString('en-US')} tokens` : null,
    downloaded ? `downloaded ${downloaded}` : null,
    used ? `used ${used}` : null,
  ]
    .filter(Boolean)
    .join(' · ');

  const canWarmUp = chatActive || utilityActive || learningActive;

  return (
    <>
      <div className="downloaded-model-row group flex flex-wrap items-center gap-3 border-b border-border-subtle py-4">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-raised text-accent"><Box className="h-[18px] w-[18px]" aria-hidden="true" /></span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-text-primary">{model.model_name}</div>
          <div className="truncate font-mono text-xs text-text-muted">{model.model_id}</div>
          <div className="mt-1.5 text-xs leading-relaxed tabular-nums text-text-muted">{meta}</div>
          {window && fits != null ? (
            <div role="status" className="mt-0.5 text-xs text-warning-fg">
              Loads at {window.toLocaleString('en-US')} tokens, which may not fit in this
              machine&apos;s memory — about {fits.toLocaleString('en-US')} tokens fit.
            </div>
          ) : null}
          {model.vision ? <LocalVisionControl model={model} vision={model.vision} /> : null}
          {loadError ? (
            <div
              role="alert"
              title={loadError}
              className="mt-0.5 line-clamp-3 wrap-break-word text-xs text-danger-fg"
            >
              Didn&apos;t load: {loadError.split('\n')[0]}
            </div>
          ) : degraded ? (
            // Loaded, but on the CPU: it works, slowly, and says why.
            <div role="status" title={degraded} className="mt-0.5 line-clamp-3 wrap-break-word text-xs text-text-secondary">
              {isReady ? 'Ready' : 'Loaded'} (on CPU: {degraded})
            </div>
          ) : null}
          {learningActive ? (
            <LearningWindowNotice
              model={modelRefForRow(model, llmSettings)}
              savedWindow={model.configured_context_length ?? null}
            />
          ) : null}
        </div>

        <div className="downloaded-model-actions flex shrink-0 flex-wrap items-center gap-1.5">
          {activeRoles.map((role) => (
            <ActiveRoleBadge key={role.id} roleId={role.id} label={role.label} />
          ))}

          <div className={activeRoles.length > 0 ? 'ml-1 flex items-center gap-0.5 border-l border-border-subtle pl-1.5' : 'flex items-center gap-0.5'}>
            <IconButton
              label="Configure model"
              data-roles={roles.map((role) => role.id).join(' ')}
              onClick={() => setShowConfigModal(true)}
            >
              <SlidersHorizontal />
            </IconButton>
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

      {showConfigModal ? (
        <LocalModelConfigModal
          isOpen={showConfigModal}
          model={model}
          onClose={() => setShowConfigModal(false)}
          onLoadModel={canWarmUp ? handleWarmUp : undefined}
        />
      ) : null}

      <ConfirmDialog
        isOpen={showDeleteConfirm}
        title="Delete model?"
        message={
          learningActive
            ? `Remove ${model.model_name} and its files? This cannot be undone. This is your Learning model. Lessons will stop until you choose another.`
            : `Remove ${model.model_name} and its files? This cannot be undone.`
        }
        confirmLabel="Delete"
        variant="danger"
        onConfirm={handleDelete}
        onCancel={() => setShowDeleteConfirm(false)}
      />
    </>
  );
}
