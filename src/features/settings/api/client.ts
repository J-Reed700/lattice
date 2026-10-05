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
  /**
   * Adds a folder to the file watcher.
   * Enables automatic reindexing when files in this folder change.
   *
   * @param path - Absolute path to folder to watch
   * @returns Void on success
   */
  addWatchFolder: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('add_watch_folder', { path }),

  /**
   * Removes a folder from the file watcher.
   * Stops monitoring for file changes in this folder.
   *
   * @param path - Absolute path to folder to stop watching
   * @returns Void on success
   */
  removeWatchFolder: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('remove_watch_folder', { path }),

  /**
   * Retrieves all application settings.
   * Returns complete settings object with all categories.
   *
   * @returns Settings object with all configuration values
   */
  getSettings: async (): Promise<ApiResult<AppSettings>> =>
    apiCall<AppSettings>('get_settings'),

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
    apiCall<Record<string, unknown>>('get_settings_category', { category }),

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
    apiCall<AppSettings>('update_settings', { settings }),

  /**
   * Resets all settings to default values.
   * Clears user customizations and restores factory defaults.
   *
   * @returns Updated settings object
   */
  resetSettings: async (): Promise<ApiResult<AppSettings>> =>
    apiCall<AppSettings>('reset_settings'),

  /**
   * Exports all settings as JSON string.
   * Useful for backup or transferring settings between devices.
   *
   * @returns JSON string containing all settings
   */
  exportSettings: async (): Promise<ApiResult<string>> =>
    apiCall<string>('export_settings'),

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
    apiCall<AppSettings>('import_settings', {
      request: {
        settings,
        merge,
      },
    }),

  /**
   * Detects the system theme preference.
   * Returns 'light' or 'dark' based on OS-level settings.
   *
   * @returns System theme as string ('light' or 'dark')
   */
  getSystemTheme: async (): Promise<ApiResult<string>> =>
    apiCall<string>('get_system_theme'),

  /**
   * Validates that a folder path exists and is accessible.
   * Checks permissions and path validity before indexing operations.
   *
   * @param path - Absolute folder path to validate
   * @returns True if path is valid and accessible, false otherwise
   */
  validateFolderPath: async (path: string): Promise<ApiResult<boolean>> =>
    apiCall<boolean>('validate_folder_path', { path }),

  /** Checks llama.cpp model discovery and a small chat completion. */
  testLlamaCppConnection: async (
    request: Wire.LlamaCppSettingsDto,
  ): Promise<ApiResult<TestOllamaConnectionResponse>> =>
    apiCall<Wire.TestOllamaConnectionResponse>('test_llama_cpp_connection', {
      request,
    }),

  /** Lists models through the native Ollama API. */
  testOllamaConnection: async (
    request: TestOllamaConnectionRequest,
  ): Promise<ApiResult<TestOllamaConnectionResponse>> =>
    apiCall<Wire.TestOllamaConnectionResponse>('test_ollama_connection', {
      request,
    }),

  /**
   * Tests a custom tool endpoint with a sample query.
   * Useful for validating endpoint/query-param wiring before saving.
   */
  testCustomTool: async (
    request: TestCustomToolRequest,
  ): Promise<ApiResult<TestCustomToolResponse>> =>
    apiCall<Wire.TestCustomToolResponse>('test_custom_tool', { request }),
};
