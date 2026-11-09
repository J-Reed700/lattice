import { useEffect, useState } from 'react';

import * as Dialog from '@radix-ui/react-dialog';
import { ChevronLeft } from 'lucide-react';

import { Switch } from '@/components/ui';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { JsonValue, LocalModelConfigDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import type { DownloadedModel } from '@/types/downloadedModels';
import { contextFitWarning, fittedLaunchWindow } from '@/utils/chatModelSelection';

import { RoleButton } from './RoleButton';
import { ROLES } from './roleConfig';
import { HelpTooltip, MIN_CONTEXT_LENGTH, TokenLimitField } from './TokenLimitField';
import { NUMBER_FIELD_CLASS, SWITCH_CLASS, clamp, toFinite } from '../AITab/shared';

const DEFAULT_MAX_CONTEXT_LENGTH = 262_144;
const DEFAULT_CONCURRENT_PREDICTIONS = 4;

function extractQuantizationTag(model: DownloadedModel): string {
  const source = `${model.file_path} ${model.model_id} ${model.model_name}`;
  const match = source.match(/\b(Q[2-8]_[A-Z0-9_]+|IQ[1-4]_[A-Z0-9]+|F16|BF16|F32|[48]bit)\b/i);
  return match ? match[1].toUpperCase() : '4bit';
}

interface LocalModelConfigModalProps {
  isOpen: boolean;
  model: DownloadedModel;
  onClose: () => void;
  onLoadModel?: () => Promise<void>;
}

export function LocalModelConfigModal({
  isOpen,
  model,
  onClose,
  onLoadModel,
}: LocalModelConfigModalProps) {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  const isEmbeddingModel = model.model_type === 'embedding' || model.model_type === 'text_embeddings';

  const maxSupportedTokens =
    model.trained_context_length && model.trained_context_length >= MIN_CONTEXT_LENGTH
      ? model.trained_context_length
      : DEFAULT_MAX_CONTEXT_LENGTH;

  const savedConfig: LocalModelConfigDto | undefined =
    settings?.llm?.localModelConfigs?.[model.model_id];

  // Shows the window in force: saved for this model, else for every local
  // model, else the model's maximum, which the window fitted to this machine
  // stands in for until the user edits it.
  const [contextLength, setContextLength] = useState<number>(maxSupportedTokens);
  const [contextEdited, setContextEdited] = useState<boolean>(false);
  const [maxConcurrentPredictions, setMaxConcurrentPredictions] = useState<number>(
    DEFAULT_CONCURRENT_PREDICTIONS,
  );
  const [useFixedSeed, setUseFixedSeed] = useState<boolean>(false);
  const [seed, setSeed] = useState<number>(42);
  const [kvCacheQuantization, setKvCacheQuantization] = useState<boolean>(false);
  const [rememberSettings, setRememberSettings] = useState<boolean>(true);
  const [showAdvanced, setShowAdvanced] = useState<boolean>(true);
  const [isSaving, setIsSaving] = useState<boolean>(false);

  useEffect(() => {
    if (!isOpen) return;
    const globalLocalCtx = settings?.llm?.localContextWindow ?? null;
    setContextEdited(false);
    if (savedConfig) {
      setContextLength(savedConfig.contextLength ?? globalLocalCtx ?? maxSupportedTokens);
      setMaxConcurrentPredictions(
        savedConfig.maxConcurrentPredictions ?? DEFAULT_CONCURRENT_PREDICTIONS,
      );
      setUseFixedSeed(savedConfig.seed !== null);
      setSeed(savedConfig.seed ?? 42);
      setKvCacheQuantization(savedConfig.kvCacheQuantization ?? false);
      setRememberSettings(true);
    } else {
      setContextLength(globalLocalCtx ?? maxSupportedTokens);
      setMaxConcurrentPredictions(DEFAULT_CONCURRENT_PREDICTIONS);
      setUseFixedSeed(false);
      setSeed(42);
      setKvCacheQuantization(false);
      setRememberSettings(true);
    }
  }, [isOpen, savedConfig, settings?.llm?.localContextWindow, maxSupportedTokens]);

  const quantTag = extractQuantizationTag(model);
  const savedWindow = savedConfig?.contextLength ?? settings?.llm?.localContextWindow ?? null;
  const fittedWindow =
    !contextEdited && savedWindow === null
      ? fittedLaunchWindow(model, kvCacheQuantization)
      : null;
  const fitWarning =
    fittedWindow === null ? contextFitWarning(model, contextLength, kvCacheQuantization) : null;

  const handleSaveAndLoad = async () => {
    if (!settings?.llm) return;
    setIsSaving(true);
    try {
      // A window the user did not touch stays as it was saved: none saved
      // keeps the window fitted to this machine.
      const editedContext = contextEdited
        ? Math.round(clamp(contextLength, MIN_CONTEXT_LENGTH, maxSupportedTokens))
        : null;
      const modelContext = editedContext ?? savedConfig?.contextLength ?? null;
      const globalContext = editedContext ?? settings.llm.localContextWindow ?? null;
      const resolvedSlots = Math.round(clamp(maxConcurrentPredictions, 1, 8));
      const resolvedSeed = useFixedSeed ? Math.max(0, Math.round(seed)) : null;

      const nextConfigs = { ...(settings.llm.localModelConfigs ?? {}) };
      if (rememberSettings) {
        nextConfigs[model.model_id] = {
          // Kept as saved: vision is switched on the model row, not here.
          ...savedConfig,
          contextLength: modelContext,
          maxConcurrentPredictions:
            resolvedSlots === DEFAULT_CONCURRENT_PREDICTIONS ? null : resolvedSlots,
          seed: resolvedSeed,
          kvCacheQuantization,
        };
      } else {
        delete nextConfigs[model.model_id];
      }

      await updateSettings({
        category: 'llm',
        updates: {
          localModelConfigs: nextConfigs,
          ...(!rememberSettings ? { localContextWindow: globalContext } : {}),
        } as unknown as Record<string, JsonValue>,
      });

      onClose();
      if (onLoadModel) {
        await onLoadModel();
      } else {
        toast.success(`Saved settings for ${model.model_name}`);
      }
    } catch (error) {
      toast.error('Failed to save model settings', {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog.Root open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/60 backdrop-blur-[2px]" />
        <Dialog.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-1/2 z-50 w-full max-w-[640px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border-subtle bg-surface-raised shadow-2xl outline-none"
        >
          {/* Header */}
          <div className="relative flex items-center justify-center border-b border-border-subtle px-5 py-3.5">
            <button
              type="button"
              onClick={onClose}
              aria-label="Close"
              className="absolute left-4 flex h-7 w-7 items-center justify-center rounded-md text-text-secondary transition-colors hover:bg-surface hover:text-text-primary"
            >
              <ChevronLeft className="h-4 w-4" />
            </button>
            <Dialog.Title className="truncate text-sm font-semibold text-text-primary">
              {model.model_name}
            </Dialog.Title>
          </div>

          <div className="space-y-5 px-6 py-5">
            {/* Model file */}
            <div>
              <div className="mb-1.5 text-xs font-medium text-text-secondary">Model file</div>
              <div className="flex items-center gap-2 rounded-lg border border-border-subtle bg-surface px-3.5 py-2.5">
                <span className="truncate text-sm font-medium text-text-primary">
                  {model.model_name}
                </span>
                <span className="rounded bg-surface-raised px-1.5 py-0.5 font-mono text-[11px] text-text-secondary">
                  {quantTag}
                </span>
                <span className="rounded bg-surface-raised px-1.5 py-0.5 font-mono text-[11px] uppercase text-text-secondary">
                  GGUF
                </span>
              </div>
            </div>

            {/* Roles */}
            <div>
              <div className="mb-1.5 text-xs font-medium text-text-secondary">Roles</div>
              <div className="flex flex-wrap items-center gap-1.5">
                {ROLES.map((role) => (
                  <RoleButton key={role.id} model={model} role={role} />
                ))}
              </div>
              <p className="mt-1.5 text-[11px] text-text-muted">
                Choose which tasks this model powers. Role changes take effect immediately.
              </p>
            </div>

            {/* Context Length */}
            {!isEmbeddingModel ? (
            <>
              <TokenLimitField
                label="Context Length"
                help="Tokens the model keeps in its active context window. The model loads with the window you set here. Until you set one, it loads with the largest window that fits this machine's memory now, up to its maximum. A window you set that is larger than fits fails to load and says how much it needed."
                min={MIN_CONTEXT_LENGTH}
                max={maxSupportedTokens}
                value={fittedWindow ?? contextLength}
                onChange={(value) => {
                  setContextEdited(true);
                  setContextLength(value);
                }}
              >
                {fittedWindow !== null ? (
                  <p role="status" className="mt-1.5 text-[11px] text-text-muted">
                    {`Loads at ${fittedWindow.toLocaleString('en-US')} tokens, the largest window that fits this machine's memory now. The model was trained for up to ${maxSupportedTokens.toLocaleString('en-US')}.`}
                  </p>
                ) : null}
                {fitWarning ? (
                  <p role="status" className="mt-1.5 text-[11px] text-warning-fg">
                    {fitWarning}
                  </p>
                ) : null}
              </TokenLimitField>
            </>
            ) : null}

            {/* Advanced settings */}
            {!isEmbeddingModel && showAdvanced ? (
              <div className="space-y-4 border-t border-border-subtle pt-4">
                {/* Max Concurrent Predictions */}
                <div className="flex items-center justify-between gap-3">
                  <div className="flex items-center gap-1.5 text-xs font-medium text-text-primary">
                    <span>Max Concurrent Predictions</span>
                    <HelpTooltip content="Parallel slots (-np) sharing the unified KV cache on llama-server." />
                    <span className="rounded-full bg-surface px-2 py-0.5 text-[10px] font-normal text-text-muted">
                      Experimental
                    </span>
                  </div>
                  <input
                    aria-label="Max Concurrent Predictions"
                    type="number"
                    min={1}
                    max={8}
                    step={1}
                    value={maxConcurrentPredictions}
                    onChange={(event) =>
                      setMaxConcurrentPredictions(
                        Math.round(
                          clamp(toFinite(event.target.value, maxConcurrentPredictions), 1, 8),
                        ),
                      )
                    }
                    className={NUMBER_FIELD_CLASS}
                  />
                </div>

                {/* Seed */}
                <div className="flex items-center justify-between gap-3">
                  <div className="flex items-center gap-2 text-xs font-medium text-text-primary">
                    <span>Seed</span>
                    <HelpTooltip content="Fix the random number generator seed for reproducible sampling." />
                    <input
                      aria-label="Use Fixed Seed"
                      type="checkbox"
                      checked={useFixedSeed}
                      onChange={(event) => setUseFixedSeed(event.target.checked)}
                      className="h-3.5 w-3.5 rounded border-border-subtle"
                    />
                  </div>
                  {useFixedSeed ? (
                    <input
                      aria-label="Seed Value"
                      type="number"
                      min={0}
                      step={1}
                      value={seed}
                      onChange={(event) =>
                        setSeed(Math.max(0, Math.round(toFinite(event.target.value, seed))))
                      }
                      className={NUMBER_FIELD_CLASS}
                    />
                  ) : (
                    <span className="text-xs text-text-muted">Random Seed</span>
                  )}
                </div>

                {/* KV Cache Quantization */}
                <div className="flex items-center justify-between gap-3">
                  <div className="flex items-center gap-1.5 text-xs font-medium text-text-primary">
                    <span>KV Cache Quantization</span>
                    <HelpTooltip content="Quantize the K and V caches to q8_0 (-ctk q8_0 -ctv q8_0), cutting KV cache memory in half so larger contexts fit in GPU/unified memory." />
                    <span className="rounded-full bg-surface px-2 py-0.5 text-[10px] font-normal text-text-muted">
                      Experimental
                    </span>
                  </div>
                  <Switch
                    aria-label="KV Cache Quantization"
                    checked={kvCacheQuantization}
                    onCheckedChange={setKvCacheQuantization}
                    className={SWITCH_CLASS}
                  />
                </div>
              </div>
            ) : null}
          </div>

          {/* Footer */}
          <div className="flex flex-wrap items-center justify-between gap-4 border-t border-border-subtle px-6 py-4">
            {!isEmbeddingModel ? (
              <div className="space-y-2">
                <label className="flex items-center gap-2 text-xs text-text-secondary">
                  <input
                    type="checkbox"
                    checked={rememberSettings}
                    onChange={(event) => setRememberSettings(event.target.checked)}
                    className="h-3.5 w-3.5 rounded border-border-subtle"
                  />
                  <span>Remember settings for</span>
                  <code className="rounded bg-surface px-1.5 py-0.5 font-mono text-[11px] text-text-primary">
                    {model.model_id}
                  </code>
                </label>

                <div className="flex items-center gap-2 text-xs text-text-secondary">
                  <Switch
                    aria-label="Show advanced settings"
                    checked={showAdvanced}
                    onCheckedChange={setShowAdvanced}
                    className={SWITCH_CLASS}
                  />
                  <span>Show advanced settings</span>
                </div>
              </div>
            ) : <div />}

            <div className="flex items-center gap-2">
              {isEmbeddingModel ? (
                <button
                  type="button"
                  onClick={onClose}
                  className="rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-white shadow-xs transition-opacity hover:opacity-90"
                >
                  Done
                </button>
              ) : (
                <>
                  <button
                    type="button"
                    onClick={onClose}
                    className="rounded-lg border border-border-subtle bg-surface px-3.5 py-2 text-xs font-medium text-text-secondary transition-colors hover:bg-surface-raised hover:text-text-primary"
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    disabled={isSaving}
                    onClick={() => void handleSaveAndLoad()}
                    className="rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-white shadow-xs transition-opacity hover:opacity-90 disabled:opacity-50"
                  >
                    {isSaving ? (onLoadModel ? 'Loading…' : 'Saving…') : (onLoadModel ? 'Load Model' : 'Save')}
                  </button>
                </>
              )}
            </div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
