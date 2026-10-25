import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { COMMAND_PLUGINS, type CommandName } from '@/shared/ipc/routes.generated';
import { diagnostics } from '@/utils/diagnostics';

import { settingsApi } from './client';

const route = (command: CommandName) => {
  const plugin = COMMAND_PLUGINS[command];
  if (!plugin) throw new Error(`No plugin route for ${command}`);
  return `plugin:${plugin}|${command}`;
};

type Case = [label: string, call: () => Promise<unknown>, command: CommandName, args: Record<string, unknown>];

const cases: Case[] = [
  ['listCloudModels', () => settingsApi.listCloudModels('anthropic'), 'list_cloud_models', { provider: 'anthropic' }],
  ['addWatchFolder', () => settingsApi.addWatchFolder('/archives'), 'add_watch_folder', { path: '/archives' }],
  ['removeWatchFolder', () => settingsApi.removeWatchFolder('/archives'), 'remove_watch_folder', { path: '/archives' }],
  ['getSettings', () => settingsApi.getSettings(), 'get_settings', {}],
  ['getSettingsCategory', () => settingsApi.getSettingsCategory('llm'), 'get_settings_category', { category: 'llm' }],
  ['updateSettings', () => settingsApi.updateSettings({ theme: 'dark' } as never), 'update_settings', { settings: { theme: 'dark' } }],
  ['resetSettings', () => settingsApi.resetSettings(), 'reset_settings', {}],
  ['exportSettings', () => settingsApi.exportSettings(), 'export_settings', {}],
  ['importSettings merging', () => settingsApi.importSettings('{"a":1}', true), 'import_settings', { request: { settings: '{"a":1}', merge: true } }],
  ['importSettings replacing', () => settingsApi.importSettings('{"a":1}'), 'import_settings', { request: { settings: '{"a":1}', merge: undefined } }],
  ['getSystemTheme', () => settingsApi.getSystemTheme(), 'get_system_theme', {}],
  ['validateFolderPath', () => settingsApi.validateFolderPath('/archives'), 'validate_folder_path', { path: '/archives' }],
  ['testLlamaCppConnection', () => settingsApi.testLlamaCppConnection({ baseUrl: 'http://h' } as never), 'test_llama_cpp_connection', { request: { baseUrl: 'http://h' } }],
  ['testOllamaConnection', () => settingsApi.testOllamaConnection({ baseUrl: 'http://h' } as never), 'test_ollama_connection', { request: { baseUrl: 'http://h' } }],
  ['testCustomTool', () => settingsApi.testCustomTool({ name: 'tide' } as never), 'test_custom_tool', { request: { name: 'tide' } }],
  ['setCloudApiKey', () => settingsApi.setCloudApiKey('openai', 'sk'), 'set_cloud_api_key', { provider: 'openai', key: 'sk' }],
  ['getSourcesStatus', () => settingsApi.getSourcesStatus(), 'get_sources_status', {}],
  ['saveSourceKey', () => settingsApi.saveSourceKey('brave' as never, 'k'), 'save_source_key', { service: 'brave', key: 'k' }],
  ['removeSourceKey', () => settingsApi.removeSourceKey('brave' as never), 'remove_source_key', { service: 'brave' }],
];

describe('settingsApi', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    diagnostics.clear();
  });

  it.each(cases)('%s invokes its command through the registering plugin', async (_label, call, command, args) => {
    vi.mocked(invoke).mockResolvedValueOnce('ok');
    await expect(call()).resolves.toEqual({ ok: true, data: 'ok' });
    expect(invoke).toHaveBeenCalledWith(route(command), args);
  });

  it('returns a refused update as a failed result', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('invalid theme');
    await expect(settingsApi.updateSettings({} as never)).resolves.toMatchObject({ ok: false, error: 'invalid theme' });
  });
});
