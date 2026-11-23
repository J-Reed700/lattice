/**
 * Settings domain architecture, legacy `?tab=` mapping, and instant search index.
 *
 * Consolidates all 16 Settings tabs into 6 minimalist sidebar domains while
 * keeping every single tab, setting, and `?tab=<legacyTab>` deep link 100%
 * intact and searchable.
 */

import type { ComponentType } from 'react';

import {
  Boxes,
  FolderOpen,
  GraduationCap,
  Palette,
  Search,
  SlidersHorizontal,
  type LucideIcon,
} from 'lucide-react';

import { AIModelsTab } from './AIModelsTab';
import { ChatTab, ModelsTab, PromptsTab, ToolsTab, TuningTab } from './AITab';
import { DisplayTab } from './DisplayTab';
import { IndexingTab } from './IndexingTab';
import { LogsTab } from './LogsTab';
import { PrivacyTab } from './PrivacyTab';
import { ReadAloudTab } from './ReadAloudTab';
import { SearchTab } from './SearchTab';
import { SourcesTab } from './SourcesTab';
import { SourceTrustSettings } from './SourceTrustSettings';
import { SpacesTab } from './SpacesTab';
import { VaultTab } from './VaultTab';

export type SettingsTab =
  | 'search'
  | 'indexing'
  | 'vault'
  | 'spaces'
  | 'chat'
  | 'models'
  | 'downloaded-models'
  | 'prompts'
  | 'tuning'
  | 'tools'
  | 'source-trust'
  | 'sources'
  | 'display'
  | 'read-aloud'
  | 'privacy'
  | 'logs';

export type SettingsDomainId =
  | 'general'
  | 'models-ai'
  | 'search-indexing'
  | 'storage-sync'
  | 'learning-studio'
  | 'system-diagnostics';

export interface SettingsSubTab {
  id: SettingsTab;
  domainId: SettingsDomainId;
  label: string;
  description: string;
  component: ComponentType;
  wide?: boolean;
}

export interface SettingsDomain {
  id: SettingsDomainId;
  label: string;
  description: string;
  icon: LucideIcon;
  tabs: SettingsSubTab[];
}

export interface SearchableSetting {
  id: string;
  tabId: SettingsTab;
  domainId: SettingsDomainId;
  section: string;
  label: string;
  description: string;
  keywords: string[];
  /** Optional DOM element id to scroll to and highlight after jumping. */
  elementId?: string;
}

export const SETTINGS_DOMAINS: SettingsDomain[] = [
  {
    id: 'general',
    label: 'General',
    description: 'Appearance, spaces, and application updates.',
    icon: Palette,
    tabs: [
      {
        id: 'display',
        domainId: 'general',
        label: 'Display',
        description: 'Theme, Iris indicator, version, and updates.',
        component: DisplayTab,
      },
      {
        id: 'spaces',
        domainId: 'general',
        label: 'Spaces',
        description: 'Create and open conversation spaces.',
        component: SpacesTab,
      },
    ],
  },
  {
    id: 'models-ai',
    label: 'Models & AI',
    description: 'Catalog, local roles, providers, prompts, tools, and voice.',
    icon: Boxes,
    tabs: [
      {
        id: 'models',
        domainId: 'models-ai',
        label: 'Models',
        description: 'Browse catalog, model storage, and Hugging Face.',
        component: ModelsTab,
        wide: true,
      },
      {
        id: 'downloaded-models',
        domainId: 'models-ai',
        label: 'Downloaded',
        description: 'Assign Chat, Utility, Embedding, Learning, and Verifier roles.',
        component: AIModelsTab,
        wide: true,
      },
      {
        id: 'chat',
        domainId: 'models-ai',
        label: 'Chat',
        description: 'Provider selection, Ollama, llama.cpp, cloud keys, and memory.',
        component: ChatTab,
      },
      {
        id: 'prompts',
        domainId: 'models-ai',
        label: 'Prompts',
        description: 'System prompt, RAG templates, and response verification.',
        component: PromptsTab,
      },
      {
        id: 'tools',
        domainId: 'models-ai',
        label: 'Tools',
        description: 'Built-in tools, presets, and custom HTTP endpoints.',
        component: ToolsTab,
        wide: true,
      },
      {
        id: 'read-aloud',
        domainId: 'models-ai',
        label: 'Read aloud',
        description: 'On-device speech synthesis voices, pace, and listening model.',
        component: ReadAloudTab,
      },
    ],
  },
  {
    id: 'search-indexing',
    label: 'Search & Indexing',
    description: 'Retrieval ranking, watched folders, and web source finders.',
    icon: Search,
    tabs: [
      {
        id: 'search',
        domainId: 'search-indexing',
        label: 'Search',
        description: 'Reranker, hybrid weights, vector compression, and retrieval tuning.',
        component: SearchTab,
      },
      {
        id: 'indexing',
        domainId: 'search-indexing',
        label: 'Indexing',
        description: 'Auto-indexing, batch size, watched folders, and exclude patterns.',
        component: IndexingTab,
      },
      {
        id: 'sources',
        domainId: 'search-indexing',
        label: 'Sources',
        description: 'Web search engines, SearXNG, polite pools, and archive fallback.',
        component: SourcesTab,
      },
    ],
  },
  {
    id: 'storage-sync',
    label: 'Storage & Sync',
    description: 'Vault markdown mirror, encrypted backups, export, and privacy.',
    icon: FolderOpen,
    tabs: [
      {
        id: 'vault',
        domainId: 'storage-sync',
        label: 'Vault',
        description: 'Vault folder, disk mirroring, local backups, and off-device encryption.',
        component: VaultTab,
      },
      {
        id: 'privacy',
        domainId: 'storage-sync',
        label: 'Privacy',
        description: 'Anonymous usage telemetry and crash reporting.',
        component: PrivacyTab,
      },
    ],
  },
  {
    id: 'learning-studio',
    label: 'Learning Studio',
    description: 'Source provenance trust rules, calibration bars, and checker reliability.',
    icon: GraduationCap,
    tabs: [
      {
        id: 'source-trust',
        domainId: 'learning-studio',
        label: 'Source trust',
        description: 'Site trust overrides, claim bars, policy parameters, and eval export.',
        component: SourceTrustSettings,
      },
    ],
  },
  {
    id: 'system-diagnostics',
    label: 'System & Diagnostics',
    description: 'Model runtime tuning, thinking, router, and local diagnostic logs.',
    icon: SlidersHorizontal,
    tabs: [
      {
        id: 'tuning',
        domainId: 'system-diagnostics',
        label: 'Tuning',
        description: 'Temperature, Top P/K, penalties, thinking, router, and tool output.',
        component: TuningTab,
      },
      {
        id: 'logs',
        domainId: 'system-diagnostics',
        label: 'Logs',
        description: 'Live app events, backend logs, filtering, and diagnostic export.',
        component: LogsTab,
      },
    ],
  },
];

export const ALL_SETTINGS_TABS: SettingsSubTab[] = SETTINGS_DOMAINS.flatMap(
  (domain) => domain.tabs,
);

const TAB_BY_ID = new Map<string, SettingsSubTab>(
  ALL_SETTINGS_TABS.map((tab) => [tab.id, tab]),
);

const DOMAIN_BY_ID = new Map<string, SettingsDomain>(
  SETTINGS_DOMAINS.map((domain) => [domain.id, domain]),
);

/**
 * Legacy-compatible `tabGroups` export so existing tests and references remain
 * completely intact.
 */
export const tabGroups = SETTINGS_DOMAINS.map((domain) => ({
  id: domain.id,
  label: domain.label,
  icon: domain.icon,
  tabs: domain.tabs,
}));

export const DEFAULT_SETTINGS_TAB: SettingsTab = 'display';

/**
 * Resolve any `?tab=<id>` query parameter or `settings:navigate-tab` payload.
 * Supports all 16 legacy tab IDs as well as the 6 consolidated domain IDs.
 */
export function resolveSettingsTab(raw: string | null | undefined): SettingsTab | null {
  if (!raw) return null;
  const trimmed = raw.trim();
  const exact = TAB_BY_ID.get(trimmed);
  if (exact) return exact.id;
  const domain = DOMAIN_BY_ID.get(trimmed);
  if (domain) return domain.tabs[0].id;
  return null;
}

export function getTabMeta(tabId: SettingsTab): SettingsSubTab {
  return TAB_BY_ID.get(tabId) ?? TAB_BY_ID.get(DEFAULT_SETTINGS_TAB)!;
}

export function getDomainForTab(tabId: SettingsTab): SettingsDomain {
  const meta = getTabMeta(tabId);
  return DOMAIN_BY_ID.get(meta.domainId) ?? SETTINGS_DOMAINS[0];
}

/**
 * Searchable catalog of individual settings across all 16 tabs.
 */
export const SEARCHABLE_SETTINGS: SearchableSetting[] = [
  // General — Display
  {
    id: 'display.theme',
    tabId: 'display',
    domainId: 'general',
    section: 'Appearance',
    label: 'Theme',
    description: 'Choose Light, Dark, or follow your system appearance.',
    keywords: ['appearance', 'dark mode', 'light mode', 'system', 'color', 'theme'],
  },
  {
    id: 'display.iris',
    tabId: 'display',
    domainId: 'general',
    section: 'Appearance',
    label: 'Iris',
    description: 'Sidebar status indicator showing background activity.',
    keywords: ['iris', 'sidebar', 'light', 'indicator', 'status'],
    elementId: 'display-iris',
  },
  {
    id: 'display.updates',
    tabId: 'display',
    domainId: 'general',
    section: 'About',
    label: 'Version & updates',
    description: 'View current build version and check for Lattice updates.',
    keywords: ['about', 'version', 'build', 'update', 'check for updates', 'release'],
  },

  // General — Spaces
  {
    id: 'spaces.manage',
    tabId: 'spaces',
    domainId: 'general',
    section: 'Spaces',
    label: 'Create & open spaces',
    description: 'Group related conversations into dedicated workspaces.',
    keywords: ['space', 'workspace', 'folder', 'group', 'create space', 'conversations'],
    elementId: 'settings-new-space-name',
  },

  // Models & AI — Models
  {
    id: 'models.catalog',
    tabId: 'models',
    domainId: 'models-ai',
    section: 'Catalog',
    label: 'Model catalog',
    description: 'Browse, filter, and download local LLM, embedding, and speech models.',
    keywords: ['catalog', 'download', 'gguf', 'llm', 'browse', 'quantization', 'qwen', 'llama'],
  },
  {
    id: 'models.storage',
    tabId: 'models',
    domainId: 'models-ai',
    section: 'Model storage',
    label: 'Downloads folder & external model folders',
    description: 'Inspect where models download and add extra local directories to scan.',
    keywords: ['downloads folder', 'external', 'directory', 'path', 'storage', 'models'],
  },
  {
    id: 'models.huggingface',
    tabId: 'models',
    domainId: 'models-ai',
    section: 'Hugging Face',
    label: 'Hugging Face token',
    description: 'Store a token in your system keychain for gated model downloads.',
    keywords: ['huggingface', 'hf', 'token', 'gated', 'keychain', 'auth'],
    elementId: 'hf-token',
  },

  // Models & AI — Downloaded
  {
    id: 'downloaded.roles',
    tabId: 'downloaded-models',
    domainId: 'models-ai',
    section: 'On this computer',
    label: 'Downloaded models & roles',
    description: 'Assign Chat, Utility, Embedding, Learning, and Verifier models.',
    keywords: ['downloaded', 'roles', 'chat model', 'utility model', 'embedding', 'learning model', 'verifier', 'vision'],
  },
  {
    id: 'downloaded.contextWindow',
    tabId: 'downloaded-models',
    domainId: 'models-ai',
    section: 'On this computer',
    label: 'Default context window',
    description: 'Fit each local model to machine memory or set an explicit token window.',
    keywords: ['context window', 'memory', 'vram', 'tokens', 'rope', 'fit'],
    elementId: 'local-context-window',
  },

  // Models & AI — Chat
  {
    id: 'chat.provider',
    tabId: 'chat',
    domainId: 'models-ai',
    section: 'Provider',
    label: 'Chat provider',
    description: 'Choose Auto, Local only, Ollama, llama.cpp, OpenAI, Anthropic, or Google.',
    keywords: ['provider', 'openai', 'anthropic', 'google', 'gemini', 'claude', 'gpt', 'local', 'cloud'],
    elementId: 'chatProvider',
  },
  {
    id: 'chat.ollama',
    tabId: 'chat',
    domainId: 'models-ai',
    section: 'Ollama server',
    label: 'Ollama server connection',
    description: 'Configure Ollama URL, model, utility model, and authentication headers.',
    keywords: ['ollama', 'server', 'url', 'auth', 'basic auth', 'header'],
    elementId: 'ollamaUrl',
  },
  {
    id: 'chat.llamacpp',
    tabId: 'chat',
    domainId: 'models-ai',
    section: 'llama.cpp server',
    label: 'llama.cpp server connection',
    description: 'Connect to an external llama.cpp server URL and auth header.',
    keywords: ['llamacpp', 'llama.cpp', 'server', 'url', 'endpoint'],
    elementId: 'llamaCppUrl',
  },
  {
    id: 'chat.memory',
    tabId: 'chat',
    domainId: 'models-ai',
    section: 'Memory',
    label: 'Remember requirements in a conversation',
    description: 'Record constraints, decisions, and goals with source quotations.',
    keywords: ['memory', 'conversation memory', 'requirements', 'facts', 'recall'],
  },

  // Models & AI — Prompts
  {
    id: 'prompts.verification',
    tabId: 'prompts',
    domainId: 'models-ai',
    section: 'Verification',
    label: 'Verify responses & judge sampling',
    description: 'Mark verified claims in answers and tune verification temperature/top-p/top-k.',
    keywords: ['verify', 'verification', 'claims', 'judge', 'hallucination', 'grounded'],
    elementId: 'verification-temperature',
  },
  {
    id: 'prompts.templates',
    tabId: 'prompts',
    domainId: 'models-ai',
    section: 'Templates',
    label: 'System prompt & RAG templates',
    description: 'Customize system prompt, greeting, with-documents, and without-documents templates.',
    keywords: ['prompt', 'system prompt', 'template', 'greeting', 'rag', 'instructions'],
    elementId: 'prompt-systemPrompt',
  },

  // Models & AI — Tools
  {
    id: 'tools.presets',
    tabId: 'tools',
    domainId: 'models-ai',
    section: 'Presets',
    label: 'Tool presets & custom endpoints',
    description: 'Add OpenAlex, Crossref, SearXNG, PubMed, or custom JSON HTTP tools.',
    keywords: ['tools', 'presets', 'custom tools', 'endpoint', 'api', 'openalex', 'crossref', 'web_search'],
    elementId: 'tool-preset',
  },

  // Models & AI — Read aloud
  {
    id: 'readAloud.voice',
    tabId: 'read-aloud',
    domainId: 'models-ai',
    section: 'Voice',
    label: 'Voice model & voice picker',
    description: 'Download Kokoro speech models and select British or American English voices.',
    keywords: ['voice', 'read aloud', 'tts', 'speech', 'kokoro', 'audio', 'speak', 'preview'],
  },
  {
    id: 'readAloud.speed',
    tabId: 'read-aloud',
    domainId: 'models-ai',
    section: 'Pace',
    label: 'Reading speed',
    description: 'Adjust how fast read-aloud voices speak.',
    keywords: ['speed', 'pace', 'rate', 'read aloud', 'tts'],
    elementId: 'speech-speed',
  },
  {
    id: 'readAloud.listening',
    tabId: 'read-aloud',
    domainId: 'models-ai',
    section: 'Voice conversations',
    label: 'Listening model (Whisper)',
    description: 'Download the on-device Whisper transcription model for microphone input.',
    keywords: ['whisper', 'mic', 'microphone', 'transcription', 'stt', 'voice conversation', 'listen'],
  },

  // Search & Indexing — Search
  {
    id: 'search.reranker',
    tabId: 'search',
    domainId: 'search-indexing',
    section: 'Retrieval',
    label: 'Reranker',
    description: 'Cross-encoder model that rescores retrieved passages before answering.',
    keywords: ['reranker', 'rerank', 'cross-encoder', 'qwen3-reranker', 'ranking'],
  },
  {
    id: 'search.webAndRetry',
    tabId: 'search',
    domainId: 'search-indexing',
    section: 'Retrieval',
    label: 'Read web pages & corrective retry',
    description: 'Fetch full web pages for web results and retry insufficient retrieval passes.',
    keywords: ['web pages', 'corrective retry', 'sufficiency', 'retrieval'],
    elementId: 'search-read-web-pages',
  },
  {
    id: 'search.vectorStorage',
    tabId: 'search',
    domainId: 'search-indexing',
    section: 'Retrieval',
    label: 'Vector storage & quantization',
    description: 'Configure full or truncated embeddings, stored dimensions, and precision.',
    keywords: ['vector', 'compression', 'truncation', 'quantization', 'dims', 'i8', 'f16', 'f32'],
    elementId: 'search-vector-compression-mode',
  },
  {
    id: 'search.summaries',
    tabId: 'search',
    domainId: 'search-indexing',
    section: 'Retrieval',
    label: 'Document summaries',
    description: 'Summarize documents and sections with the utility model for whole-doc questions.',
    keywords: ['document summaries', 'summary index', 'utility model'],
    elementId: 'search-summary-index',
  },
  {
    id: 'search.thresholds',
    tabId: 'search',
    domainId: 'search-indexing',
    section: 'Retrieval',
    label: 'Maximum results, similarity threshold & hybrid alpha',
    description: 'Tune result count, minimum similarity score, and keyword-vs-vector balance.',
    keywords: ['max results', 'similarity', 'threshold', 'hybrid', 'alpha', 'keyword', 'vector'],
    elementId: 'search-hybrid-alpha',
  },
  {
    id: 'search.advancedTuning',
    tabId: 'search',
    domainId: 'search-indexing',
    section: 'Advanced retrieval tuning',
    label: 'Advanced retrieval tuning',
    description: 'Fine-tune document shortlist, chunk candidates, and rerank limits.',
    keywords: ['advanced retrieval', 'shortlist', 'chunks', 'candidates', 'tuning'],
  },

  // Search & Indexing — Indexing
  {
    id: 'indexing.behavior',
    tabId: 'indexing',
    domainId: 'search-indexing',
    section: 'Behavior',
    label: 'Auto-index new files & batch size',
    description: 'Automatically index incoming files and control batch throughput.',
    keywords: ['auto-index', 'batch size', 'indexing', 'files', 'watcher'],
    elementId: 'autoIndex',
  },
  {
    id: 'indexing.folders',
    tabId: 'indexing',
    domainId: 'search-indexing',
    section: 'Watched folders',
    label: 'Watched folders',
    description: 'Add or remove directories indexed into your Lattice library.',
    keywords: ['watched folders', 'directories', 'library', 'add folder', 'index'],
  },
  {
    id: 'indexing.excludes',
    tabId: 'indexing',
    domainId: 'search-indexing',
    section: 'Excluded patterns',
    label: 'Excluded patterns',
    description: 'Skip matching glob patterns such as node_modules, .git, or temp files.',
    keywords: ['exclude', 'ignore', 'glob', 'patterns', 'node_modules'],
    elementId: 'excludePattern',
  },

  // Search & Indexing — Sources
  {
    id: 'sources.engines',
    tabId: 'sources',
    domainId: 'search-indexing',
    section: 'Paid search engines',
    label: 'Search engines & API keys',
    description: 'Configure web finders, rate-limit API keys, and disclosures.',
    keywords: ['sources', 'search engines', 'brave', 'tavily', 'exa', 'api key', 'finders'],
  },
  {
    id: 'sources.searxng',
    tabId: 'sources',
    domainId: 'search-indexing',
    section: 'Your SearXNG server',
    label: 'SearXNG server & polite pool contact',
    description: 'Point to a self-hosted SearXNG instance and set contact email for Crossref/Unpaywall.',
    keywords: ['searxng', 'crossref', 'unpaywall', 'polite pool', 'archive', 'wayback', 'internet archive'],
    elementId: 'searxngUrl',
  },

  // Storage & Sync — Vault
  {
    id: 'vault.folder',
    tabId: 'vault',
    domainId: 'storage-sync',
    section: 'Folder & Sync',
    label: 'Vault folder & markdown mirror',
    description: 'Choose the ~/Lattice mirror path, backfill notes, and watch external edits.',
    keywords: ['vault', 'folder', 'markdown', 'mirror', 'sync', 'obsidian', 'external edits'],
  },
  {
    id: 'vault.backups',
    tabId: 'vault',
    domainId: 'storage-sync',
    section: 'Backups',
    label: 'Local backups & library export',
    description: 'Create or restore local snapshots and export conversations as Markdown or JSON.',
    keywords: ['backup', 'restore', 'snapshot', 'export', 'markdown', 'json'],
  },
  {
    id: 'vault.offDevice',
    tabId: 'vault',
    domainId: 'storage-sync',
    section: 'Off-device backup',
    label: 'Off-device encrypted backup',
    description: 'Encrypted archive destination, retention count, passphrase, and 24-word recovery code.',
    keywords: ['off-device', 'encrypted', 'icloud', 'dropbox', 'nas', 'recovery code', 'passphrase', 'archive'],
  },

  // Storage & Sync — Privacy
  {
    id: 'privacy.telemetry',
    tabId: 'privacy',
    domainId: 'storage-sync',
    section: 'What leaves this device',
    label: 'Anonymous usage statistics & crash reports',
    description: 'Control opt-in telemetry and sanitized crash reports.',
    keywords: ['privacy', 'telemetry', 'analytics', 'crash reports', 'usage', 'data'],
  },

  // Learning Studio — Source trust
  {
    id: 'sourceTrust.overrides',
    tabId: 'source-trust',
    domainId: 'learning-studio',
    section: 'Source trust',
    label: 'Site trust overrides & calibration bars',
    description: 'Review site overrides, evidence bars in force, policy parameters, and checker reliability.',
    keywords: ['source trust', 'learning studio', 'provenance', 'overrides', 'checker', 'reliability', 'eval', 'citations'],
  },

  // System & Diagnostics — Tuning
  {
    id: 'tuning.runtime',
    tabId: 'tuning',
    domainId: 'system-diagnostics',
    section: 'Model runtime',
    label: 'Temperature, Top P, Top K & penalties',
    description: 'Tune generation sampling, max tokens, stall timeout, and streaming.',
    keywords: ['temperature', 'top p', 'top k', 'repeat penalty', 'presence penalty', 'max tokens', 'timeout', 'stream', 'sampling', 'runtime'],
    elementId: 'tuning-temperature',
  },
  {
    id: 'tuning.thinking',
    tabId: 'tuning',
    domainId: 'system-diagnostics',
    section: 'Thinking',
    label: 'Thinking & speculative decoding',
    description: 'Control reasoning before answers, thinking sampling profile, and experimental MTP drafting.',
    keywords: ['thinking', 'reasoning', 'speculative', 'mtp', 'draft tokens', 'decoding'],
    elementId: 'thinking-override',
  },
  {
    id: 'tuning.router',
    tabId: 'tuning',
    domainId: 'system-diagnostics',
    section: 'Router',
    label: 'Follow-up router',
    description: 'Configure follow-up routing model, timeout, ambiguity threshold, and prompts.',
    keywords: ['router', 'follow-up', 'ambiguity', 'clarify', 'last document'],
    elementId: 'router-model',
  },
  {
    id: 'tuning.toolOutput',
    tabId: 'tuning',
    domainId: 'system-diagnostics',
    section: 'Tool output',
    label: 'Tool output excerpts & templates',
    description: 'Adjust excerpt character length, highlight terms, and tool result templates.',
    keywords: ['tool output', 'excerpt', 'highlight', 'template'],
    elementId: 'tool-excerpt-chars',
  },

  // System & Diagnostics — Logs
  {
    id: 'logs.diagnostics',
    tabId: 'logs',
    domainId: 'system-diagnostics',
    section: 'Logs',
    label: 'Diagnostic logs',
    description: 'Inspect live app and backend logs, filter by severity, export, or clear.',
    keywords: ['logs', 'diagnostics', 'errors', 'warnings', 'debug', 'backend log', 'export logs'],
  },
];

export interface SettingsSearchMatch {
  item: SearchableSetting;
  domain: SettingsDomain;
  tab: SettingsSubTab;
  score: number;
}

/**
 * Filter and rank individual settings matching a user search query.
 * Every token in a multi-word query must match somewhere across label,
 * description, section, tab label, domain label, or keywords.
 */
export function searchSettings(query: string): SettingsSearchMatch[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return [];
  const tokens = normalized.split(/\s+/).filter(Boolean);

  const results: SettingsSearchMatch[] = [];

  for (const item of SEARCHABLE_SETTINGS) {
    const domain = DOMAIN_BY_ID.get(item.domainId)!;
    const tab = TAB_BY_ID.get(item.tabId)!;

    const labelLower = item.label.toLowerCase();
    const tabLower = tab.label.toLowerCase();
    const domainLower = domain.label.toLowerCase();
    const sectionLower = item.section.toLowerCase();
    const descLower = item.description.toLowerCase();
    const keywordsLower = item.keywords.join(' ').toLowerCase();
    const haystack = `${labelLower} ${tabLower} ${domainLower} ${sectionLower} ${descLower} ${keywordsLower}`;

    if (!tokens.every((token) => haystack.includes(token))) {
      continue;
    }

    let score = 0;
    if (labelLower.startsWith(normalized)) score += 100;
    else if (labelLower.includes(normalized)) score += 70;
    if (tabLower === normalized || domainLower === normalized) score += 80;
    else if (tabLower.includes(normalized) || domainLower.includes(normalized)) score += 45;
    if (sectionLower.includes(normalized)) score += 35;
    if (keywordsLower.includes(normalized)) score += 25;
    if (descLower.includes(normalized)) score += 15;
    score += tokens.length * 5;

    results.push({ item, domain, tab, score });
  }

  return results.sort((a, b) => b.score - a.score || a.item.label.localeCompare(b.item.label));
}
