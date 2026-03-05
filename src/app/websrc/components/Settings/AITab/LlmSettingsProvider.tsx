import { LlmSettingsContext, useLlmSettingsProvider } from './useLlmSettings';

/**
 * Wraps AI sub-tabs so they share a single LLM settings instance.
 * The provider loads settings once on mount and all child tabs
 * read/write the same state - no duplicate API calls, no stale data.
 *
 * Safe to wrap non-AI tabs too; the API call is cheap and only fires once.
 */
export function LlmSettingsProvider({ children }: { children: React.ReactNode }) {
  const value = useLlmSettingsProvider();
  return (
    <LlmSettingsContext.Provider value={value}>
      {children}
    </LlmSettingsContext.Provider>
  );
}
