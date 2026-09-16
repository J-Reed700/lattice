/**
 * Tools — built-in tools, preset templates, and saved custom endpoints.
 */

import { useEffect, useMemo, useState } from 'react';

import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { X } from 'lucide-react';

import {
  CUSTOM_TOOL_PRESETS,
  GHOST_BUTTON_CLASS,
  INPUT_CLASS,
  NUMBER_FIELD_CLASS,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
  SWITCH_CLASS,
  normalizeCustomTool,
} from './shared';
import { useLlmSettings } from './useLlmSettings';
import { VaultAPI } from '../../../lib/api';
import { toast } from '../../../stores/toastStore';
import { PageHeader, SettingsRow, SettingsSection, Switch } from '../../ui';

import type { CustomToolSettings as ApiCustomToolSettings } from '../../../types/api/settings';

const BUILT_IN_TOOLS = ['web_search', 'fetch_url_content', 'wiki_search', 'wiki_summary'];

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

  useEffect(() => {
    if (!llmSettings) return;
    setCustomToolsDraft(llmSettings.customTools || []);
  }, [llmSettings]);

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
        : parsed &&
            typeof parsed === 'object' &&
            Array.isArray((parsed as { customTools?: unknown }).customTools)
          ? (parsed as { customTools: unknown[] }).customTools
          : null;

      if (!candidates) {
        toast.error("Couldn't read that file", {
          message: 'Expected an array or { customTools: [...] }',
        });
        return;
      }

      const imported = candidates
        .map(coerceCustomTool)
        .filter((tool): tool is ApiCustomToolSettings => tool !== null);

      if (imported.length === 0) {
        toast.error('No valid tools in that file');
        return;
      }

      setCustomToolsDraft(imported);
      toast.success(`Imported ${imported.length} tool(s)`);
    } catch (error) {
      toast.error("Couldn't import tools", {
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
      toast.success('Tools exported');
    } catch (error) {
      toast.error("Couldn't export tools", {
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
      customToolsDraft
        .map((tool) => tool.name.trim().toLowerCase())
        .filter((name) => name.length > 0)
    );
    if (existing.has(preset.tool.name.toLowerCase())) {
      toast.info(`"${preset.tool.name}" is already in your tools`);
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
      toast.error('Add an endpoint first');
      return;
    }
    if (!normalized.queryParam) {
      toast.error('Add a query param first');
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
      toast.error("The tool didn't respond", {
        message: result.error,
      });
      return;
    }

    toast.success(`Tool responded (HTTP ${result.data.status})`, {
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
      toast.success('Tools saved');
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

  if (isLoading || !llmSettings) {
    return (
      <>
        <PageHeader title="Tools" />
        <p className="text-sm text-text-muted">Loading…</p>
      </>
    );
  }

  return (
    <>
      <PageHeader title="Tools" />

      <SettingsSection title="Built in">
        {BUILT_IN_TOOLS.map((toolName) => (
          <div
            key={toolName}
            className="flex items-center justify-between gap-4 border-b border-border-subtle py-2.5"
          >
            <span className="font-mono text-xs text-text-primary">{toolName}</span>
            <span className="text-xs text-text-muted">Always on</span>
          </div>
        ))}
      </SettingsSection>

      <SettingsSection
        title="Presets"
        actions={
          <button type="button" onClick={addPresetTool} className={SECONDARY_BUTTON_CLASS}>
            Add as tool
          </button>
        }
      >
        <SettingsRow label="Preset" htmlFor="tool-preset">
          <select
            id="tool-preset"
            value={selectedPresetId}
            onChange={(event) => setSelectedPresetId(event.target.value)}
            className={INPUT_CLASS}
          >
            {CUSTOM_TOOL_PRESETS.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.label}
              </option>
            ))}
          </select>
        </SettingsRow>

        <SettingsRow label={selectedPreset.label} hint={selectedPreset.summary} stacked>
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs">
            <dt className="text-text-muted">Endpoint</dt>
            <dd className="break-all font-mono text-text-primary">
              {selectedPreset.tool.endpoint}
            </dd>
            <dt className="text-text-muted">Query param</dt>
            <dd className="font-mono text-text-primary">{selectedPreset.tool.queryParam}</dd>
            <dt className="text-text-muted">Max results param</dt>
            <dd className="font-mono text-text-primary">
              {selectedPreset.tool.maxResultsParam ?? 'none'}
            </dd>
          </dl>
          {selectedPreset.docsUrl ? (
            <a
              href={selectedPreset.docsUrl}
              target="_blank"
              rel="noreferrer"
              className="mt-2 inline-block text-xs text-accent hover:underline"
            >
              Open docs
            </a>
          ) : null}
        </SettingsRow>
      </SettingsSection>

      <SettingsSection
        title="Custom tools"
        actions={
          <>
            <button
              type="button"
              onClick={() => void importCustomToolsFromJson()}
              disabled={isImportingCustomTools}
              className={GHOST_BUTTON_CLASS}
            >
              {isImportingCustomTools ? 'Importing…' : 'Import'}
            </button>
            <button
              type="button"
              onClick={() => void exportCustomToolsToJson()}
              disabled={isExportingCustomTools || customToolsDraft.length === 0}
              className={GHOST_BUTTON_CLASS}
            >
              {isExportingCustomTools ? 'Exporting…' : 'Export'}
            </button>
            <button type="button" onClick={addCustomTool} className={SECONDARY_BUTTON_CLASS}>
              Add tool
            </button>
          </>
        }
      >
        {customToolsDraft.length > 0 ? (
          <SettingsRow
            label="Test query"
            hint="Sent to the endpoint when you press Test."
            htmlFor="tool-test-query"
          >
            <input
              id="tool-test-query"
              type="text"
              value={toolTestQuery}
              onChange={(event) => setToolTestQuery(event.target.value)}
              className={INPUT_CLASS}
            />
          </SettingsRow>
        ) : null}

        {customToolsDraft.length === 0 ? (
          <div className="border-b border-border-subtle py-3 text-sm text-text-muted">
            No custom tools yet.
          </div>
        ) : (
          customToolsDraft.map((tool, index) => {
            const testKey = `${tool.name || 'custom_tool'}_${index}`;
            const idPrefix = `custom-tool-${index}`;
            return (
              <div key={testKey} className="border-b border-border-subtle py-4">
                <div className="flex items-center justify-between gap-3 pb-2">
                  <div className="flex min-w-0 items-center gap-3">
                    <Switch
                      className={SWITCH_CLASS}
                      checked={tool.enabled}
                      onCheckedChange={(checked) => updateCustomTool(index, 'enabled', checked)}
                      aria-label={`Enable ${tool.name || 'this tool'}`}
                    />
                    <span className="truncate font-mono text-sm text-text-primary">
                      {tool.name || 'Untitled tool'}
                    </span>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <button
                      type="button"
                      onClick={() => void testCustomTool(tool, index)}
                      disabled={activeToolTestKey === testKey}
                      className={GHOST_BUTTON_CLASS}
                    >
                      {activeToolTestKey === testKey ? 'Testing…' : 'Test'}
                    </button>
                    <button
                      type="button"
                      onClick={() => removeCustomTool(index)}
                      aria-label={`Remove ${tool.name || 'this tool'}`}
                      title="Remove tool"
                      className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-text-muted transition-colors duration-fast hover:bg-surface-raised hover:text-danger-fg"
                    >
                      <X className="h-4 w-4" />
                    </button>
                  </div>
                </div>

                <div className="grid grid-cols-1 gap-x-6 gap-y-3 md:grid-cols-2">
                  <Field
                    id={`${idPrefix}-name`}
                    label="Name"
                    hint="Letters, numbers and underscores only."
                  >
                    <input
                      id={`${idPrefix}-name`}
                      type="text"
                      value={tool.name}
                      onChange={(event) => updateCustomTool(index, 'name', event.target.value)}
                      placeholder="pubmed_search"
                      className={INPUT_CLASS}
                    />
                  </Field>
                  <Field id={`${idPrefix}-query-param`} label="Query param">
                    <input
                      id={`${idPrefix}-query-param`}
                      type="text"
                      value={tool.queryParam}
                      onChange={(event) =>
                        updateCustomTool(index, 'queryParam', event.target.value)
                      }
                      placeholder="q"
                      className={INPUT_CLASS}
                    />
                  </Field>
                  <Field id={`${idPrefix}-description`} label="Description" wide>
                    <input
                      id={`${idPrefix}-description`}
                      type="text"
                      value={tool.description}
                      onChange={(event) =>
                        updateCustomTool(index, 'description', event.target.value)
                      }
                      placeholder="Search public medical abstracts"
                      className={INPUT_CLASS}
                    />
                  </Field>
                  <Field id={`${idPrefix}-endpoint`} label="Endpoint" wide>
                    <input
                      id={`${idPrefix}-endpoint`}
                      type="url"
                      value={tool.endpoint}
                      onChange={(event) => updateCustomTool(index, 'endpoint', event.target.value)}
                      placeholder="https://example.org/search"
                      className={INPUT_CLASS}
                    />
                  </Field>
                  <Field id={`${idPrefix}-max-results-param`} label="Max results param">
                    <input
                      id={`${idPrefix}-max-results-param`}
                      type="text"
                      value={tool.maxResultsParam || ''}
                      onChange={(event) =>
                        updateCustomTool(index, 'maxResultsParam', event.target.value)
                      }
                      placeholder="limit"
                      className={INPUT_CLASS}
                    />
                  </Field>
                  <Field id={`${idPrefix}-max-results`} label="Default max results">
                    <input
                      id={`${idPrefix}-max-results`}
                      type="number"
                      min={1}
                      max={100}
                      value={tool.defaultMaxResults}
                      onChange={(event) =>
                        updateCustomTool(index, 'defaultMaxResults', Number(event.target.value || 0))
                      }
                      className={NUMBER_FIELD_CLASS}
                    />
                  </Field>
                </div>
              </div>
            );
          })
        )}

        <div className="flex items-center justify-end gap-2 pt-4">
          <button
            type="button"
            onClick={resetCustomTools}
            disabled={!customToolsDirty || isSavingCustomTools}
            className={GHOST_BUTTON_CLASS}
          >
            Discard changes
          </button>
          <button
            type="button"
            onClick={persistCustomTools}
            disabled={!customToolsDirty || isSavingCustomTools}
            className={PRIMARY_BUTTON_CLASS}
          >
            {isSavingCustomTools ? 'Saving…' : 'Save tools'}
          </button>
        </div>
      </SettingsSection>
    </>
  );
}

function Field({
  id,
  label,
  hint,
  wide,
  children,
}: {
  id: string;
  label: string;
  hint?: string;
  wide?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className={wide ? 'md:col-span-2' : undefined}>
      <label htmlFor={id} className="block pb-1 text-xs text-text-muted">
        {label}
      </label>
      {children}
      {hint ? <p className="pt-1 text-xs text-text-muted">{hint}</p> : null}
    </div>
  );
}
