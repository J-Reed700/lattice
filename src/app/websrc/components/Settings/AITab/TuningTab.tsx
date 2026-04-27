/**
 * Tuning Tab - Model runtime parameters, router layer, and tool output layer configuration.
 */

import { useEffect, useState } from 'react';

import { Settings2 } from 'lucide-react';

import { useLlmSettings } from './useLlmSettings';
import { INPUT_CLASS, TEXTAREA_CLASS, toFinite, clamp } from './shared';

import type { LLMSettings as ApiLLMSettings } from '../../../types/api/settings';

export function TuningTab() {
  const { llmSettings, isLoading, saveLlmUpdates } = useLlmSettings();

  const [runtimeDraft, setRuntimeDraft] = useState({
    temperature: 0.7,
    topP: 0.9,
    topK: 40,
    repeatPenalty: 1.1,
    maxTokens: 131072,
    contextWindow: 131072,
    timeoutSeconds: 30,
  });
  const [routerDraft, setRouterDraft] = useState<ApiLLMSettings['router'] | null>(null);
  const [toolOutputDraft, setToolOutputDraft] = useState<ApiLLMSettings['toolOutput'] | null>(
    null
  );

  useEffect(() => {
    if (!llmSettings) return;
    setRuntimeDraft({
      temperature: llmSettings.temperature,
      topP: llmSettings.topP,
      topK: llmSettings.topK,
      repeatPenalty: llmSettings.repeatPenalty,
      maxTokens: llmSettings.maxTokens,
      contextWindow: llmSettings.contextWindow,
      timeoutSeconds: llmSettings.timeoutSeconds,
    });
    setRouterDraft(llmSettings.router);
    setToolOutputDraft(llmSettings.toolOutput);
  }, [llmSettings]);

  const saveRouterField = async <K extends keyof ApiLLMSettings['router']>(
    key: K,
    value: ApiLLMSettings['router'][K]
  ) => {
    if (!llmSettings) return;
    if (llmSettings.router[key] === value) return;
    await saveLlmUpdates({
      router: {
        ...llmSettings.router,
        [key]: value,
      },
    });
  };

  const saveToolOutputField = async <K extends keyof ApiLLMSettings['toolOutput']>(
    key: K,
    value: ApiLLMSettings['toolOutput'][K]
  ) => {
    if (!llmSettings) return;
    if (llmSettings.toolOutput[key] === value) return;
    await saveLlmUpdates({
      toolOutput: {
        ...llmSettings.toolOutput,
        [key]: value,
      },
    });
  };

  const saveToolTemplateField = async <
    K extends keyof ApiLLMSettings['toolOutput']['templates'],
  >(
    key: K,
    value: ApiLLMSettings['toolOutput']['templates'][K]
  ) => {
    if (!llmSettings) return;
    if (llmSettings.toolOutput.templates[key] === value) return;
    await saveLlmUpdates({
      toolOutput: {
        ...llmSettings.toolOutput,
        templates: {
          ...llmSettings.toolOutput.templates,
          [key]: value,
        },
      },
    });
  };

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center gap-3 pb-4 border-b border-[hsl(var(--border-subtle))]">
        <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
          <Settings2 className="w-5 h-5 text-[hsl(var(--accent))]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">Tuning</h2>
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Model runtime parameters, router layer, and tool output configuration
          </p>
        </div>
      </div>

      {isLoading || !llmSettings || !routerDraft || !toolOutputDraft ? (
        <div className="text-xs text-[hsl(var(--text-tertiary))]">
          Loading tuning and layer settings...
        </div>
      ) : (
        <div className="space-y-6">
          {/* Model Runtime */}
          <section className="space-y-3 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4">
            <div className="text-sm font-medium text-[hsl(var(--text-primary))]">Model Runtime</div>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Temperature
                </label>
                <input
                  type="number"
                  min={0}
                  max={2}
                  step={0.05}
                  value={runtimeDraft.temperature}
                  onChange={(event) =>
                    setRuntimeDraft((previous) => ({
                      ...previous,
                      temperature: toFinite(event.target.value, previous.temperature),
                    }))
                  }
                  onBlur={() =>
                    void saveLlmUpdates({
                      temperature: clamp(runtimeDraft.temperature, 0, 2),
                    })
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">Top P</label>
                <input
                  type="number"
                  min={0}
                  max={1}
                  step={0.01}
                  value={runtimeDraft.topP}
                  onChange={(event) =>
                    setRuntimeDraft((previous) => ({
                      ...previous,
                      topP: toFinite(event.target.value, previous.topP),
                    }))
                  }
                  onBlur={() =>
                    void saveLlmUpdates({
                      topP: clamp(runtimeDraft.topP, 0, 1),
                    })
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">Top K</label>
                <input
                  type="number"
                  min={0}
                  max={500}
                  step={1}
                  value={runtimeDraft.topK}
                  onChange={(event) =>
                    setRuntimeDraft((previous) => ({
                      ...previous,
                      topK: Math.round(toFinite(event.target.value, previous.topK)),
                    }))
                  }
                  onBlur={() =>
                    void saveLlmUpdates({
                      topK: Math.round(clamp(runtimeDraft.topK, 0, 500)),
                    })
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Repeat Penalty
                </label>
                <input
                  type="number"
                  min={0.5}
                  max={2}
                  step={0.05}
                  value={runtimeDraft.repeatPenalty}
                  onChange={(event) =>
                    setRuntimeDraft((previous) => ({
                      ...previous,
                      repeatPenalty: toFinite(event.target.value, previous.repeatPenalty),
                    }))
                  }
                  onBlur={() =>
                    void saveLlmUpdates({
                      repeatPenalty: clamp(runtimeDraft.repeatPenalty, 0.5, 2),
                    })
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Max Tokens
                </label>
                <input
                  type="number"
                  min={128}
                  max={runtimeDraft.contextWindow}
                  step={1}
                  value={runtimeDraft.maxTokens}
                  onChange={(event) =>
                    setRuntimeDraft((previous) => ({
                      ...previous,
                      maxTokens: Math.round(toFinite(event.target.value, previous.maxTokens)),
                    }))
                  }
                  onBlur={() =>
                    void saveLlmUpdates({
                      maxTokens: Math.round(
                        clamp(runtimeDraft.maxTokens, 128, runtimeDraft.contextWindow)
                      ),
                    })
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Context Window
                </label>
                <input
                  type="number"
                  min={512}
                  max={262144}
                  step={1}
                  value={runtimeDraft.contextWindow}
                  onChange={(event) =>
                    setRuntimeDraft((previous) => ({
                      ...previous,
                      contextWindow: Math.round(
                        toFinite(event.target.value, previous.contextWindow)
                      ),
                    }))
                  }
                  onBlur={() =>
                    void saveLlmUpdates({
                      contextWindow: Math.round(clamp(runtimeDraft.contextWindow, 512, 262144)),
                      maxTokens: Math.round(
                        clamp(runtimeDraft.maxTokens, 128, runtimeDraft.contextWindow)
                      ),
                    })
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Timeout (seconds)
                </label>
                <input
                  type="number"
                  min={5}
                  max={300}
                  step={1}
                  value={runtimeDraft.timeoutSeconds}
                  onChange={(event) =>
                    setRuntimeDraft((previous) => ({
                      ...previous,
                      timeoutSeconds: Math.round(
                        toFinite(event.target.value, previous.timeoutSeconds)
                      ),
                    }))
                  }
                  onBlur={() =>
                    void saveLlmUpdates({
                      timeoutSeconds: Math.round(clamp(runtimeDraft.timeoutSeconds, 5, 300)),
                    })
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="flex items-end">
                <label className="inline-flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))]">
                  <input
                    type="checkbox"
                    checked={llmSettings.streamResponses}
                    onChange={(event) =>
                      void saveLlmUpdates({ streamResponses: event.target.checked })
                    }
                    className="h-4 w-4 rounded border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--accent))] focus:ring-2 focus:ring-[hsl(var(--accent))]"
                  />
                  Stream Responses
                </label>
              </div>
            </div>
          </section>

          {/* Retrieval note */}
          <section className="space-y-3 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4">
            <div className="text-sm font-medium text-[hsl(var(--text-primary))]">Retrieval Tuning</div>
            <p className="text-xs text-[hsl(var(--text-secondary))]">
              Retrieval knobs moved to the Search tab. Use Settings &rarr; Search &rarr; Advanced
              Retrieval Tuning for reranker, shortlist, overlap, and support-ratio controls.
            </p>
          </section>

          {/* Router Layer */}
          <section className="space-y-3 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4">
            <div className="text-sm font-medium text-[hsl(var(--text-primary))]">Router Layer</div>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <label className="inline-flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))]">
                <input
                  type="checkbox"
                  checked={routerDraft.enabled}
                  onChange={(event) => {
                    const enabled = event.target.checked;
                    setRouterDraft((previous) =>
                      previous ? { ...previous, enabled } : previous
                    );
                    void saveRouterField('enabled', enabled);
                  }}
                  className="h-4 w-4 rounded border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--accent))] focus:ring-2 focus:ring-[hsl(var(--accent))]"
                />
                Router Enabled
              </label>
              <label className="inline-flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))]">
                <input
                  type="checkbox"
                  checked={routerDraft.preferLastDocument}
                  onChange={(event) => {
                    const preferLastDocument = event.target.checked;
                    setRouterDraft((previous) =>
                      previous ? { ...previous, preferLastDocument } : previous
                    );
                    void saveRouterField('preferLastDocument', preferLastDocument);
                  }}
                  className="h-4 w-4 rounded border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--accent))] focus:ring-2 focus:ring-[hsl(var(--accent))]"
                />
                Prefer Last Document
              </label>
              <div className="space-y-1.5 md:col-span-2">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Router Model
                </label>
                <input
                  type="text"
                  value={routerDraft.model}
                  onChange={(event) =>
                    setRouterDraft((previous) =>
                      previous ? { ...previous, model: event.target.value } : previous
                    )
                  }
                  onBlur={() => void saveRouterField('model', routerDraft.model.trim())}
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Timeout (ms)
                </label>
                <input
                  type="number"
                  min={50}
                  max={5000}
                  step={10}
                  value={routerDraft.timeoutMs}
                  onChange={(event) =>
                    setRouterDraft((previous) =>
                      previous
                        ? {
                            ...previous,
                            timeoutMs: Math.round(
                              toFinite(event.target.value, previous.timeoutMs)
                            ),
                          }
                        : previous
                    )
                  }
                  onBlur={() =>
                    void saveRouterField(
                      'timeoutMs',
                      Math.round(clamp(routerDraft.timeoutMs, 50, 5000))
                    )
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Max Tokens
                </label>
                <input
                  type="number"
                  min={16}
                  max={1024}
                  step={1}
                  value={routerDraft.maxTokens}
                  onChange={(event) =>
                    setRouterDraft((previous) =>
                      previous
                        ? {
                            ...previous,
                            maxTokens: Math.round(
                              toFinite(event.target.value, previous.maxTokens)
                            ),
                          }
                        : previous
                    )
                  }
                  onBlur={() =>
                    void saveRouterField(
                      'maxTokens',
                      Math.round(clamp(routerDraft.maxTokens, 16, 1024))
                    )
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Temperature
                </label>
                <input
                  type="number"
                  min={0}
                  max={1}
                  step={0.01}
                  value={routerDraft.temperature}
                  onChange={(event) =>
                    setRouterDraft((previous) =>
                      previous
                        ? { ...previous, temperature: toFinite(event.target.value, previous.temperature) }
                        : previous
                    )
                  }
                  onBlur={() =>
                    void saveRouterField('temperature', clamp(routerDraft.temperature, 0, 1))
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Ambiguity Threshold
                </label>
                <input
                  type="number"
                  min={0}
                  max={1}
                  step={0.01}
                  value={routerDraft.ambiguityThreshold}
                  onChange={(event) =>
                    setRouterDraft((previous) =>
                      previous
                        ? {
                            ...previous,
                            ambiguityThreshold: toFinite(
                              event.target.value,
                              previous.ambiguityThreshold
                            ),
                          }
                        : previous
                    )
                  }
                  onBlur={() =>
                    void saveRouterField(
                      'ambiguityThreshold',
                      clamp(routerDraft.ambiguityThreshold, 0, 1)
                    )
                  }
                  className={INPUT_CLASS}
                />
              </div>
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                Router Prompt Template
              </label>
              <textarea
                value={routerDraft.promptTemplate}
                onChange={(event) =>
                  setRouterDraft((previous) =>
                    previous ? { ...previous, promptTemplate: event.target.value } : previous
                  )
                }
                onBlur={() => void saveRouterField('promptTemplate', routerDraft.promptTemplate)}
                rows={6}
                className={TEXTAREA_CLASS}
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                Clarify Prompt Template
              </label>
              <textarea
                value={routerDraft.clarifyPromptTemplate}
                onChange={(event) =>
                  setRouterDraft((previous) =>
                    previous
                      ? { ...previous, clarifyPromptTemplate: event.target.value }
                      : previous
                  )
                }
                onBlur={() =>
                  void saveRouterField('clarifyPromptTemplate', routerDraft.clarifyPromptTemplate)
                }
                rows={3}
                className={TEXTAREA_CLASS}
              />
            </div>
          </section>

          {/* Tool Output Layer */}
          <section className="space-y-3 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4">
            <div className="text-sm font-medium text-[hsl(var(--text-primary))]">
              Tool Output Layer
            </div>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Max Chars
                </label>
                <input
                  type="number"
                  min={200}
                  max={200000}
                  step={50}
                  value={toolOutputDraft.maxChars}
                  onChange={(event) =>
                    setToolOutputDraft((previous) =>
                      previous
                        ? {
                            ...previous,
                            maxChars: Math.round(toFinite(event.target.value, previous.maxChars)),
                          }
                        : previous
                    )
                  }
                  onBlur={() =>
                    void saveToolOutputField(
                      'maxChars',
                      Math.round(clamp(toolOutputDraft.maxChars, 200, 200000))
                    )
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Excerpt Chars
                </label>
                <input
                  type="number"
                  min={100}
                  max={4000}
                  step={50}
                  value={toolOutputDraft.excerptChars}
                  onChange={(event) =>
                    setToolOutputDraft((previous) =>
                      previous
                        ? {
                            ...previous,
                            excerptChars: Math.round(
                              toFinite(event.target.value, previous.excerptChars)
                            ),
                          }
                        : previous
                    )
                  }
                  onBlur={() =>
                    void saveToolOutputField(
                      'excerptChars',
                      Math.round(clamp(toolOutputDraft.excerptChars, 100, 4000))
                    )
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Max Tool Results
                </label>
                <input
                  type="number"
                  min={1}
                  max={48}
                  step={1}
                  value={toolOutputDraft.maxResults}
                  onChange={(event) =>
                    setToolOutputDraft((previous) =>
                      previous
                        ? {
                            ...previous,
                            maxResults: Math.round(
                              toFinite(event.target.value, previous.maxResults)
                            ),
                          }
                        : previous
                    )
                  }
                  onBlur={() =>
                    void saveToolOutputField(
                      'maxResults',
                      Math.round(clamp(toolOutputDraft.maxResults, 1, 48))
                    )
                  }
                  className={INPUT_CLASS}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                  Highlight Terms Max
                </label>
                <input
                  type="number"
                  min={1}
                  max={32}
                  step={1}
                  value={toolOutputDraft.highlightTermsMax}
                  onChange={(event) =>
                    setToolOutputDraft((previous) =>
                      previous
                        ? {
                            ...previous,
                            highlightTermsMax: Math.round(
                              toFinite(event.target.value, previous.highlightTermsMax)
                            ),
                          }
                        : previous
                    )
                  }
                  onBlur={() =>
                    void saveToolOutputField(
                      'highlightTermsMax',
                      Math.round(clamp(toolOutputDraft.highlightTermsMax, 1, 32))
                    )
                  }
                  className={INPUT_CLASS}
                />
              </div>
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                Default Tool Template
              </label>
              <textarea
                value={toolOutputDraft.templates.defaultTemplate}
                onChange={(event) =>
                  setToolOutputDraft((previous) =>
                    previous
                      ? {
                          ...previous,
                          templates: {
                            ...previous.templates,
                            defaultTemplate: event.target.value,
                          },
                        }
                      : previous
                  )
                }
                onBlur={() =>
                  void saveToolTemplateField(
                    'defaultTemplate',
                    toolOutputDraft.templates.defaultTemplate
                  )
                }
                rows={3}
                className={TEXTAREA_CLASS}
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                Get Document Template
              </label>
              <textarea
                value={toolOutputDraft.templates.getDocumentTemplate}
                onChange={(event) =>
                  setToolOutputDraft((previous) =>
                    previous
                      ? {
                          ...previous,
                          templates: {
                            ...previous.templates,
                            getDocumentTemplate: event.target.value,
                          },
                        }
                      : previous
                  )
                }
                onBlur={() =>
                  void saveToolTemplateField(
                    'getDocumentTemplate',
                    toolOutputDraft.templates.getDocumentTemplate
                  )
                }
                rows={3}
                className={TEXTAREA_CLASS}
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                Semantic Search Template
              </label>
              <textarea
                value={toolOutputDraft.templates.semanticSearchTemplate}
                onChange={(event) =>
                  setToolOutputDraft((previous) =>
                    previous
                      ? {
                          ...previous,
                          templates: {
                            ...previous.templates,
                            semanticSearchTemplate: event.target.value,
                          },
                        }
                      : previous
                  )
                }
                onBlur={() =>
                  void saveToolTemplateField(
                    'semanticSearchTemplate',
                    toolOutputDraft.templates.semanticSearchTemplate
                  )
                }
                rows={3}
                className={TEXTAREA_CLASS}
              />
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
