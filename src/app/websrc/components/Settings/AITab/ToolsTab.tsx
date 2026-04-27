/**
 * Tools Tab - Built-in tools, custom tool presets, and tool management.
 */

import { useEffect, useMemo, useState } from 'react';

import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { Plus, Trash2, Wrench } from 'lucide-react';

import { useLlmSettings } from './useLlmSettings';
import { CUSTOM_TOOL_PRESETS, INPUT_CLASS, normalizeCustomTool } from './shared';
import { VaultAPI } from '../../../lib/api';
import { toast } from '../../../stores/toastStore';

import type { CustomToolSettings as ApiCustomToolSettings } from '../../../types/api/settings';

export function ToolsTab() {
  const { llmSettings, isLoading, saveLlmUpdates } = useLlmSettings();

  const [customToolsDraft, setCustomToolsDraft] = useState<ApiCustomToolSettings[]>([]);
  const [isSavingCustomTools, setIsSavingCustomTools] = useState(false);
  const [selectedPresetId, setSelectedPresetId] = useState(CUSTOM_TOOL_PRESETS[0].id);
  const [toolTestQuery, setToolTestQuery] = useState('arugula health benefits');
  const [activeToolTestKey, setActiveToolTestKey] = useState<string | null>(null);
  const [isImportingCustomTools, setIsImportingCustomTools] = useState(false);
  const [isExportingCustomTools, setIsExportingCustomTools] = useState(false);

  const selectedPreset =
    CUSTOM_TOOL_PRESETS.find((candidate) => candidate.id === selectedPresetId) ??
    CUSTOM_TOOL_PRESETS[0];
  const selectedPresetIndex = Math.max(
    0,
    CUSTOM_TOOL_PRESETS.findIndex((candidate) => candidate.id === selectedPreset.id)
  );

  useEffect(() => {
    if (!llmSettings) return;
    setCustomToolsDraft(llmSettings.customTools || []);
  }, [llmSettings]);

  const cyclePreset = (direction: 1 | -1) => {
    const total = CUSTOM_TOOL_PRESETS.length;
    if (total === 0) return;
    const nextIndex = (selectedPresetIndex + direction + total) % total;
    setSelectedPresetId(CUSTOM_TOOL_PRESETS[nextIndex].id);
  };

  const coerceCustomTool = (value: unknown): ApiCustomToolSettings | null => {
    if (!value || typeof value !== 'object') {
      return null;
    }

    const raw = value as Record<string, unknown>;
    const queryParamValue =
      (typeof raw.queryParam === 'string' ? raw.queryParam : undefined) ??
      (typeof raw.query_param === 'string' ? raw.query_param : undefined) ??
      'q';
    const maxResultsParamValue =
      (typeof raw.maxResultsParam === 'string' ? raw.maxResultsParam : undefined) ??
      (typeof raw.max_results_param === 'string' ? raw.max_results_param : undefined) ??
      null;
    const defaultMaxResultsValue =
      (typeof raw.defaultMaxResults === 'number' ? raw.defaultMaxResults : undefined) ??
      (typeof raw.default_max_results === 'number' ? raw.default_max_results : undefined) ??
      5;

    return normalizeCustomTool({
      enabled: typeof raw.enabled === 'boolean' ? raw.enabled : true,
      name: typeof raw.name === 'string' ? raw.name : '',
      description: typeof raw.description === 'string' ? raw.description : '',
      endpoint: typeof raw.endpoint === 'string' ? raw.endpoint : '',
      queryParam: queryParamValue,
      maxResultsParam: maxResultsParamValue,
      defaultMaxResults: defaultMaxResultsValue,
    });
  };

  const importCustomToolsFromJson = async () => {
    setIsImportingCustomTools(true);
    try {
      const filePath = await open({
        multiple: false,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath || typeof filePath !== 'string') {
        return;
      }

      const text = await readTextFile(filePath);
      const parsed = JSON.parse(text) as unknown;
      const candidates = Array.isArray(parsed)
        ? parsed
        : parsed && typeof parsed === 'object' && Array.isArray((parsed as { customTools?: unknown }).customTools)
          ? (parsed as { customTools: unknown[] }).customTools
          : null;

      if (!candidates) {
        toast.error('Invalid custom tools JSON', {
          message: 'Expected an array or { customTools: [...] }',
        });
        return;
      }

      const imported = candidates
        .map(coerceCustomTool)
        .filter((tool): tool is ApiCustomToolSettings => tool !== null);

      if (imported.length === 0) {
        toast.error('No valid tools found in JSON');
        return;
      }

      setCustomToolsDraft(imported);
      toast.success(`Imported ${imported.length} custom tool(s)`);
    } catch (error) {
      toast.error('Failed to import custom tools', {
        message: String(error),
      });
    } finally {
      setIsImportingCustomTools(false);
    }
  };

  const exportCustomToolsToJson = async () => {
    setIsExportingCustomTools(true);
    try {
      const filePath = await save({
        defaultPath: 'lattice-custom-tools.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath || typeof filePath !== 'string') {
        return;
      }

      const payload = {
        version: 1,
        customTools: customToolsDraft.map(normalizeCustomTool),
      };
      await writeTextFile(filePath, JSON.stringify(payload, null, 2));
      toast.success('Custom tools exported');
    } catch (error) {
      toast.error('Failed to export custom tools', {
        message: String(error),
      });
    } finally {
      setIsExportingCustomTools(false);
    }
  };

  const addCustomTool = () => {
    setCustomToolsDraft((previous) => [
      ...previous,
      {
        enabled: true,
        name: '',
        description: '',
        endpoint: '',
        queryParam: 'q',
        maxResultsParam: 'max_results',
        defaultMaxResults: 5,
      },
    ]);
  };

  const addPresetTool = () => {
    const preset = CUSTOM_TOOL_PRESETS.find((candidate) => candidate.id === selectedPresetId);
    if (!preset) return;

    const existing = new Set(
      customToolsDraft.map((tool) => tool.name.trim().toLowerCase()).filter((name) => name.length > 0)
    );
    if (existing.has(preset.tool.name.toLowerCase())) {
      toast.info(`Tool "${preset.tool.name}" already exists`);
      return;
    }

    setCustomToolsDraft((previous) => [...previous, { ...preset.tool }]);
  };

  const updateCustomTool = (
    index: number,
    field: keyof ApiCustomToolSettings,
    value: string | number | boolean | null
  ) => {
    setCustomToolsDraft((previous) =>
      previous.map((tool, toolIndex) => {
        if (toolIndex !== index) return tool;
        return {
          ...tool,
          [field]: value,
        };
      })
    );
  };

  const removeCustomTool = (index: number) => {
    setCustomToolsDraft((previous) => previous.filter((_, toolIndex) => toolIndex !== index));
  };

  const testCustomTool = async (tool: ApiCustomToolSettings, index: number) => {
    const normalized = normalizeCustomTool(tool);
    if (!normalized.endpoint) {
      toast.error('Endpoint is required');
      return;
    }
    if (!normalized.queryParam) {
      toast.error('Query param is required');
      return;
    }

    const key = `${normalized.name || 'custom_tool'}_${index}`;
    setActiveToolTestKey(key);
    const query = toolTestQuery.trim().length > 0 ? toolTestQuery.trim() : 'test';
    const result = await VaultAPI.testCustomTool({
      endpoint: normalized.endpoint,
      queryParam: normalized.queryParam,
      maxResultsParam: normalized.maxResultsParam,
      defaultMaxResults: normalized.defaultMaxResults,
      query,
      maxResults: normalized.defaultMaxResults,
    });
    setActiveToolTestKey(null);

    if (!result.ok) {
      toast.error('Custom tool test failed', {
        message: result.error,
      });
      return;
    }

    toast.success(`Tool test succeeded (HTTP ${result.data.status})`, {
      message: result.data.finalUrl,
    });
  };

  const persistCustomTools = async () => {
    if (!llmSettings) return;

    const normalized = customToolsDraft.map(normalizeCustomTool);
    setIsSavingCustomTools(true);
    const ok = await saveLlmUpdates({
      customTools: normalized,
    });
    setIsSavingCustomTools(false);

    if (ok) {
      setCustomToolsDraft(normalized);
      toast.success('Tool integrations saved');
    }
  };

  const resetCustomTools = () => {
    setCustomToolsDraft((llmSettings?.customTools || []).map(normalizeCustomTool));
  };

  const customToolsDirty = useMemo(() => {
    const normalizeAll = (tools: ApiCustomToolSettings[]) => tools.map(normalizeCustomTool);
    return (
      JSON.stringify(normalizeAll(customToolsDraft)) !==
      JSON.stringify(normalizeAll(llmSettings?.customTools || []))
    );
  }, [customToolsDraft, llmSettings?.customTools]);

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center gap-3 pb-4 border-b border-[hsl(var(--border-subtle))]">
        <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
          <Wrench className="w-5 h-5 text-[hsl(var(--accent))]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">Tool Integrations</h2>
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Built-in tools, custom search endpoints, and presets
          </p>
        </div>
      </div>

      {isLoading || !llmSettings ? (
        <div className="text-xs text-[hsl(var(--text-tertiary))]">Loading tools...</div>
      ) : (
        <div className="space-y-3 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4">
          <div className="space-y-2">
            <div className="text-xs font-medium text-[hsl(var(--text-secondary))]">Built-in tools</div>
            <div className="flex flex-wrap gap-2">
              {['web_search', 'fetch_url_content', 'wiki_search', 'wiki_summary'].map(
                (toolName) => (
                  <span
                    key={toolName}
                    className="inline-flex items-center rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2 py-1 text-[11px] text-[hsl(var(--text-secondary))]"
                  >
                    {toolName}
                  </span>
                )
              )}
            </div>
          </div>

          <div className="border-t border-[hsl(var(--border-subtle))] pt-3 space-y-3">
            <div className="rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] p-3 space-y-3">
              <div className="space-y-1">
                <div className="text-xs font-semibold text-[hsl(var(--text-primary))]">
                  Preset Catalog (Templates)
                </div>
                <p className="text-xs text-[hsl(var(--text-secondary))]">
                  Curated starter presets for common public APIs and self-hosted search. This
                  list is intentionally small and not exhaustive.
                </p>
                <p className="text-[11px] text-[hsl(var(--text-tertiary))]">
                  Select a preset, then click <span className="font-medium">Add Preset as Tool</span>.
                  This copies the template into your saved custom tools list below. Selecting a
                  preset alone does not change any saved tool.
                </p>
              </div>
              <div className="rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-2.5">
                <div className="flex items-center justify-between gap-2">
                  <button
                    type="button"
                    onClick={() => cyclePreset(-1)}
                    className="rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2 py-1 text-[11px] text-[hsl(var(--text-primary))] hover:border-[hsl(var(--border-default))]"
                  >
                    Previous
                  </button>
                  <div className="text-[11px] text-[hsl(var(--text-secondary))]">
                    Preset {selectedPresetIndex + 1} of {CUSTOM_TOOL_PRESETS.length}
                  </div>
                  <button
                    type="button"
                    onClick={() => cyclePreset(1)}
                    className="rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2 py-1 text-[11px] text-[hsl(var(--text-primary))] hover:border-[hsl(var(--border-default))]"
                  >
                    Next
                  </button>
                </div>
              </div>
              <div className="rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-2.5 text-xs text-[hsl(var(--text-secondary))]">
                <div className="font-medium text-[hsl(var(--text-primary))]">
                  Selected preset: {selectedPreset.label}
                </div>
                <div className="mt-1 text-[11px]">{selectedPreset.summary}</div>
                <div className="mt-1">Endpoint template: {selectedPreset.tool.endpoint}</div>
                <div className="mt-1">
                  Query param: {selectedPreset.tool.queryParam}
                  {selectedPreset.tool.maxResultsParam
                    ? ` | Max results param: ${selectedPreset.tool.maxResultsParam}`
                    : ' | Max results param: none'}
                </div>
                {selectedPreset.docsUrl && (
                  <a
                    href={selectedPreset.docsUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="mt-1 inline-block text-[hsl(var(--accent))] hover:underline"
                  >
                    Open preset docs
                  </a>
                )}
                {selectedPreset.id === 'searxng_search' && (
                  <p className="mt-1 text-[hsl(var(--text-tertiary))]">
                    Replace the endpoint with your deployed SearXNG URL
                    (`https://your-domain/search?format=json`). Localhost/private-network
                    endpoints are blocked by default security policy.
                  </p>
                )}
              </div>
            </div>

            <div className="flex items-center justify-between gap-3">
              <div>
                <h4 className="text-sm font-semibold text-[hsl(var(--text-primary))]">Saved Custom Tools</h4>
                <p className="text-xs text-[hsl(var(--text-secondary))] mt-0.5">
                  These are the tools chat can actually use after you click Save Tools.
                </p>
              </div>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => void importCustomToolsFromJson()}
                  disabled={isImportingCustomTools}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2.5 py-1.5 text-xs font-medium text-[hsl(var(--text-primary))] hover:border-[hsl(var(--border-default))] disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {isImportingCustomTools ? 'Importing...' : 'Import JSON'}
                </button>
                <button
                  type="button"
                  onClick={() => void exportCustomToolsToJson()}
                  disabled={isExportingCustomTools || customToolsDraft.length === 0}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2.5 py-1.5 text-xs font-medium text-[hsl(var(--text-primary))] hover:border-[hsl(var(--border-default))] disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {isExportingCustomTools ? 'Exporting...' : 'Export JSON'}
                </button>
                <button
                  type="button"
                  onClick={addPresetTool}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2.5 py-1.5 text-xs font-medium text-[hsl(var(--text-primary))] hover:border-[hsl(var(--border-default))]"
                >
                  <Plus className="w-3.5 h-3.5" />
                  Add Preset as Tool
                </button>
                <button
                  type="button"
                  onClick={addCustomTool}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2.5 py-1.5 text-xs font-medium text-[hsl(var(--text-primary))] hover:border-[hsl(var(--border-default))]"
                >
                  <Plus className="w-3.5 h-3.5" />
                  Add Tool
                </button>
              </div>
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                Test Query
              </label>
              <input
                type="text"
                value={toolTestQuery}
                onChange={(event) => setToolTestQuery(event.target.value)}
                placeholder="Enter a query used when testing tools"
                className={INPUT_CLASS}
              />
            </div>

            {customToolsDraft.length === 0 ? (
              <p className="text-xs text-[hsl(var(--text-tertiary))]">
                No custom tools configured. Add one to expose a custom search endpoint to the
                model.
              </p>
            ) : (
              <div className="space-y-3">
                {customToolsDraft.map((tool, index) => (
                  <div
                    key={`${tool.name || 'custom_tool'}_${index}`}
                    className="rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] p-3 space-y-3"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <label className="inline-flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))]">
                        <input
                          type="checkbox"
                          checked={tool.enabled}
                          onChange={(event) =>
                            updateCustomTool(index, 'enabled', event.target.checked)
                          }
                          className="h-4 w-4 rounded border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--accent))] focus:ring-2 focus:ring-[hsl(var(--accent))]"
                        />
                        Enabled
                      </label>
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={() => void testCustomTool(tool, index)}
                          disabled={activeToolTestKey === `${tool.name || 'custom_tool'}_${index}`}
                          className="inline-flex items-center gap-1 rounded-md border border-[hsl(var(--border-subtle))] px-2 py-1 text-xs text-[hsl(var(--text-secondary))] hover:border-[hsl(var(--border-default))] hover:text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-60"
                        >
                          {activeToolTestKey === `${tool.name || 'custom_tool'}_${index}`
                            ? 'Testing...'
                            : 'Test'}
                        </button>
                        <button
                          type="button"
                          onClick={() => removeCustomTool(index)}
                          className="inline-flex items-center gap-1 text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--danger-fg))]"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                          Remove
                        </button>
                      </div>
                    </div>

                    <div className="grid grid-cols-2 gap-3">
                      <div className="space-y-1.5">
                        <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                          Tool Name
                        </label>
                        <input
                          type="text"
                          value={tool.name}
                          onChange={(event) =>
                            updateCustomTool(index, 'name', event.target.value)
                          }
                          placeholder="pubmed_search"
                          className={INPUT_CLASS}
                        />
                      </div>
                      <div className="space-y-1.5">
                        <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                          Query Param
                        </label>
                        <input
                          type="text"
                          value={tool.queryParam}
                          onChange={(event) =>
                            updateCustomTool(index, 'queryParam', event.target.value)
                          }
                          placeholder="q"
                          className={INPUT_CLASS}
                        />
                      </div>
                    </div>

                    <div className="space-y-1.5">
                      <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                        Description
                      </label>
                      <input
                        type="text"
                        value={tool.description}
                        onChange={(event) =>
                          updateCustomTool(index, 'description', event.target.value)
                        }
                        placeholder="Search public medical abstracts"
                        className={INPUT_CLASS}
                      />
                    </div>

                    <div className="space-y-1.5">
                      <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                        Endpoint
                      </label>
                      <input
                        type="url"
                        value={tool.endpoint}
                        onChange={(event) =>
                          updateCustomTool(index, 'endpoint', event.target.value)
                        }
                        placeholder="https://example.org/search"
                        className={INPUT_CLASS}
                      />
                    </div>

                    <div className="grid grid-cols-2 gap-3">
                      <div className="space-y-1.5">
                        <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                          Max Results Param (optional)
                        </label>
                        <input
                          type="text"
                          value={tool.maxResultsParam || ''}
                          onChange={(event) =>
                            updateCustomTool(index, 'maxResultsParam', event.target.value)
                          }
                          placeholder="limit"
                          className={INPUT_CLASS}
                        />
                      </div>
                      <div className="space-y-1.5">
                        <label className="text-xs font-medium text-[hsl(var(--text-secondary))]">
                          Default Max Results
                        </label>
                        <input
                          type="number"
                          min={1}
                          max={100}
                          value={tool.defaultMaxResults}
                          onChange={(event) =>
                            updateCustomTool(
                              index,
                              'defaultMaxResults',
                              Number(event.target.value || 0)
                            )
                          }
                          className={INPUT_CLASS}
                        />
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}

            <div className="flex items-center justify-end gap-2 pt-1">
              <button
                type="button"
                onClick={resetCustomTools}
                disabled={!customToolsDirty || isSavingCustomTools}
                className="rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-3 py-1.5 text-xs font-medium text-[hsl(var(--text-primary))] disabled:cursor-not-allowed disabled:opacity-50"
              >
                Reset
              </button>
              <button
                type="button"
                onClick={persistCustomTools}
                disabled={!customToolsDirty || isSavingCustomTools}
                className="rounded-lg bg-[hsl(var(--accent))] px-3 py-1.5 text-xs font-medium text-[hsl(var(--accent-fg))] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {isSavingCustomTools ? 'Saving...' : 'Save Tools'}
              </button>
            </div>

            <p className="text-xs text-[hsl(var(--text-tertiary))]">
              Tool names must use only letters, numbers, and underscores.
            </p>
            <p className="text-xs text-[hsl(var(--text-tertiary))]">
              Preset = template only. Tool = saved runnable integration.
            </p>
            <p className="text-xs text-[hsl(var(--text-tertiary))]">
              Presets are convenience defaults. Some providers apply rate limits or anti-bot
              protection and may intermittently fail.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
