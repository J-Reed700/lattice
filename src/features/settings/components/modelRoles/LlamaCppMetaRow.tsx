/**
 * LlamaCppMetaRow
 *
 * The llama.cpp connection lives in LLM settings rather than the downloaded
 * models table. Selecting Chat updates the provider; selecting Utility,
 * Learning, or Verifier saves the served model reference in Settings.
 * Active roles are shown as compact badges on the row, while role assignment
 * lives in the model configuration menu.
 */

import { useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { Check, ChevronLeft, Network, Plus, SlidersHorizontal } from 'lucide-react';

import { IconButton } from '@/components/ui';
import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';
import { LLAMACPP_SERVER_MODEL_ID, modelRefForOption, sameModelRef } from '@/utils/chatModelSelection';

import { LearningWindowNotice } from './LearningWindowNotice';
import { useModelRoles } from './ModelRolesContext';
import { ActiveRoleBadge } from './RoleButton';
import { ModelVisionDeclaration } from '../AITab/VisionDeclarationEditor';

import type { RoleId } from './roleConfig';

const TOGGLE_CLASS =
  'inline-flex h-7 items-center gap-1 rounded-md border px-2 text-xs transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring';
const ON_CLASS = 'border-accent/25 bg-accent-muted font-medium text-accent';
const OFF_CLASS = 'border-border-default text-text-secondary hover:bg-surface-raised hover:text-text-primary';

export function LlamaCppMetaRow() {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings, isPending } = useUpdateSettingsMutation();
  const { clearActiveUtilityModel } = useDownloadedModels();
  const { refresh } = useModelRoles();
  const [showConfigModal, setShowConfigModal] = useState(false);

  const connection = settings?.llm.llamaCpp;
  const url = connection?.url.trim() ?? '';
  const model = connection?.model.trim() ?? '';
  const isActive = settings?.llm.provider === 'llamacpp';
  const isConfigured = Boolean(url && model);
  const servedRef = settings && isConfigured
    ? modelRefForOption({ id: LLAMACPP_SERVER_MODEL_ID, backend: 'llamacpp' }, settings.llm)
    : null;
  const isUtility = sameModelRef(settings?.llm.utilityModel, servedRef);
  const isLearning = sameModelRef(settings?.llm.learningModel, servedRef);
  const isVerifier = sameModelRef(settings?.llm.verifierModel, servedRef);

  const activeBadges: Array<{ id: RoleId; label: string }> = [
    ...(isActive && isConfigured ? [{ id: 'chat' as const, label: 'Chat' }] : []),
    ...(isUtility ? [{ id: 'utility' as const, label: 'Utility' }] : []),
    ...(isLearning ? [{ id: 'learning' as const, label: 'Learning' }] : []),
    ...(isVerifier ? [{ id: 'verifier' as const, label: 'Verifier' }] : []),
  ];

  const selectForChat = async () => {
    if (!isConfigured || isActive || isPending) return;
    try {
      await updateSettings({ category: 'llm', updates: { provider: 'llamacpp' } });
    } catch (error) {
      toast.error('Failed to set chat role', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const toggleUtility = async () => {
    if (!servedRef || isPending) return;
    try {
      if (!isUtility) {
        await clearActiveUtilityModel();
      }
      await updateSettings({ category: 'llm', updates: { utilityModel: isUtility ? null : servedRef } });
      await refresh();
    } catch (error) {
      toast.error('Failed to set utility role', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const toggleLearning = async () => {
    if (!servedRef || isPending) return;
    try {
      await updateSettings({ category: 'llm', updates: { learningModel: isLearning ? null : servedRef } });
    } catch (error) {
      toast.error('Failed to set learning role', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const toggleVerifier = async () => {
    if (!servedRef || isPending) return;
    try {
      await updateSettings({ category: 'llm', updates: { verifierModel: isVerifier ? null : servedRef } });
    } catch (error) {
      toast.error('Failed to set verifier role', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const goToChatSettings = () => {
    window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'chat' } }));
  };

  const meta = model ? `Model · ${model}` : 'No model selected';

  return (
    <div className="border-b border-border-subtle py-4">
      <div className="connected-model-row group flex items-center gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-raised text-accent"><Network className="h-4 w-4" aria-hidden="true" /></span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-text-primary">llama.cpp connection</div>
          {url ? <div className="truncate font-mono text-xs text-text-muted">{url}</div> : null}
          <p className="mt-1 wrap-break-word text-xs text-text-muted">{meta}</p>
          {settings?.llm.provider === 'auto' && isConfigured ? (
            <p className="mt-1 text-xs text-text-muted">
              Auto tries local models first. Select Chat to always use this connection.
            </p>
          ) : null}
          {isLearning ? <LearningWindowNotice model={servedRef} savedWindow={null} /> : null}
        </div>

        <div className="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
          {activeBadges.map((badge) => (
            <ActiveRoleBadge key={badge.id} roleId={badge.id} label={badge.label} />
          ))}

          <div className={cn('flex items-center gap-1.5', activeBadges.length > 0 && 'ml-1 border-l border-border-subtle pl-1.5')}>
            <IconButton
              label="Configure model"
              onClick={() => setShowConfigModal(true)}
            >
              <SlidersHorizontal />
            </IconButton>
            <button
              type="button"
              onClick={goToChatSettings}
              className="shrink-0 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
            >
              Edit connection
            </button>
          </div>
        </div>
      </div>
      {servedRef ? (
        <div className="mt-3 pl-12">
          <ModelVisionDeclaration modelRef={servedRef} />
        </div>
      ) : null}

      {showConfigModal ? (
        <Dialog.Root open={showConfigModal} onOpenChange={(open) => !open && setShowConfigModal(false)}>
          <Dialog.Portal>
            <Dialog.Overlay className="fixed inset-0 z-50 bg-black/60 backdrop-blur-[2px]" />
            <Dialog.Content
              aria-describedby={undefined}
              className="fixed left-1/2 top-1/2 z-50 w-full max-w-[520px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border-subtle bg-surface-raised shadow-2xl outline-none"
            >
              <div className="relative flex items-center justify-center border-b border-border-subtle px-5 py-3.5">
                <button
                  type="button"
                  onClick={() => setShowConfigModal(false)}
                  aria-label="Close"
                  className="absolute left-4 flex h-7 w-7 items-center justify-center rounded-md text-text-secondary transition-colors hover:bg-surface hover:text-text-primary"
                >
                  <ChevronLeft className="h-4 w-4" />
                </button>
                <Dialog.Title className="truncate text-sm font-semibold text-text-primary">
                  llama.cpp connection
                </Dialog.Title>
              </div>

              <div className="space-y-5 px-6 py-5">
                <div>
                  <div className="mb-1.5 text-xs font-medium text-text-secondary">Connected model</div>
                  <div className="flex flex-col gap-0.5 rounded-lg border border-border-subtle bg-surface px-3.5 py-2.5">
                    <span className="truncate text-sm font-medium text-text-primary">
                      {model || 'No model selected'}
                    </span>
                    {url ? <span className="truncate font-mono text-xs text-text-muted">{url}</span> : null}
                  </div>
                </div>

                <div>
                  <div className="mb-1.5 text-xs font-medium text-text-secondary">Roles</div>
                  <div className="flex flex-wrap items-center gap-1.5">
                    <button
                      type="button"
                      onClick={() => void selectForChat()}
                      disabled={!isConfigured || isActive || isPending}
                      aria-pressed={isActive && isConfigured}
                      data-role="chat"
                      title={!isConfigured ? 'Set a server URL and model in connection settings first'
                        : isActive ? 'Selected for chat' : 'Use this llama.cpp connection for chat'}
                      className={cn(
                        TOGGLE_CLASS,
                        isActive && isConfigured ? ON_CLASS : OFF_CLASS,
                        (!isConfigured || isPending) && 'cursor-not-allowed opacity-60',
                      )}
                    >
                      {isActive && isConfigured ? <Check className="h-3 w-3" aria-hidden="true" /> : <Plus className="h-3 w-3" aria-hidden="true" />}
                      {isPending ? 'Setting…' : 'Chat'}
                    </button>
                    <button
                      type="button"
                      onClick={() => void toggleUtility()}
                      disabled={!servedRef || isPending}
                      aria-pressed={isUtility}
                      data-role="utility"
                      title={!isConfigured ? 'Set a server URL and model in connection settings first'
                        : isUtility ? 'Active utility model. Click to stop using it.'
                          : 'Fast model for query expansion, routing, and grounding checks.'}
                      className={cn(
                        TOGGLE_CLASS,
                        isUtility ? ON_CLASS : OFF_CLASS,
                        (!servedRef || isPending) && 'cursor-not-allowed opacity-60',
                      )}
                    >
                      {isUtility ? <Check className="h-3 w-3" aria-hidden="true" /> : <Plus className="h-3 w-3" aria-hidden="true" />}
                      Utility
                    </button>
                    <button
                      type="button"
                      onClick={() => void toggleLearning()}
                      disabled={!servedRef || isPending}
                      aria-pressed={isLearning}
                      data-role="learning"
                      title={!isConfigured ? 'Set a server URL and model in connection settings first'
                        : isLearning ? 'Active learning model. Click to stop using it.'
                          : 'Prepares lessons, outlines, practice and study decks. Needs a large context window.'}
                      className={cn(
                        TOGGLE_CLASS,
                        isLearning ? ON_CLASS : OFF_CLASS,
                        (!servedRef || isPending) && 'cursor-not-allowed opacity-60',
                      )}
                    >
                      {isLearning ? <Check className="h-3 w-3" aria-hidden="true" /> : <Plus className="h-3 w-3" aria-hidden="true" />}
                      Learning
                    </button>
                    <button
                      type="button"
                      onClick={() => void toggleVerifier()}
                      disabled={!servedRef || isPending}
                      aria-pressed={isVerifier}
                      data-role="verifier"
                      title={!isConfigured ? 'Set a server URL and model in connection settings first'
                        : isVerifier ? 'Active verifier model. Click to stop using it.'
                          : 'Audits every claim of a lesson against its cited passages before it publishes.'}
                      className={cn(
                        TOGGLE_CLASS,
                        isVerifier ? ON_CLASS : OFF_CLASS,
                        (!servedRef || isPending) && 'cursor-not-allowed opacity-60',
                      )}
                    >
                      {isVerifier ? <Check className="h-3 w-3" aria-hidden="true" /> : <Plus className="h-3 w-3" aria-hidden="true" />}
                      Verifier
                    </button>
                  </div>
                  <p className="mt-1.5 text-[11px] text-text-muted">
                    Choose which tasks this connection powers. Role changes take effect immediately.
                  </p>
                </div>
              </div>

              <div className="flex items-center justify-end gap-2 border-t border-border-subtle px-6 py-4">
                <button
                  type="button"
                  onClick={() => setShowConfigModal(false)}
                  className="rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-white shadow-xs transition-opacity hover:opacity-90"
                >
                  Done
                </button>
              </div>
            </Dialog.Content>
          </Dialog.Portal>
        </Dialog.Root>
      ) : null}
    </div>
  );
}
