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
  addWatchFolder: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('add_watch_folder', { path }),

  removeWatchFolder: async (path: string): Promise<ApiResult<void>> =>
    apiCall<void>('remove_watch_folder', { path }),

  getSettings: async (): Promise<ApiResult<AppSettings>> =>
    apiCall<AppSettings>('get_settings'),

  getSettingsCategory: async (
    category: string,
  ): Promise<ApiResult<Record<string, unknown>>> =>
    apiCall<Record<string, unknown>>('get_settings_category', { category }),

  updateSettings: async (
    settings: Wire.UpdateSettingsRequest,
  ): Promise<ApiResult<AppSettings>> =>
    apiCall<AppSettings>('update_settings', { settings }),

  resetSettings: async (): Promise<ApiResult<AppSettings>> =>
    apiCall<AppSettings>('reset_settings'),

  exportSettings: async (): Promise<ApiResult<string>> =>
    apiCall<string>('export_settings'),

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

  getSystemTheme: async (): Promise<ApiResult<string>> =>
    apiCall<string>('get_system_theme'),

  validateFolderPath: async (path: string): Promise<ApiResult<boolean>> =>
    apiCall<boolean>('validate_folder_path', { path }),

  testLlamaCppConnection: async (
    request: Wire.LlamaCppSettingsDto,
  ): Promise<ApiResult<TestOllamaConnectionResponse>> =>
    apiCall<Wire.TestOllamaConnectionResponse>('test_llama_cpp_connection', {
      request,
    }),

  testOllamaConnection: async (
    request: TestOllamaConnectionRequest,
  ): Promise<ApiResult<TestOllamaConnectionResponse>> =>
    apiCall<Wire.TestOllamaConnectionResponse>('test_ollama_connection', {
      request,
    }),

  testCustomTool: async (
    request: TestCustomToolRequest,
  ): Promise<ApiResult<TestCustomToolResponse>> =>
    apiCall<Wire.TestCustomToolResponse>('test_custom_tool', { request }),

  /** Stores a cloud provider's API key in the system keychain. */
  setCloudApiKey: async (
    provider: string,
    key: string,
  ): Promise<ApiResult<void>> =>
    apiCall<void>('set_cloud_api_key', { provider, key }),

  getSourcesStatus: async (): Promise<ApiResult<Wire.SourcesStatusDto>> =>
    apiCall<Wire.SourcesStatusDto>('get_sources_status'),

  /** Stores a source-finding key in the system keychain. */
  saveSourceKey: async (
    service: Wire.KeyServiceDto,
    key: string,
  ): Promise<ApiResult<Wire.SourcesStatusDto>> =>
    apiCall<Wire.SourcesStatusDto>('save_source_key', { service, key }),

  /** Removes a source-finding key from the system keychain. */
  removeSourceKey: async (
    service: Wire.KeyServiceDto,
  ): Promise<ApiResult<Wire.SourcesStatusDto>> =>
    apiCall<Wire.SourcesStatusDto>('remove_source_key', { service }),
};
