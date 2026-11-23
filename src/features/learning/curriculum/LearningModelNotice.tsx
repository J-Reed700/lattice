import { useDownloadedModels } from '@/features/model/hooks/useDownloadedModels';
import { learningModelState } from '@/features/settings/components/modelRoles/LearningModelBanner';
import { useAdoptChatModel } from '@/features/settings/hooks/useAdoptChatModel';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import { MODEL_SETTINGS_PATH } from '@/lib/settingsLinks';

const ACTION_CLASS = 'shrink-0 rounded-md border border-border bg-surface px-3 py-1.5 text-sm font-medium text-text-primary hover:bg-bg disabled:opacity-60';

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
  const text = state.kind === 'unavailable' ? state.message : 'Courses need a model to write them.';
  return <div role="status" className="mx-auto mb-6 flex max-w-6xl flex-wrap items-center justify-between gap-3 rounded-xl border border-[hsl(var(--warning)/0.35)] bg-[hsl(var(--warning-muted))] px-4 py-3 text-sm text-[hsl(var(--warning-fg))]">
    <span>{text}</span>
    {canAdoptChat && chatModel
      ? <button type="button" disabled={isPending} onClick={() => void adopt()} className={ACTION_CLASS}>Use {chatModel.label}</button>
      : <button type="button" onClick={() => navigate(MODEL_SETTINGS_PATH)} className={ACTION_CLASS}>Choose a model</button>}
  </div>;
}
