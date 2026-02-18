/**
 * AI Settings Tab
 *
 * Configure chat provider, browse downloadable models, and tune advanced options.
 */

import { useEffect, useRef, useState } from 'react';

import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import {
  Brain,
  ChevronDown,
  ChevronRight,
  Download,
  FolderPlus,
  Plus,
  Server,
  Settings2,
  ShieldCheck,
  ShieldOff,
  Trash2,
  X,
} from 'lucide-react';

import { ModelCatalogBrowser } from './ModelCatalog';
import { useDownloadedModels } from '../../hooks/useDownloadedModels';
import { VaultAPI } from '../../lib/api';
import { useDownloadedModelsStore } from '../../stores/downloadedModelsStore';
import { useModelCatalogStore } from '../../stores/modelCatalogStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { toast } from '../../stores/toastStore';

import type {
  CustomToolSettings as ApiCustomToolSettings,
  LLMSettings as ApiLLMSettings,
} from '../../types/api/settings';

type CustomToolPreset = {
  id: string;
  label: string;
  summary: string;
  docsUrl?: string;
  tool: ApiCustomToolSettings;
};

const CUSTOM_TOOL_PRESETS: CustomToolPreset[] = [
  {
    id: 'searxng_search',
    label: 'SearXNG (self-hosted)',
    summary: 'Meta-search through your own SearXNG instance (best for privacy and control)',
    docsUrl: 'https://docs.searxng.org/dev/search_api.html',
    tool: {
      enabled: true,
      name: 'searxng_search',
      description: 'Search with your SearXNG instance (JSON endpoint)',
      endpoint: 'https://your-searxng-domain.example/search?format=json',
      queryParam: 'q',
      maxResultsParam: null,
      defaultMaxResults: 5,
    },
  },
  {
    id: 'openalex_search',
    label: 'OpenAlex (works)',
    summary: 'Scholarly works metadata and abstracts index',
    docsUrl: 'https://docs.openalex.org/',
    tool: {
      enabled: true,
      name: 'openalex_search',
      description: 'Search scholarly works via OpenAlex (free, no API key)',
      endpoint: 'https://api.openalex.org/works',
      queryParam: 'search',
      maxResultsParam: 'per-page',
      defaultMaxResults: 10,
    },
  },
  {
    id: 'crossref_works',
    label: 'Crossref (works)',
    summary: 'Publication metadata and DOI discovery',
    docsUrl: 'https://api.crossref.org/swagger-ui/index.html',
    tool: {
      enabled: true,
      name: 'crossref_works',
      description: 'Search publication metadata via Crossref (free, no API key)',
      endpoint: 'https://api.crossref.org/works',
      queryParam: 'query',
      maxResultsParam: 'rows',
      defaultMaxResults: 10,
    },
  },
  {
    id: 'europe_pmc_search',
    label: 'Europe PMC',
    summary: 'Biomedical literature and preprints',
    docsUrl: 'https://europepmc.org/RestfulWebService',
    tool: {
      enabled: true,
      name: 'europe_pmc_search',
      description: 'Search biomedical literature via Europe PMC (free, no API key)',
      endpoint: 'https://www.ebi.ac.uk/europepmc/webservices/rest/search?format=json',
      queryParam: 'query',
      maxResultsParam: 'pageSize',
      defaultMaxResults: 10,
    },
  },
  {
    id: 'openlibrary_search',
    label: 'Open Library',
    summary: 'Books, editions, and bibliographic records',
    docsUrl: 'https://openlibrary.org/dev/docs/api/search',
    tool: {
      enabled: true,
      name: 'openlibrary_search',
      description: 'Search books and editions via Open Library (free, no API key)',
      endpoint: 'https://openlibrary.org/search.json',
      queryParam: 'q',
      maxResultsParam: 'limit',
      defaultMaxResults: 10,
    },
  },
  {
    id: 'semantic_scholar_search',
    label: 'Semantic Scholar',
    summary: 'Academic paper search with paper metadata',
    docsUrl: 'https://api.semanticscholar.org/api-docs/',
    tool: {
      enabled: true,
      name: 'semantic_scholar_search',
      description: 'Search papers via Semantic Scholar Graph API (free tier, no key required)',
      endpoint: 'https://api.semanticscholar.org/graph/v1/paper/search',
      queryParam: 'query',
      maxResultsParam: 'limit',
      defaultMaxResults: 10,
    },
  },
];


export function AITab() {
  const aiSettings = useSettingsStore((state) => state.settings.ai);
  const updateAI = useSettingsStore((state) => state.updateAI);
  const [llmSettings, setLlmSettings] = useState<ApiLLMSettings | null>(null);
  const [ollamaUrlDraft, setOllamaUrlDraft] = useState('');
  const [ollamaModelDraft, setOllamaModelDraft] = useState('');
  const [ollamaHeaderNameDraft, setOllamaHeaderNameDraft] = useState('');
  const [ollamaHeaderValueDraft, setOllamaHeaderValueDraft] = useState('');
  const [ollamaBasicUserDraft, setOllamaBasicUserDraft] = useState('');
  const [ollamaBasicPassDraft, setOllamaBasicPassDraft] = useState('');
  const [ollamaAvailableModels, setOllamaAvailableModels] = useState<string[]>([]);
  const [ollamaModelsEndpoint, setOllamaModelsEndpoint] = useState('');
  const [isTestingOllamaConnection, setIsTestingOllamaConnection] = useState(false);
  const [isLoadingChatSettings, setIsLoadingChatSettings] = useState(true);
  const [showOllamaAuth, setShowOllamaAuth] = useState(false);
  const [isAddingExternalDirectory, setIsAddingExternalDirectory] = useState(false);
  const [systemPromptDraft, setSystemPromptDraft] = useState('');
  const [greetingPromptDraft, setGreetingPromptDraft] = useState('');
  const [ragPromptDraft, setRagPromptDraft] = useState('');
  const [noContextPromptDraft, setNoContextPromptDraft] = useState('');
  const [toolFollowupPromptDraft, setToolFollowupPromptDraft] = useState('');
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
  const cyclePreset = (direction: 1 | -1) => {
    const total = CUSTOM_TOOL_PRESETS.length;
    if (total === 0) return;
    const nextIndex = (selectedPresetIndex + direction + total) % total;
    setSelectedPresetId(CUSTOM_TOOL_PRESETS[nextIndex].id);
  };
  const [modelDownloadPath, setModelDownloadPath] = useState('');
  const [isLoadingModelDownloadPath, setIsLoadingModelDownloadPath] = useState(true);
  const [isModelCatalogExpanded, setIsModelCatalogExpanded] = useState(false);
  const [isVerificationHelpExpanded, setIsVerificationHelpExpanded] = useState(false);
  const { getActiveModel, getActiveEmbeddingModel } = useDownloadedModels();
  const activeChatModel = useDownloadedModelsStore((state) => state.activeModel);
  const activeEmbeddingModel = useDownloadedModelsStore((state) => state.activeEmbeddingModel);
  const selectedCatalogModel = useModelCatalogStore((state) => state.selectedModel);
  const downloadSectionRef = useRef<HTMLElement | null>(null);
  const tuningSectionRef = useRef<HTMLElement | null>(null);
  const verificationSectionRef = useRef<HTMLElement | null>(null);
  const toolsSectionRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    let isActive = true;

    const loadSettings = async () => {
      const result = await VaultAPI.getSettings();
      if (!isActive) return;

      if (result.ok) {
        setLlmSettings(result.data.llm);
        setOllamaUrlDraft(result.data.llm.ollamaUrl);
        setOllamaModelDraft(result.data.llm.model);
        setOllamaHeaderNameDraft(result.data.llm.ollamaAuthHeaderName || '');
        setOllamaHeaderValueDraft(result.data.llm.ollamaAuthHeaderValue || '');
        setSystemPromptDraft(result.data.llm.prompts.systemPrompt || '');
        setGreetingPromptDraft(result.data.llm.prompts.greetingPromptTemplate || '');
        setRagPromptDraft(result.data.llm.prompts.ragPromptTemplate || '');
        setNoContextPromptDraft(result.data.llm.prompts.noContextPromptTemplate || '');
        setToolFollowupPromptDraft(result.data.llm.prompts.toolFollowupPromptTemplate || '');
        setRuntimeDraft({
          temperature: result.data.llm.temperature,
          topP: result.data.llm.topP,
          topK: result.data.llm.topK,
          repeatPenalty: result.data.llm.repeatPenalty,
          maxTokens: result.data.llm.maxTokens,
          contextWindow: result.data.llm.contextWindow,
          timeoutSeconds: result.data.llm.timeoutSeconds,
        });
        setRouterDraft(result.data.llm.router);
        setToolOutputDraft(result.data.llm.toolOutput);
        setCustomToolsDraft(result.data.llm.customTools || []);
        // Show auth section if already configured
        if (result.data.llm.ollamaAuthHeaderName || result.data.llm.ollamaAuthHeaderValue) {
          setShowOllamaAuth(true);
        }
      } else {
        toast.error('Failed to load chat settings', {
          message: result.error,
        });
      }

      setIsLoadingChatSettings(false);
    };

    loadSettings();

    return () => {
      isActive = false;
    };
  }, []);

  useEffect(() => {
    void getActiveModel();
    void getActiveEmbeddingModel();
  }, [getActiveModel, getActiveEmbeddingModel]);

  useEffect(() => {
    if (!llmSettings) return;
    setSystemPromptDraft(llmSettings.prompts.systemPrompt || '');
    setGreetingPromptDraft(llmSettings.prompts.greetingPromptTemplate || '');
    setRagPromptDraft(llmSettings.prompts.ragPromptTemplate || '');
    setNoContextPromptDraft(llmSettings.prompts.noContextPromptTemplate || '');
    setToolFollowupPromptDraft(llmSettings.prompts.toolFollowupPromptTemplate || '');
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

  const saveLlmUpdates = async (updates: Partial<ApiLLMSettings>): Promise<boolean> => {
    if (!llmSettings) return false;

    const previous = llmSettings;
    const next = { ...llmSettings, ...updates };
    setLlmSettings(next);
    if (updates.ollamaUrl !== undefined) {
      setOllamaUrlDraft(next.ollamaUrl);
    }
    if (updates.model !== undefined) {
      setOllamaModelDraft(next.model);
    }
    if (updates.ollamaAuthHeaderName !== undefined) {
      setOllamaHeaderNameDraft(next.ollamaAuthHeaderName || '');
    }
    if (updates.ollamaAuthHeaderValue !== undefined) {
      setOllamaHeaderValueDraft(next.ollamaAuthHeaderValue || '');
    }

    const result = await VaultAPI.updateSettings({
      category: 'llm',
      updates,
    });

    if (!result.ok) {
      setLlmSettings(previous);
      setOllamaUrlDraft(previous.ollamaUrl);
      setOllamaModelDraft(previous.model);
      setOllamaHeaderNameDraft(previous.ollamaAuthHeaderName || '');
      setOllamaHeaderValueDraft(previous.ollamaAuthHeaderValue || '');
      toast.error('Failed to update chat settings', {
        message: result.error,
      });
      return false;
    }

    return true;
  };

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

  const provider = llmSettings?.provider ?? 'auto';
  const externalModelDirectories = llmSettings?.externalModelDirectories ?? [];
  const showOllamaSettings = provider === 'ollama' || provider === 'auto';
  const verificationEnabled = llmSettings?.verification?.enabled ?? true;

  useEffect(() => {
    if (selectedCatalogModel && downloadSectionRef.current) {
      setIsModelCatalogExpanded(true);
      downloadSectionRef.current.scrollIntoView({
        block: 'start',
        behavior: 'smooth',
      });
    }
  }, [selectedCatalogModel]);

  useEffect(() => {
    let isActive = true;

    const loadModelDownloadPath = async () => {
      setIsLoadingModelDownloadPath(true);
      const result = await VaultAPI.getModelDownloadPath();
      if (!isActive) return;
      setModelDownloadPath(result.ok ? result.data : '');
      setIsLoadingModelDownloadPath(false);
    };

    loadModelDownloadPath();

    return () => {
      isActive = false;
    };
  }, []);

  const handleVerificationToggle = async (enabled: boolean) => {
    if (!llmSettings) return;
    if ((llmSettings.verification?.enabled ?? true) === enabled) return;

    const ok = await saveLlmUpdates({
      verification: {
        ...(llmSettings.verification || { enabled: true }),
        enabled,
      },
    });

    if (ok) {
      toast.success(enabled ? 'Response verification enabled' : 'Response verification disabled');
    }
  };

  const handleApplyBasicAuth = () => {
    if (!ollamaBasicUserDraft.trim()) {
      toast.error('Basic auth username is required');
      return;
    }
    const token = btoa(`${ollamaBasicUserDraft}:${ollamaBasicPassDraft}`);
    saveLlmUpdates({
      ollamaAuthHeaderName: 'Authorization',
      ollamaAuthHeaderValue: `Basic ${token}`,
    });
  };

  const handleTestOllamaConnection = async () => {
    const ollamaUrl = ollamaUrlDraft.trim();
    const authHeaderName = ollamaHeaderNameDraft.trim();
    const authHeaderValue = ollamaHeaderValueDraft.trim();

    if (!ollamaUrl) {
      toast.error('Ollama URL is required');
      return;
    }

    if ((authHeaderName && !authHeaderValue) || (!authHeaderName && authHeaderValue)) {
      toast.error('Header name and value must both be set');
      return;
    }

    setIsTestingOllamaConnection(true);
    const result = await VaultAPI.testOllamaConnection({
      ollamaUrl,
      authHeaderName,
      authHeaderValue,
    });
    setIsTestingOllamaConnection(false);

    if (!result.ok) {
      setOllamaAvailableModels([]);
      setOllamaModelsEndpoint('');
      toast.error('Connection test failed', {
        message: result.error,
      });
      return;
    }

    setOllamaAvailableModels(result.data.models);
    setOllamaModelsEndpoint(result.data.endpoint);

    if (!result.data.models.includes(ollamaModelDraft)) {
      setOllamaModelDraft('');
    }

    toast.success(
      `Connected. Loaded ${result.data.models.length} model(s) from ${result.data.endpoint}.`
    );
  };

  const persistExternalDirectories = async (directories: string[]) => {
    if (!llmSettings) return;

    const normalized = Array.from(
      new Set(
        directories
          .map((dir) => dir.trim())
          .filter((dir) => dir.length > 0)
      )
    );

    await saveLlmUpdates({
      externalModelDirectories: normalized,
    });
  };

  const handleAddExternalDirectory = async () => {
    setIsAddingExternalDirectory(true);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Model Folder',
      });

      if (!selected || typeof selected !== 'string') {
        return;
      }

      if (externalModelDirectories.includes(selected)) {
        toast.info('Folder already added');
        return;
      }

      await persistExternalDirectories([...externalModelDirectories, selected]);
      toast.success('External model folder added');
    } catch (error) {
      toast.error('Failed to add external model folder', {
        message: String(error),
      });
    } finally {
      setIsAddingExternalDirectory(false);
    }
  };

  const handleRemoveExternalDirectory = async (path: string) => {
    await persistExternalDirectories(
      externalModelDirectories.filter((directory) => directory !== path)
    );
  };

  const inputClass =
    'w-full px-3 py-2 text-sm bg-[var(--bg-secondary)] text-[var(--text-primary)] border border-[var(--border-color)] rounded-lg focus:outline-none focus:ring-2 focus:ring-[var(--accent-primary)] focus:border-transparent';
  const textareaClass =
    'w-full rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] px-3 py-2 text-xs text-[var(--text-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--accent-primary)] focus:border-transparent';
  const toFinite = (value: string, fallback: number) => {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : fallback;
  };
  const clamp = (value: number, min: number, max: number) =>
    Math.min(max, Math.max(min, value));

  const savePromptField = async (
    key: keyof ApiLLMSettings['prompts'],
    value: string
  ) => {
    if (!llmSettings) return;
    const trimmed = value.trim();
    if (trimmed === llmSettings.prompts[key]) return;
    await saveLlmUpdates({
      prompts: {
        ...llmSettings.prompts,
        [key]: trimmed,
      },
    });
  };

  const normalizeCustomTool = (tool: ApiCustomToolSettings): ApiCustomToolSettings => {
    const maxResults = Number.isFinite(tool.defaultMaxResults)
      ? Math.max(1, Math.min(100, Math.round(tool.defaultMaxResults)))
      : 5;
    const maxResultsParam = (tool.maxResultsParam || '').trim();

    return {
      enabled: tool.enabled,
      name: tool.name.trim(),
      description: tool.description.trim(),
      endpoint: tool.endpoint.trim(),
      queryParam: tool.queryParam.trim(),
      maxResultsParam: maxResultsParam.length > 0 ? maxResultsParam : null,
      defaultMaxResults: maxResults,
    };
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
        defaultPath: 'recall-custom-tools.json',
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

  const customToolsDirty = (() => {
    const normalizeAll = (tools: ApiCustomToolSettings[]) => tools.map(normalizeCustomTool);
    return (
      JSON.stringify(normalizeAll(customToolsDraft)) !==
      JSON.stringify(normalizeAll(llmSettings?.customTools || []))
    );
  })();

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center gap-3 pb-4 border-b border-[var(--border-color)]">
        <div className="p-2 bg-[var(--accent-light)] rounded-lg">
          <Brain className="w-5 h-5 text-[var(--accent-primary)]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[var(--text-primary)]">AI Settings</h2>
          <p className="text-sm text-[var(--text-secondary)]">
            Configure chat and downloadable local models
          </p>
        </div>
      </div>

      <div className="rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-3">
        <div className="mb-2 text-xs font-medium text-[var(--text-secondary)]">
          Quick Jump
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => downloadSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })}
            className="rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2.5 py-1 text-xs text-[var(--text-primary)] hover:border-[var(--border-hover)]"
          >
            Download Models
          </button>
          <button
            type="button"
            onClick={() => tuningSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })}
            className="rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2.5 py-1 text-xs text-[var(--text-primary)] hover:border-[var(--border-hover)]"
          >
            Tuning & Layers
          </button>
          <button
            type="button"
            onClick={() =>
              verificationSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })
            }
            className="rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2.5 py-1 text-xs text-[var(--text-primary)] hover:border-[var(--border-hover)]"
          >
            Verification
          </button>
          <button
            type="button"
            onClick={() => toolsSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })}
            className="rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2.5 py-1 text-xs text-[var(--text-primary)] hover:border-[var(--border-hover)]"
          >
            Tool Integrations
          </button>
        </div>
      </div>

      {/* ──────────────────────────────────────────────────────────────────────
          Section 1: Chat Provider
          ────────────────────────────────────────────────────────────────────── */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Server className="w-4 h-4 text-[var(--accent-primary)]" />
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">Chat Provider</h3>
        </div>

        {isLoadingChatSettings ? (
          <div className="text-xs text-[var(--text-tertiary)]">Loading chat settings...</div>
        ) : !llmSettings ? (
          <div className="text-xs text-[var(--text-tertiary)]">
            Unable to load chat settings right now.
          </div>
        ) : (
          <div className="space-y-4">
            {/* Provider radio cards */}
            <div className="grid grid-cols-3 gap-3">
              {[
                { value: 'auto', label: 'Auto', desc: 'Local + Ollama fallback' },
                { value: 'local', label: 'Local Only', desc: 'Downloaded models' },
                { value: 'ollama', label: 'Ollama', desc: 'Remote server' },
              ].map((option) => (
                <label
                  key={option.value}
                  className={`p-3 rounded-lg border-2 transition-all text-center cursor-pointer ${
                    provider === option.value
                      ? 'border-[var(--accent-primary)] bg-[var(--accent-light)]'
                      : 'border-[var(--border-color)] hover:border-[var(--border-hover)] bg-[var(--surface-elevated)]'
                  }`}
                >
                  <input
                    type="radio"
                    name="chatProvider"
                    value={option.value}
                    checked={provider === option.value}
                    onChange={() =>
                      saveLlmUpdates({ provider: option.value as ApiLLMSettings['provider'] })
                    }
                    className="sr-only"
                  />
                  <div className="font-medium text-sm text-[var(--text-primary)]">
                    {option.label}
                  </div>
                  <div className="text-xs text-[var(--text-secondary)] mt-0.5">{option.desc}</div>
                </label>
              ))}
            </div>

            {/* Ollama configuration */}
            {showOllamaSettings && (
              <div className="p-4 bg-[var(--bg-secondary)] rounded-lg space-y-4">
                {/* URL + Model row */}
                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1.5">
                    <label
                      htmlFor="ollamaUrl"
                      className="block text-xs font-medium text-[var(--text-secondary)]"
                    >
                      Server URL
                    </label>
                    <input
                      id="ollamaUrl"
                      type="text"
                      value={ollamaUrlDraft}
                      onChange={(e) => setOllamaUrlDraft(e.target.value)}
                      onBlur={() => {
                        if (ollamaUrlDraft !== llmSettings.ollamaUrl) {
                          saveLlmUpdates({ ollamaUrl: ollamaUrlDraft.trim() });
                        }
                      }}
                      placeholder="http://localhost:11434"
                      className={inputClass}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <label
                      htmlFor="ollamaModel"
                      className="block text-xs font-medium text-[var(--text-secondary)]"
                    >
                      Model
                    </label>
                    <select
                      id="ollamaModel"
                      value={ollamaModelDraft}
                      onChange={(e) => {
                        const selectedModel = e.target.value;
                        setOllamaModelDraft(selectedModel);
                        if (selectedModel && selectedModel !== llmSettings.model) {
                          saveLlmUpdates({ model: selectedModel });
                        }
                      }}
                      disabled={ollamaAvailableModels.length === 0}
                      className={inputClass}
                    >
                      <option value="">
                        {ollamaAvailableModels.length === 0
                          ? 'Test connection first'
                          : 'Select a model'}
                      </option>
                      {ollamaAvailableModels.map((modelName) => (
                        <option key={modelName} value={modelName}>
                          {modelName}
                        </option>
                      ))}
                    </select>
                  </div>
                </div>

                {/* Test connection + auth toggle row */}
                <div className="flex items-center gap-3">
                  <button
                    type="button"
                    onClick={handleTestOllamaConnection}
                    disabled={isTestingOllamaConnection}
                    className="px-3 py-1.5 text-xs font-medium rounded-lg bg-[var(--accent-primary)] text-white hover:opacity-90 disabled:opacity-60 disabled:cursor-not-allowed transition-opacity"
                  >
                    {isTestingOllamaConnection ? 'Testing...' : 'Test Connection'}
                  </button>
                  {ollamaModelsEndpoint && (
                    <span className="text-xs text-[var(--text-tertiary)]">
                      {ollamaAvailableModels.length} model(s) from {ollamaModelsEndpoint}
                    </span>
                  )}
                  <button
                    type="button"
                    onClick={() => setShowOllamaAuth(!showOllamaAuth)}
                    className="ml-auto text-xs text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] transition-colors"
                  >
                    {showOllamaAuth ? 'Hide auth' : 'Authentication...'}
                  </button>
                </div>

                {/* Auth section (collapsible) */}
                {showOllamaAuth && (
                  <div className="pt-3 border-t border-[var(--border-color)] space-y-3">
                    <div className="text-xs font-medium text-[var(--text-secondary)]">
                      Security Header
                    </div>
                    <div className="grid grid-cols-2 gap-3">
                      <input
                        type="text"
                        value={ollamaHeaderNameDraft}
                        onChange={(e) => setOllamaHeaderNameDraft(e.target.value)}
                        onBlur={() => {
                          if (ollamaHeaderNameDraft !== llmSettings.ollamaAuthHeaderName) {
                            saveLlmUpdates({
                              ollamaAuthHeaderName: ollamaHeaderNameDraft.trim(),
                            });
                          }
                        }}
                        placeholder="Header name (e.g. Authorization)"
                        className={inputClass}
                      />
                      <input
                        type="text"
                        value={ollamaHeaderValueDraft}
                        onChange={(e) => setOllamaHeaderValueDraft(e.target.value)}
                        onBlur={() => {
                          if (ollamaHeaderValueDraft !== llmSettings.ollamaAuthHeaderValue) {
                            saveLlmUpdates({
                              ollamaAuthHeaderValue: ollamaHeaderValueDraft.trim(),
                            });
                          }
                        }}
                        placeholder="Header value (e.g. Bearer ...)"
                        className={inputClass}
                      />
                    </div>

                    <div className="text-xs font-medium text-[var(--text-secondary)] pt-1">
                      Basic Auth Helper
                    </div>
                    <div className="flex items-end gap-3">
                      <div className="flex-1">
                        <input
                          type="text"
                          value={ollamaBasicUserDraft}
                          onChange={(e) => setOllamaBasicUserDraft(e.target.value)}
                          placeholder="Username"
                          className={inputClass}
                        />
                      </div>
                      <div className="flex-1">
                        <input
                          type="password"
                          value={ollamaBasicPassDraft}
                          onChange={(e) => setOllamaBasicPassDraft(e.target.value)}
                          placeholder="Password"
                          className={inputClass}
                        />
                      </div>
                      <button
                        type="button"
                        onClick={handleApplyBasicAuth}
                        className="px-3 py-2 text-xs font-medium rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] text-[var(--text-primary)] hover:border-[var(--border-hover)] whitespace-nowrap"
                      >
                        Set Header
                      </button>
                    </div>
                    <p className="text-xs text-[var(--text-tertiary)]">
                      Generates an Authorization header from your credentials. Your password is not
                      stored separately.
                    </p>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </section>

      {/* ──────────────────────────────────────────────────────────────────────
          Section 1.5: Active Models
          ────────────────────────────────────────────────────────────────────── */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Brain className="w-4 h-4 text-[var(--accent-primary)]" />
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">Active Models</h3>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          <div className="p-3 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)]">
            <div className="text-xs font-medium text-[var(--text-tertiary)]">Chat LLM</div>
            <div className="text-sm text-[var(--text-primary)] mt-1">
              {provider === 'ollama'
                ? (llmSettings?.model ? `${llmSettings.model} (Ollama)` : 'Not set')
                : (activeChatModel?.model_name ?? 'Not set')}
            </div>
            {provider !== 'ollama' && activeChatModel?.model_id && (
              <div className="text-xs text-[var(--text-tertiary)] mt-1">
                {activeChatModel.model_id}
              </div>
            )}
          </div>

          <div className="p-3 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)]">
            <div className="text-xs font-medium text-[var(--text-tertiary)]">Router Model</div>
            <div className="text-sm text-[var(--text-primary)] mt-1">
              {llmSettings?.router?.model ?? 'Not set'}
            </div>
            <div className="text-xs text-[var(--text-tertiary)] mt-1">
              Required for follow-up routing
            </div>
          </div>

          <div className="p-3 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)]">
            <div className="text-xs font-medium text-[var(--text-tertiary)]">Embedding Model</div>
            <div className="text-sm text-[var(--text-primary)] mt-1">
              {activeEmbeddingModel?.model_name ?? 'Not set'}
            </div>
            {activeEmbeddingModel?.model_id && (
              <div className="text-xs text-[var(--text-tertiary)] mt-1">
                {activeEmbeddingModel.model_id}
              </div>
            )}
          </div>
        </div>
      </section>

      {/* ──────────────────────────────────────────────────────────────────────
          Section 2: Download Models
          ────────────────────────────────────────────────────────────────────── */}
      <section className="space-y-4" ref={downloadSectionRef}>
        <div className="flex items-center gap-2">
          <Download className="w-4 h-4 text-[var(--accent-primary)]" />
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">Download Models</h3>
        </div>

        <div className="rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4 space-y-2">
          <div className="text-xs font-medium text-[var(--text-secondary)]">Model Storage</div>
          {isLoadingModelDownloadPath ? (
            <p className="text-xs text-[var(--text-tertiary)]">Loading model folder...</p>
          ) : modelDownloadPath ? (
            <code className="block rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-2 text-xs text-[var(--text-primary)]">
              {modelDownloadPath}
            </code>
          ) : (
            <p className="text-xs text-[var(--text-tertiary)]">
              Unable to read local model folder path right now.
            </p>
          )}
          <p className="text-xs text-[var(--text-tertiary)]">
            This location is always available, even if your current chat provider is Ollama.
          </p>
        </div>

        <div className="rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4 space-y-3">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h4 className="text-sm font-semibold text-[var(--text-primary)]">
                External Model Folders
              </h4>
              <p className="text-xs text-[var(--text-secondary)] mt-1">
                Reuse models from LM Studio or other local directories.
              </p>
            </div>
            <button
              type="button"
              onClick={handleAddExternalDirectory}
              disabled={isAddingExternalDirectory}
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-lg bg-[var(--accent-primary)] text-white hover:opacity-90 disabled:opacity-60 disabled:cursor-not-allowed transition-opacity whitespace-nowrap"
            >
              <FolderPlus className="w-3.5 h-3.5" />
              {isAddingExternalDirectory ? 'Adding...' : 'Add Folder'}
            </button>
          </div>

          {externalModelDirectories.length === 0 ? (
            <p className="text-xs text-[var(--text-tertiary)]">
              No external model folders configured.
            </p>
          ) : (
            <div className="space-y-2">
              {externalModelDirectories.map((directory) => (
                <div
                  key={directory}
                  className="flex items-center justify-between gap-2 px-3 py-2 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)]"
                >
                  <code className="text-xs text-[var(--text-primary)] truncate">{directory}</code>
                  <button
                    type="button"
                    onClick={() => handleRemoveExternalDirectory(directory)}
                    className="p-1 text-[var(--text-secondary)] hover:text-[var(--error)] transition-colors"
                    aria-label={`Remove external folder ${directory}`}
                    title="Remove folder"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}

          <p className="text-xs text-[var(--text-tertiary)]">
            Scans these folders for `.gguf` and `.onnx` files. Removing an external model entry
            from Recall does not delete the original file.
          </p>
        </div>

        {(provider === 'auto' || provider === 'local') ? (
          <div className="rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4 space-y-3">
            <div className="flex items-start justify-between gap-3">
              <div>
                <h4 className="text-sm font-semibold text-[var(--text-primary)]">
                  Browse Model Catalog
                </h4>
                <p className="text-xs text-[var(--text-secondary)] mt-1">
                  Browse public Hugging Face models. Expand only when needed.
                </p>
              </div>
              <button
                type="button"
                onClick={() => setIsModelCatalogExpanded((previous) => !previous)}
                className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2.5 py-1.5 text-xs font-medium text-[var(--text-primary)] hover:border-[var(--border-hover)]"
              >
                {isModelCatalogExpanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                {isModelCatalogExpanded ? 'Collapse' : 'Expand'}
              </button>
            </div>

            {isModelCatalogExpanded ? (
              <ModelCatalogBrowser
                routerModelId={llmSettings?.router?.model}
                onSetRouterModel={async (modelId) => {
                  if (!llmSettings) return;
                  await saveLlmUpdates({
                    router: {
                      ...llmSettings.router,
                      model: modelId,
                    },
                  });
                }}
              />
            ) : (
              <p className="text-xs text-[var(--text-tertiary)]">
                Catalog hidden to keep this page compact. Expand when you want to search or download models.
              </p>
            )}
          </div>
        ) : (
          <div className="rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4 space-y-2">
            <p className="text-xs text-[var(--text-secondary)]">
              You are currently using Ollama. Switch to <strong>Auto</strong> or <strong>Local Only</strong> to browse and download local models.
            </p>
          </div>
        )}
      </section>

      {/* ──────────────────────────────────────────────────────────────────────
          Section 3: Prompting + Verification
          ────────────────────────────────────────────────────────────────────── */}
      <section className="space-y-4" ref={verificationSectionRef}>
        <div className="flex items-center gap-2">
          <Settings2 className="w-4 h-4 text-[var(--accent-primary)]" />
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">
            Prompting & Verification
          </h3>
        </div>

        {isLoadingChatSettings || !llmSettings ? (
          <div className="text-xs text-[var(--text-tertiary)]">Loading prompt settings...</div>
        ) : (
          <div className="space-y-3 rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4">
            <div className="space-y-3 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <div className="text-sm font-medium text-[var(--text-primary)]">
                    Response Verification
                  </div>
                  <p className="mt-0.5 text-xs text-[var(--text-secondary)]">
                    Show verified and unverified claims in assistant messages.
                  </p>
                </div>
                <span
                  className={`inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[11px] ${
                    verificationEnabled
                      ? 'border-emerald-400/35 bg-emerald-500/10 text-emerald-200'
                      : 'border-white/20 bg-white/5 text-white/70'
                  }`}
                >
                  {verificationEnabled ? (
                    <ShieldCheck className="h-3.5 w-3.5" />
                  ) : (
                    <ShieldOff className="h-3.5 w-3.5" />
                  )}
                  {verificationEnabled ? 'On' : 'Off'}
                </span>
              </div>

              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => void handleVerificationToggle(true)}
                  className={`rounded-md border px-3 py-1.5 text-xs font-medium transition-colors ${
                    verificationEnabled
                      ? 'border-[var(--accent-primary)] bg-[var(--accent-light)] text-[var(--text-primary)]'
                      : 'border-[var(--border-color)] bg-[var(--bg-secondary)] text-[var(--text-secondary)] hover:border-[var(--border-hover)]'
                  }`}
                >
                  Turn On
                </button>
                <button
                  type="button"
                  onClick={() => void handleVerificationToggle(false)}
                  className={`rounded-md border px-3 py-1.5 text-xs font-medium transition-colors ${
                    !verificationEnabled
                      ? 'border-[var(--accent-primary)] bg-[var(--accent-light)] text-[var(--text-primary)]'
                      : 'border-[var(--border-color)] bg-[var(--bg-secondary)] text-[var(--text-secondary)] hover:border-[var(--border-hover)]'
                  }`}
                >
                  Turn Off
                </button>
              </div>

              <button
                type="button"
                onClick={() => setIsVerificationHelpExpanded((previous) => !previous)}
                className="inline-flex items-center gap-1 text-xs text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]"
              >
                {isVerificationHelpExpanded ? (
                  <ChevronDown className="w-3.5 h-3.5" />
                ) : (
                  <ChevronRight className="w-3.5 h-3.5" />
                )}
                {isVerificationHelpExpanded ? 'Hide details' : 'What changes when this is off?'}
              </button>

              {isVerificationHelpExpanded && (
                <ul className="space-y-1 text-xs text-[var(--text-tertiary)]">
                  <li>Chat messages won&apos;t include verification summaries.</li>
                  <li>Badges and verified/unverified claim details are hidden.</li>
                  <li>Turn it back on any time from this panel.</li>
                </ul>
              )}
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[var(--text-secondary)]">
                System Prompt
              </label>
              <textarea
                value={systemPromptDraft}
                onChange={(e) => setSystemPromptDraft(e.target.value)}
                onBlur={() => void savePromptField('systemPrompt', systemPromptDraft)}
                rows={3}
                className={textareaClass}
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[var(--text-secondary)]">
                Greeting Prompt Template
              </label>
              <textarea
                value={greetingPromptDraft}
                onChange={(e) => setGreetingPromptDraft(e.target.value)}
                onBlur={() => void savePromptField('greetingPromptTemplate', greetingPromptDraft)}
                rows={3}
                className={textareaClass}
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[var(--text-secondary)]">
                RAG Prompt Template
              </label>
              <textarea
                value={ragPromptDraft}
                onChange={(e) => setRagPromptDraft(e.target.value)}
                onBlur={() => void savePromptField('ragPromptTemplate', ragPromptDraft)}
                rows={5}
                className={textareaClass}
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[var(--text-secondary)]">
                No-Context Prompt Template
              </label>
              <textarea
                value={noContextPromptDraft}
                onChange={(e) => setNoContextPromptDraft(e.target.value)}
                onBlur={() =>
                  void savePromptField('noContextPromptTemplate', noContextPromptDraft)
                }
                rows={4}
                className={textareaClass}
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-medium text-[var(--text-secondary)]">
                Tool Follow-up Prompt Template
              </label>
              <textarea
                value={toolFollowupPromptDraft}
                onChange={(e) => setToolFollowupPromptDraft(e.target.value)}
                onBlur={() =>
                  void savePromptField('toolFollowupPromptTemplate', toolFollowupPromptDraft)
                }
                rows={4}
                className={textareaClass}
              />
            </div>
          </div>
        )}
      </section>

      {/* ──────────────────────────────────────────────────────────────────────
          Section 4: Tuning + Layers
          ────────────────────────────────────────────────────────────────────── */}
      <section className="space-y-4" ref={tuningSectionRef}>
        <div className="flex items-center gap-2">
          <Settings2 className="w-4 h-4 text-[var(--accent-primary)]" />
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">Tuning & Layers</h3>
        </div>

        {isLoadingChatSettings ||
        !llmSettings ||
        !routerDraft ||
        !toolOutputDraft ? (
          <div className="text-xs text-[var(--text-tertiary)]">
            Loading tuning and layer settings...
          </div>
        ) : (
          <div className="space-y-4 rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4">
            <div className="space-y-3 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3">
              <div className="text-sm font-medium text-[var(--text-primary)]">Model Runtime</div>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">Top P</label>
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">Top K</label>
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="flex items-end">
                  <label className="inline-flex items-center gap-2 text-xs text-[var(--text-secondary)]">
                    <input
                      type="checkbox"
                      checked={llmSettings.streamResponses}
                      onChange={(event) =>
                        void saveLlmUpdates({ streamResponses: event.target.checked })
                      }
                      className="h-4 w-4 rounded border-[var(--border-color)] bg-[var(--bg-tertiary)] text-[var(--accent-primary)] focus:ring-2 focus:ring-[var(--accent-primary)]"
                    />
                    Stream Responses
                  </label>
                </div>
              </div>
            </div>

            <div className="space-y-3 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3">
              <div className="text-sm font-medium text-[var(--text-primary)]">Retrieval Tuning</div>
              <p className="text-xs text-[var(--text-secondary)]">
                Retrieval knobs moved to the Search tab. Use Settings → Search → Advanced
                Retrieval Tuning for reranker, shortlist, overlap, and support-ratio controls.
              </p>
            </div>

            <div className="space-y-3 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3">
              <div className="text-sm font-medium text-[var(--text-primary)]">Router Layer</div>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <label className="inline-flex items-center gap-2 text-xs text-[var(--text-secondary)]">
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
                    className="h-4 w-4 rounded border-[var(--border-color)] bg-[var(--bg-tertiary)] text-[var(--accent-primary)] focus:ring-2 focus:ring-[var(--accent-primary)]"
                  />
                  Router Enabled
                </label>
                <label className="inline-flex items-center gap-2 text-xs text-[var(--text-secondary)]">
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
                    className="h-4 w-4 rounded border-[var(--border-color)] bg-[var(--bg-tertiary)] text-[var(--accent-primary)] focus:ring-2 focus:ring-[var(--accent-primary)]"
                  />
                  Prefer Last Document
                </label>
                <div className="space-y-1.5 md:col-span-2">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                  className={textareaClass}
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                  className={textareaClass}
                />
              </div>
            </div>

            <div className="space-y-3 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3">
              <div className="text-sm font-medium text-[var(--text-primary)]">
                Tool Output Layer
              </div>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                    className={inputClass}
                  />
                </div>
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                  className={textareaClass}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                  className={textareaClass}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                  className={textareaClass}
                />
              </div>
            </div>
          </div>
        )}
      </section>

      {/* ──────────────────────────────────────────────────────────────────────
          Section 5: Tool Integrations
          ────────────────────────────────────────────────────────────────────── */}
      <section className="space-y-4" ref={toolsSectionRef}>
        <div className="flex items-center gap-2">
          <Settings2 className="w-4 h-4 text-[var(--accent-primary)]" />
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">Tool Integrations</h3>
        </div>

        {isLoadingChatSettings || !llmSettings ? (
          <div className="text-xs text-[var(--text-tertiary)]">Loading tools...</div>
        ) : (
          <div className="space-y-3 rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4">
            <div className="space-y-2">
              <div className="text-xs font-medium text-[var(--text-secondary)]">Built-in tools</div>
              <div className="flex flex-wrap gap-2">
                {['web_search', 'fetch_url_content', 'wiki_search', 'wiki_summary'].map(
                  (toolName) => (
                    <span
                      key={toolName}
                      className="inline-flex items-center rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2 py-1 text-[11px] text-[var(--text-secondary)]"
                    >
                      {toolName}
                    </span>
                  )
                )}
              </div>
            </div>

            <div className="border-t border-[var(--border-color)] pt-3 space-y-3">
              <div className="rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 space-y-3">
                <div className="space-y-1">
                  <div className="text-xs font-semibold text-[var(--text-primary)]">
                    Preset Catalog (Templates)
                  </div>
                  <p className="text-xs text-[var(--text-secondary)]">
                    Curated starter presets for common public APIs and self-hosted search. This
                    list is intentionally small and not exhaustive.
                  </p>
                  <p className="text-[11px] text-[var(--text-tertiary)]">
                    Select a preset, then click <span className="font-medium">Add Preset as Tool</span>.
                    This copies the template into your saved custom tools list below. Selecting a
                    preset alone does not change any saved tool.
                  </p>
                </div>
                <div className="rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] p-2.5">
                  <div className="flex items-center justify-between gap-2">
                    <button
                      type="button"
                      onClick={() => cyclePreset(-1)}
                      className="rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2 py-1 text-[11px] text-[var(--text-primary)] hover:border-[var(--border-hover)]"
                    >
                      Previous
                    </button>
                    <div className="text-[11px] text-[var(--text-secondary)]">
                      Preset {selectedPresetIndex + 1} of {CUSTOM_TOOL_PRESETS.length}
                    </div>
                    <button
                      type="button"
                      onClick={() => cyclePreset(1)}
                      className="rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2 py-1 text-[11px] text-[var(--text-primary)] hover:border-[var(--border-hover)]"
                    >
                      Next
                    </button>
                  </div>
                </div>
                <div className="rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] p-2.5 text-xs text-[var(--text-secondary)]">
                  <div className="font-medium text-[var(--text-primary)]">
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
                      className="mt-1 inline-block text-[var(--accent-primary)] hover:underline"
                    >
                      Open preset docs
                    </a>
                  )}
                  {selectedPreset.id === 'searxng_search' && (
                    <p className="mt-1 text-[var(--text-tertiary)]">
                      Replace the endpoint with your deployed SearXNG URL
                      (`https://your-domain/search?format=json`). Localhost/private-network
                      endpoints are blocked by default security policy.
                    </p>
                  )}
                </div>
              </div>

              <div className="flex items-center justify-between gap-3">
                <div>
                  <h4 className="text-sm font-semibold text-[var(--text-primary)]">Saved Custom Tools</h4>
                  <p className="text-xs text-[var(--text-secondary)] mt-0.5">
                    These are the tools chat can actually use after you click Save Tools.
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => void importCustomToolsFromJson()}
                    disabled={isImportingCustomTools}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2.5 py-1.5 text-xs font-medium text-[var(--text-primary)] hover:border-[var(--border-hover)] disabled:cursor-not-allowed disabled:opacity-60"
                  >
                    {isImportingCustomTools ? 'Importing...' : 'Import JSON'}
                  </button>
                  <button
                    type="button"
                    onClick={() => void exportCustomToolsToJson()}
                    disabled={isExportingCustomTools || customToolsDraft.length === 0}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2.5 py-1.5 text-xs font-medium text-[var(--text-primary)] hover:border-[var(--border-hover)] disabled:cursor-not-allowed disabled:opacity-60"
                  >
                    {isExportingCustomTools ? 'Exporting...' : 'Export JSON'}
                  </button>
                  <button
                    type="button"
                    onClick={addPresetTool}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2.5 py-1.5 text-xs font-medium text-[var(--text-primary)] hover:border-[var(--border-hover)]"
                  >
                    <Plus className="w-3.5 h-3.5" />
                    Add Preset as Tool
                  </button>
                  <button
                    type="button"
                    onClick={addCustomTool}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2.5 py-1.5 text-xs font-medium text-[var(--text-primary)] hover:border-[var(--border-hover)]"
                  >
                    <Plus className="w-3.5 h-3.5" />
                    Add Tool
                  </button>
                </div>
              </div>

              <div className="space-y-1.5">
                <label className="text-xs font-medium text-[var(--text-secondary)]">
                  Test Query
                </label>
                <input
                  type="text"
                  value={toolTestQuery}
                  onChange={(event) => setToolTestQuery(event.target.value)}
                  placeholder="Enter a query used when testing tools"
                  className={inputClass}
                />
              </div>

              {customToolsDraft.length === 0 ? (
                <p className="text-xs text-[var(--text-tertiary)]">
                  No custom tools configured. Add one to expose a custom search endpoint to the
                  model.
                </p>
              ) : (
                <div className="space-y-3">
                  {customToolsDraft.map((tool, index) => (
                    <div
                      key={`${tool.name || 'custom_tool'}_${index}`}
                      className="rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 space-y-3"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <label className="inline-flex items-center gap-2 text-xs text-[var(--text-secondary)]">
                          <input
                            type="checkbox"
                            checked={tool.enabled}
                            onChange={(event) =>
                              updateCustomTool(index, 'enabled', event.target.checked)
                            }
                            className="h-4 w-4 rounded border-[var(--border-color)] bg-[var(--bg-tertiary)] text-[var(--accent-primary)] focus:ring-2 focus:ring-[var(--accent-primary)]"
                          />
                          Enabled
                        </label>
                        <div className="flex items-center gap-2">
                          <button
                            type="button"
                            onClick={() => void testCustomTool(tool, index)}
                            disabled={activeToolTestKey === `${tool.name || 'custom_tool'}_${index}`}
                            className="inline-flex items-center gap-1 rounded-md border border-[var(--border-color)] px-2 py-1 text-xs text-[var(--text-secondary)] hover:border-[var(--border-hover)] hover:text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-60"
                          >
                            {activeToolTestKey === `${tool.name || 'custom_tool'}_${index}`
                              ? 'Testing...'
                              : 'Test'}
                          </button>
                          <button
                            type="button"
                            onClick={() => removeCustomTool(index)}
                            className="inline-flex items-center gap-1 text-xs text-[var(--text-secondary)] hover:text-[var(--error)]"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                            Remove
                          </button>
                        </div>
                      </div>

                      <div className="grid grid-cols-2 gap-3">
                        <div className="space-y-1.5">
                          <label className="text-xs font-medium text-[var(--text-secondary)]">
                            Tool Name
                          </label>
                          <input
                            type="text"
                            value={tool.name}
                            onChange={(event) =>
                              updateCustomTool(index, 'name', event.target.value)
                            }
                            placeholder="pubmed_search"
                            className={inputClass}
                          />
                        </div>
                        <div className="space-y-1.5">
                          <label className="text-xs font-medium text-[var(--text-secondary)]">
                            Query Param
                          </label>
                          <input
                            type="text"
                            value={tool.queryParam}
                            onChange={(event) =>
                              updateCustomTool(index, 'queryParam', event.target.value)
                            }
                            placeholder="q"
                            className={inputClass}
                          />
                        </div>
                      </div>

                      <div className="space-y-1.5">
                        <label className="text-xs font-medium text-[var(--text-secondary)]">
                          Description
                        </label>
                        <input
                          type="text"
                          value={tool.description}
                          onChange={(event) =>
                            updateCustomTool(index, 'description', event.target.value)
                          }
                          placeholder="Search public medical abstracts"
                          className={inputClass}
                        />
                      </div>

                      <div className="space-y-1.5">
                        <label className="text-xs font-medium text-[var(--text-secondary)]">
                          Endpoint
                        </label>
                        <input
                          type="url"
                          value={tool.endpoint}
                          onChange={(event) =>
                            updateCustomTool(index, 'endpoint', event.target.value)
                          }
                          placeholder="https://example.org/search"
                          className={inputClass}
                        />
                      </div>

                      <div className="grid grid-cols-2 gap-3">
                        <div className="space-y-1.5">
                          <label className="text-xs font-medium text-[var(--text-secondary)]">
                            Max Results Param (optional)
                          </label>
                          <input
                            type="text"
                            value={tool.maxResultsParam || ''}
                            onChange={(event) =>
                              updateCustomTool(index, 'maxResultsParam', event.target.value)
                            }
                            placeholder="limit"
                            className={inputClass}
                          />
                        </div>
                        <div className="space-y-1.5">
                          <label className="text-xs font-medium text-[var(--text-secondary)]">
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
                            className={inputClass}
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
                  className="rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-1.5 text-xs font-medium text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-50"
                >
                  Reset
                </button>
                <button
                  type="button"
                  onClick={persistCustomTools}
                  disabled={!customToolsDirty || isSavingCustomTools}
                  className="rounded-lg bg-[var(--accent-primary)] px-3 py-1.5 text-xs font-medium text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {isSavingCustomTools ? 'Saving...' : 'Save Tools'}
                </button>
              </div>

              <p className="text-xs text-[var(--text-tertiary)]">
                Tool names must use only letters, numbers, and underscores.
              </p>
              <p className="text-xs text-[var(--text-tertiary)]">
                Preset = template only. Tool = saved runnable integration.
              </p>
              <p className="text-xs text-[var(--text-tertiary)]">
                Presets are convenience defaults. Some providers apply rate limits or anti-bot
                protection and may intermittently fail.
              </p>
            </div>
          </div>
        )}
      </section>

      {/* ──────────────────────────────────────────────────────────────────────
          Section 4: Options (Performance + Advanced)
          ────────────────────────────────────────────────────────────────────── */}
      <section className="space-y-4">
        <div className="flex items-center gap-2">
          <Settings2 className="w-4 h-4 text-[var(--accent-primary)]" />
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">Options</h3>
        </div>

        <div className="p-4 bg-[var(--bg-secondary)] rounded-lg space-y-4">
          {/* Quantization */}
          <div className="flex items-start gap-3">
            <input
              id="useQuantization"
              type="checkbox"
              checked={aiSettings.useQuantization}
              onChange={(e) => updateAI({ useQuantization: e.target.checked })}
              className="mt-0.5 w-4 h-4 text-[var(--accent-primary)] bg-[var(--bg-tertiary)] border-[var(--border-color)] rounded focus:ring-2 focus:ring-[var(--accent-primary)]"
            />
            <label htmlFor="useQuantization" className="flex-1 cursor-pointer">
              <div className="text-sm font-medium text-[var(--text-primary)]">Use Quantization</div>
              <p className="text-xs text-[var(--text-secondary)] mt-0.5">
                Reduce model size and improve speed with minimal quality loss.
              </p>
            </label>
          </div>

          <div className="border-t border-[var(--border-color)]" />

          {/* Agentic RAG */}
          <div className="flex items-start gap-3">
            <input
              id="enableAgenticRAG"
              type="checkbox"
              checked={aiSettings.enableAgenticRAG}
              onChange={(e) => updateAI({ enableAgenticRAG: e.target.checked })}
              className="mt-0.5 w-4 h-4 text-[var(--accent-primary)] bg-[var(--bg-tertiary)] border-[var(--border-color)] rounded focus:ring-2 focus:ring-[var(--accent-primary)]"
            />
            <label htmlFor="enableAgenticRAG" className="flex-1 cursor-pointer">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium text-[var(--text-primary)]">
                  Agentic RAG
                </span>
                <span className="text-xs bg-[var(--warning-light)] text-[var(--warning)] px-1.5 py-0.5 rounded">
                  Experimental
                </span>
              </div>
              <p className="text-xs text-[var(--text-secondary)] mt-0.5">
                AI agents autonomously refine searches and gather information. May increase
                processing time.
              </p>
            </label>
          </div>
        </div>
      </section>
    </div>
  );
}
