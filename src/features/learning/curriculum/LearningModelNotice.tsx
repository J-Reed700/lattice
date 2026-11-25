import { Cpu, TriangleAlert } from 'lucide-react';

import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { learningModelState } from '@/features/settings/components/modelRoles/LearningModelBanner';
import { useAdoptChatModel } from '@/features/settings/hooks/useAdoptChatModel';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import { MODEL_SETTINGS_PATH } from '@/lib/settingsLinks';

const ACTION_CLASS =
  'pressable inline-flex h-8 shrink-0 items-center justify-center rounded-lg border border-border-default bg-surface px-3 text-xs font-medium text-text-primary shadow-control transition-[background-color,border-color,color,scale] duration-fast hover:border-border-strong hover:bg-surface-raised focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50';

function sameModelRef(
  a: NonNullable<ReturnType<typeof useSettingsQuery>['data']>['llm']['learningModel'] & object,
  b: NonNullable<ReturnType<typeof useSettingsQuery>['data']>['llm']['learningModel'] & object,
): boolean {
  const keysA = Object.keys(a) as Array<keyof typeof a>;
  const keysB = Object.keys(b);
  return keysA.length === keysB.length && keysA.every((key) => a[key] === b[key]);
}

/** Above the course builder when no usable Learning model is set: the one click that fixes it, before the goal is typed. */
export function LearningModelNotice() {
  const settings = useSettingsQuery().data?.llm;
  const { downloadedModels, isLoading: modelsLoading, error: modelsError } = useDownloadedModels();
  const { chatModel, adopt, isPending } = useAdoptChatModel(settings, downloadedModels);
  const navigate = useAppNavigate();
  if (!settings) return null;
  if (settings.learningModel?.kind === 'local' && (Boolean(modelsLoading) || modelsError != null)) return null;
  const state = learningModelState(settings, downloadedModels);
  if (state.kind === 'set') return null;
  const canAdoptChat = Boolean(
    chatModel
    && (!settings.learningModel || !sameModelRef(chatModel.ref, settings.learningModel))
    && learningModelState({ ...settings, learningModel: chatModel.ref }, downloadedModels).kind === 'set',
  );
  const unavailable = state.kind === 'unavailable';
  const text = unavailable ? state.message : 'Courses need a model to write them.';
  return (
    <div
      role="status"
      className="mx-auto mb-5 flex w-full max-w-[1360px] flex-wrap items-center justify-between gap-3 rounded-xl border border-border-default bg-surface px-3.5 py-2.5 text-ui text-text-primary shadow-sm"
    >
      <div className="flex min-w-0 items-center gap-2.5">
        <span
          aria-hidden="true"
          className={`grid h-7 w-7 shrink-0 place-items-center rounded-lg ${
            unavailable ? 'bg-warning-muted text-warning-fg' : 'bg-accent-muted text-accent'
          }`}
        >
          {unavailable ? <TriangleAlert className="h-3.5 w-3.5" /> : <Cpu className="h-3.5 w-3.5" />}
        </span>
        <span className="min-w-0 font-medium text-text-primary">{text}</span>
      </div>
      {canAdoptChat && chatModel
        ? <button type="button" disabled={isPending} onClick={() => void adopt()} className={ACTION_CLASS}>Use {chatModel.label}</button>
        : <button type="button" onClick={() => navigate(MODEL_SETTINGS_PATH)} className={ACTION_CLASS}>Choose a model</button>}
    </div>
  );
}
