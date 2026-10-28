/**
 * Tuning — model runtime parameters, the router layer, and tool output shaping.
 */

import { useEffect, useState } from 'react';

import { PageHeader } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { RouterTuningSection } from './RouterTuningSection';
import { RuntimeTuningSection } from './RuntimeTuningSection';
import { ThinkingSection } from './ThinkingSection';
import { ToolOutputTuningSection } from './ToolOutputTuningSection';
import { useLlmSettings } from './useLlmSettings';


export function TuningTab() {
  const { llmSettings, isLoading, saveLlmUpdates } = useLlmSettings();

  const [runtimeDraft, setRuntimeDraft] = useState({
    temperature: 0.7,
    topP: 0.8,
    topK: 20,
    repeatPenalty: 1.0,
    presencePenalty: 1.5,
    maxTokens: 131072,
    contextWindow: 131072,
    timeoutSeconds: 30,
  });
  const [routerDraft, setRouterDraft] = useState<ApiLLMSettings['router'] | null>(null);
  const [toolOutputDraft, setToolOutputDraft] = useState<ApiLLMSettings['toolOutput'] | null>(null);

  useEffect(() => {
    if (!llmSettings) return;
    setRuntimeDraft({
      temperature: llmSettings.temperature,
      topP: llmSettings.topP,
      topK: llmSettings.topK,
      repeatPenalty: llmSettings.repeatPenalty,
      presencePenalty: llmSettings.presencePenalty,
      maxTokens: llmSettings.maxTokens,
      contextWindow: llmSettings.contextWindow,
      timeoutSeconds: llmSettings.timeoutSeconds,
    });
    setRouterDraft(llmSettings.router);
    setToolOutputDraft(llmSettings.toolOutput);
  }, [llmSettings]);

  if (isLoading || !llmSettings || !routerDraft || !toolOutputDraft) {
    return (
      <>
        <PageHeader title="Tuning" description="Fine-tune model behavior, context, and resource use." />
        <p className="text-sm text-text-muted">Loading…</p>
      </>
    );
  }

  return (
    <>
      <PageHeader title="Tuning" description="Fine-tune model behavior, context, and resource use." />

      <RuntimeTuningSection
        llmSettings={llmSettings}
        runtimeDraft={runtimeDraft}
        setRuntimeDraft={setRuntimeDraft}
        saveLlmUpdates={saveLlmUpdates}
      />

      <ThinkingSection llmSettings={llmSettings} saveLlmUpdates={saveLlmUpdates} />

      <RouterTuningSection
        llmSettings={llmSettings}
        routerDraft={routerDraft}
        setRouterDraft={setRouterDraft}
        saveLlmUpdates={saveLlmUpdates}
      />

      <ToolOutputTuningSection
        llmSettings={llmSettings}
        toolOutputDraft={toolOutputDraft}
        setToolOutputDraft={setToolOutputDraft}
        saveLlmUpdates={saveLlmUpdates}
      />
    </>
  );
}
