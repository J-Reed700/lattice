/**
 * LearningModelBanner
 *
 * The line above the roles grid that says when learning work cannot run:
 * no Learning model is set, or the one set cannot be used any more. Learning
 * never borrows the chat model by itself, so the fix is offered as one
 * explicit click, and the saved setting is never cleared for the user.
 */

import { TriangleAlert } from 'lucide-react';

import { useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { toast } from '@/stores/toastStore';
import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';
import { modelRefLabel, resolveDefaultChatOption } from '@/utils/chatModelSelection';

import { useModelRoles } from './ModelRolesContext';

type LearningModelState =
  | { kind: 'unset' }
  | { kind: 'unavailable'; message: string }
  | { kind: 'set' };

/** Whether learning work has a model it can run on, as far as Settings and the rows know. */
export function learningModelState(settings: LLMSettings, models: DownloadedModel[]): LearningModelState {
  const ref = settings.learningModel;
  if (!ref) return { kind: 'unset' };
  const label = modelRefLabel(ref, models);
  switch (ref.kind) {
    case 'local':
      return models.some((model) => model.model_id === ref.model_id)
        ? { kind: 'set' }
        : { kind: 'unavailable', message: `Learning model ${label} is not downloaded any more` };
    case 'ollama':
      return settings.ollamaUrl.trim()
        ? { kind: 'set' }
        : { kind: 'unavailable', message: `Learning model ${label} runs on Ollama, and no Ollama connection is configured` };
    case 'llama_cpp':
      return settings.llamaCpp.url.trim()
        ? { kind: 'set' }
        : { kind: 'unavailable', message: `Learning model ${label} runs on llama.cpp, and no llama.cpp connection is configured` };
    case 'cloud':
      return { kind: 'set' };
  }
}

const ROW_CLASS =
  'mb-6 flex flex-wrap items-center justify-between gap-x-4 gap-y-2 rounded-lg border border-[hsl(var(--warning)/0.35)] bg-[hsl(var(--warning-muted))] px-4 py-3 text-xs leading-relaxed text-[hsl(var(--warning-fg))]';
const ACTION_CLASS =
  'shrink-0 rounded-md border border-border-default bg-surface px-2.5 py-1 text-xs font-medium text-text-primary transition-colors duration-fast hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-60';

/** Puts keyboard focus on the first Learning button in the grid. */
function focusLearningButtons() {
  document.querySelector<HTMLButtonElement>('button[data-role="learning"]:not(:disabled)')?.focus();
}

export function LearningModelBanner() {
  const { models, llmSettings, isLoading } = useModelRoles();
  const { mutateAsync: updateSettings, isPending } = useUpdateSettingsMutation();

  if (isLoading || !llmSettings) return null;
  const state = learningModelState(llmSettings, models);
  if (state.kind === 'set') return null;

  if (state.kind === 'unavailable') {
    return (
      <div role="status" className={ROW_CLASS}>
        <span className="flex min-w-0 items-center gap-2">
          <TriangleAlert className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          {state.message}
        </span>
        <button type="button" onClick={focusLearningButtons} className={ACTION_CLASS}>
          Choose another
        </button>
      </div>
    );
  }

  const chatModel = resolveDefaultChatOption(llmSettings, models);
  const adoptChatModel = async () => {
    if (!chatModel) return;
    try {
      await updateSettings({ category: 'llm', updates: { learningModel: chatModel.ref } });
    } catch (error) {
      toast.error('Failed to set learning role', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  return (
    <div role="status" className={ROW_CLASS}>
      <span className="flex min-w-0 items-center gap-2">
        <TriangleAlert className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
        Learning: not set — lessons, outlines and study decks will not run until you choose one
      </span>
      {chatModel ? (
        <button type="button" onClick={() => void adoptChatModel()} disabled={isPending} className={ACTION_CLASS}>
          Use {chatModel.label} (your chat model)
        </button>
      ) : (
        <button type="button" onClick={focusLearningButtons} className={ACTION_CLASS}>
          Choose one
        </button>
      )}
    </div>
  );
}
