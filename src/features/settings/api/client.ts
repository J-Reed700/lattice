import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  AppSettings,
  TestCustomToolRequest,
  TestCustomToolResponse,
  TestOllamaConnectionRequest,
  TestOllamaConnectionResponse,
} from '@/types';

export const settingsApi = {
  /** Models available to a provider's saved API key; the key stays native. */
  listCloudModels: async (provider: Wire.CloudProvider): Promise<ApiResult<Wire.CloudModelCatalogDto>> =>
    apiCall('list_cloud_models', { provider }),

  /** What a connected model's provider reports it can take. */
  getModelLimits: async (model: Wire.ModelRef): Promise<ApiResult<Wire.ModelWindowDto>> =>
    apiCall('get_model_limits', { model }),

  /**
   * Adds a folder to the file watcher.
   * Enables automatic reindexing when files in this folder change.
   *
   * @param path - Absolute path to folder to watch
   * @returns Void on success
   */
  addWatchFolder: async (path: string): Promise<ApiResult<null>> =>
    apiCall('add_watch_folder', { path }),

  /**
   * Removes a folder from the file watcher.
   * Stops monitoring for file changes in this folder.
   *
   * @param path - Absolute path to folder to stop watching
   * @returns Void on success
   */
  removeWatchFolder: async (path: string): Promise<ApiResult<null>> =>
    apiCall('remove_watch_folder', { path }),

  /**
   * Retrieves all application settings.
   * Returns complete settings object with all categories.
   *
   * @returns Settings object with all configuration values
   */
  getSettings: async (): Promise<ApiResult<AppSettings>> =>
    apiCall('get_settings'),

  /**
   * Gets settings for a specific category.
   * Categories include 'appearance', 'search', 'indexing', etc.
   *
   * @param category - Settings category name
   * @returns Settings object for the specified category
   */
  getSettingsCategory: async (
    category: string,
  ): Promise<ApiResult<Record<string, unknown>>> =>
    apiCall('get_settings_category', { category }),

  /**
   * Updates one or more settings.
   * Merges provided settings with existing configuration.
   *
   * @param settings - Partial settings object with values to update
   * @returns Updated settings object
   */
  updateSettings: async (
    settings: Wire.UpdateSettingsRequest,
  ): Promise<ApiResult<AppSettings>> =>
    apiCall('update_settings', { settings }),

  /**
   * Resets all settings to default values.
   * Clears user customizations and restores factory defaults.
   *
   * @returns Updated settings object
   */
  resetSettings: async (): Promise<ApiResult<AppSettings>> =>
    apiCall('reset_settings'),

  /**
   * Exports all settings as JSON string.
   * Useful for backup or transferring settings between devices.
   *
   * @returns JSON string containing all settings
   */
  exportSettings: async (): Promise<ApiResult<string>> =>
    apiCall('export_settings'),

  /**
   * Imports settings from a JSON string.
   * Replaces current settings with imported configuration.
   *
   * @param settings - JSON string containing settings to import
   * @param merge - If true, merge with existing settings instead of replacing
   * @returns Updated settings object
   */
  importSettings: async (
    settings: string,
    merge?: boolean,
  ): Promise<ApiResult<AppSettings>> =>
    apiCall('import_settings', {
      request: {
        settings,
        merge: merge ?? null,
      },
    }),

  /**
   * Shows the system save dialog and writes `contents` to the file the user
   * picks; the webview never names the path.
   *
   * @returns False when the user cancels
   */
  saveTextFile: async (
    suggestedName: string,
    filters: Wire.FileFilterDto[],
    contents: string,
  ): Promise<ApiResult<boolean>> =>
    apiCall('save_text_file', { suggestedName, filters, contents }),

  /**
   * Shows the system open dialog and reads the file the user picks as text.
   *
   * @returns The text, or null when the user cancels
   */
  openTextFile: async (
    filters: Wire.FileFilterDto[],
  ): Promise<ApiResult<string | null>> =>
    apiCall('open_text_file', { filters }),

  /**
   * Detects the system theme preference.
   * Returns 'light' or 'dark' based on OS-level settings.
   *
   * @returns System theme as string ('light' or 'dark')
   */
  getSystemTheme: async (): Promise<ApiResult<string>> =>
    apiCall('get_system_theme'),

  /**
   * Validates that a folder path exists and is accessible.
   * Checks permissions and path validity before indexing operations.
   *
   * @param path - Absolute folder path to validate
   * @returns True if path is valid and accessible, false otherwise
   */
  validateFolderPath: async (path: string): Promise<ApiResult<boolean>> =>
    apiCall('validate_folder_path', { path }),

  /** Checks llama.cpp model discovery and a small chat completion. */
  testLlamaCppConnection: async (
    request: Wire.TestLlamaCppConnectionRequest,
  ): Promise<ApiResult<TestOllamaConnectionResponse>> =>
    apiCall('test_llama_cpp_connection', {
      request,
    }),

  /** Whether the keyring holds an auth header value for each remote server. The value itself is never readable. */
  getRemoteAuthHeaderStatus: async (): Promise<ApiResult<Wire.RemoteAuthStatusDto>> =>
    apiCall('get_remote_auth_header_status'),

  /**
   * Stores (or, with null, deletes) a remote server's auth header value in the
   * OS keyring. The value is never read back; the status command reports only
   * whether one is set.
   */
  saveRemoteAuthHeaderValue: async (
    server: Wire.RemoteServer,
    value: string | null,
  ): Promise<ApiResult<null>> =>
    apiCall('save_remote_auth_header_value', { server, value }),

  /** Lists models through the native Ollama API. */
  testOllamaConnection: async (
    request: TestOllamaConnectionRequest,
  ): Promise<ApiResult<TestOllamaConnectionResponse>> =>
    apiCall('test_ollama_connection', {
      request,
    }),

  /**
   * Tests a custom tool endpoint with a sample query.
   * Useful for validating endpoint/query-param wiring before saving.
   */
  testCustomTool: async (
    request: TestCustomToolRequest,
  ): Promise<ApiResult<TestCustomToolResponse>> =>
    apiCall('test_custom_tool', { request }),

  /**
   * Stores a cloud provider's API key in the system keychain.
   *
   * @param provider - Cloud provider the key belongs to
   * @param key - The API key itself; never persisted in settings
   * @returns Void on success
   */
  setCloudApiKey: async (
    provider: string,
    key: string,
  ): Promise<ApiResult<null>> =>
    apiCall('set_cloud_api_key', { provider, key }),

  /**
   * Settings → Sources: every source finder with its need, privacy line,
   * switch, gate and health, and which key services have a key stored.
   */
  getSourcesStatus: async (): Promise<ApiResult<Wire.SourcesStatusDto>> =>
    apiCall('get_sources_status'),

  /**
   * Stores a source-finding key in the system keychain. The answer says only
   * that a key is stored, never the key.
   */
  saveSourceKey: async (
    service: Wire.KeyServiceDto,
    key: string,
  ): Promise<ApiResult<Wire.SourcesStatusDto>> =>
    apiCall('save_source_key', { service, key }),

  /** Removes a source-finding key from the system keychain. */
  removeSourceKey: async (
    service: Wire.KeyServiceDto,
  ): Promise<ApiResult<Wire.SourcesStatusDto>> =>
    apiCall('remove_source_key', { service }),
};
