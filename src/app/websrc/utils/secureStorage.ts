/**
 * SECURITY: Secure Storage Utilities
 * 
 * This module provides utilities for storing data securely in the Tauri desktop app.
 * 
 * **STORAGE CLASSIFICATION**:
 * 
 * 1. **Sensitive Data** (API keys, tokens, passwords):
 *    - MUST use OS keyring via Tauri secure storage commands
 *    - NEVER use localStorage
 *    - Examples: API keys, OAuth tokens, passwords
 * 
 * 2. **User Preferences** (non-sensitive settings):
 *    - CAN use localStorage (or Tauri's store plugin)
 *    - Examples: theme, shortcuts, UI state, onboarding status
 * 
 * 3. **Temporary Data** (session data):
 *    - CAN use sessionStorage or in-memory state
 *    - Examples: search filters, temporary UI state
 * 
 * **WHY THIS MATTERS**:
 * - localStorage is stored in plain text in the filesystem
 * - Anyone with filesystem access can read localStorage
 * - OS keyring is encrypted and protected by OS security
 * 
 * **CURRENT USAGE AUDIT** (2025-11-15):
 * All localStorage usage has been audited and confirmed to store only non-sensitive data:
 * - Error logs (errorLogger.ts) ✅
 * - Onboarding state (useFirstRun.ts) ✅
 * - UI preferences (command palette, shortcuts, recent docs) ✅
 * - Settings (settingsStore.ts) ✅ - NO sensitive data
 * 
 * **MIGRATION STATUS**:
 * ✅ API keys -> Moved to OS keyring (src-tauri/src/commands/credentials.rs)
 * ✅ OAuth tokens -> Would use OS keyring
 * ✅ User preferences -> Safe in localStorage (non-sensitive)
 */

import { VaultAPI } from '@/lib/api';

/**
 * Type-safe wrapper around localStorage for non-sensitive data.
 * 
 * SECURITY WARNING: Only use this for non-sensitive data!
 * For API keys, passwords, or tokens, use SecureCredentialStore instead.
 */
export class LocalStorageWrapper<T> {
  constructor(private key: string) {}

  get(): T | null {
    try {
      const item = localStorage.getItem(this.key);
      if (!item) return null;
      return JSON.parse(item) as T;
    } catch (error) {
      console.error(`Failed to get localStorage item ${this.key}:`, error);
      return null;
    }
  }

  set(value: T): void {
    try {
      localStorage.setItem(this.key, JSON.stringify(value));
    } catch (error) {
      console.error(`Failed to set localStorage item ${this.key}:`, error);
    }
  }

  remove(): void {
    try {
      localStorage.removeItem(this.key);
    } catch (error) {
      console.error(`Failed to remove localStorage item ${this.key}:`, error);
    }
  }

  clear(): void {
    this.remove();
  }
}

/**
 * Secure credential storage using OS keyring via Tauri DDD commands.
 *
 * Key Format: {service}_api_key
 * Examples: "ollama_api_key", "openai_api_key", "anthropic_api_key"
 *
 * Current Limitation: Rust backend only supports "ollama" service.
 * Other services will throw unsupported service errors.
 *
 * **USAGE**:
 * ```typescript
 * const apiKeyStore = new SecureCredentialStore('ollama_api_key');
 *
 * // Store API key securely
 * await apiKeyStore.set('sk-...');
 *
 * // Retrieve API key
 * const apiKey = await apiKeyStore.get();
 *
 * // Delete API key
 * await apiKeyStore.delete();
 * ```
 */
export class SecureCredentialStore {
  constructor(private key: string) {
    // Validate format immediately
    this.extractService(); // Throws if invalid
  }

  /**
   * Extracts service name from key pattern.
   *
   * @returns Service name (e.g., "ollama", "openai")
   * @throws {Error} If key doesn't match {service}_api_key pattern
   *
   * @private
   */
  private extractService(): string {
    const match = this.key.match(/^(\w+)_api_key$/);
    if (!match) {
      throw new Error(
        `Invalid credential key format: "${this.key}". ` +
        `Expected format: {service}_api_key (e.g., "ollama_api_key", "openai_api_key")`
      );
    }
    return match[1];
  }

  /**
   * Stores an API key securely in the OS keyring.
   *
   * @param value - API key value to store
   * @throws {Error} If storage fails or service is unsupported
   *
   * @example
   * await storage.set('sk-1234567890');
   */
  async set(value: string): Promise<void> {
    const service = this.extractService();
    const result = await VaultAPI.setApiKey(service, value);
    if (!result.ok) {
      console.error(`Failed to store credential ${this.key}:`, result.error);
      throw new Error(`Failed to store credential: ${result.error}`);
    }
  }

  /**
   * Retrieves an API key from the OS keyring.
   *
   * @returns API key value or null if not found
   * @throws {Error} If retrieval fails (not including "not found")
   *
   * @example
   * const key = await storage.get();
   * if (key) console.log('Key exists');
   */
  async get(): Promise<string | null> {
    const service = this.extractService();
    const result = await VaultAPI.getApiKey(service);
    if (result.ok) {
      return result.data;
    }
    // Credential not found is not an error - return null
    if (result.error.includes('not found') || result.error.includes('No entry found')) {
      return null;
    }
    console.error(`Failed to get credential ${this.key}:`, result.error);
    throw new Error(`Failed to get credential: ${result.error}`);
  }

  /**
   * Deletes an API key from the OS keyring.
   *
   * @throws {Error} If deletion fails
   *
   * @example
   * await storage.delete();
   */
  async delete(): Promise<void> {
    const service = this.extractService();
    const result = await VaultAPI.deleteApiKey(service);
    if (!result.ok) {
      console.error(`Failed to delete credential ${this.key}:`, result.error);
      throw new Error(`Failed to delete credential: ${result.error}`);
    }
  }

  /**
   * Checks if an API key exists in the OS keyring.
   *
   * @returns true if credential exists, false otherwise
   *
   * @example
   * if (await storage.exists()) {
   *   console.log('Key is stored');
   * }
   */
  async exists(): Promise<boolean> {
    const service = this.extractService();
    const result = await VaultAPI.hasApiKey(service);
    if (result.ok) {
      return result.data;
    }
    console.error(`Failed to check credential ${this.key}:`, result.error);
    // Return false on error - conservative approach
    return false;
  }
}

/**
 * Storage classifier to help developers choose the right storage mechanism
 * 
 * USAGE:
 * ```typescript
 * // This will throw an error at runtime if you try to store sensitive data in localStorage
 * const storage = StorageClassifier.getStorage('api_key'); // ❌ Error: Use secure storage!
 * const storage = StorageClassifier.getStorage('theme'); // ✅ OK: Returns localStorage wrapper
 * ```
 */
export class StorageClassifier {
  private static SENSITIVE_PATTERNS = [
    'api_key',
    'apikey',
    'api-key',
    'secret',
    'password',
    'passwd',
    'pwd',
    'token',
    'credential',
    'auth',
    'oauth',
    'jwt',
    'bearer',
    'key',
  ];

  static isSensitive(key: string): boolean {
    const lowerKey = key.toLowerCase();
    return this.SENSITIVE_PATTERNS.some((pattern) =>
      lowerKey.includes(pattern)
    );
  }

  static getStorage<T>(key: string): LocalStorageWrapper<T> {
    if (this.isSensitive(key)) {
      throw new Error(
        `SECURITY ERROR: Key "${key}" appears to contain sensitive data. ` +
          `Use SecureCredentialStore instead of localStorage for: ${key}`
      );
    }
    return new LocalStorageWrapper<T>(key);
  }

  static getSecureStorage(key: string): SecureCredentialStore {
    if (!this.isSensitive(key)) {
      console.warn(
        `Warning: Key "${key}" does not appear to be sensitive. ` +
          `You can use localStorage for non-sensitive data.`
      );
    }
    return new SecureCredentialStore(key);
  }
}

// Export convenience functions
export const createStorage = <T>(key: string) =>
  StorageClassifier.getStorage<T>(key);
export const createSecureStorage = (key: string) =>
  StorageClassifier.getSecureStorage(key);

// Type definitions for common storage keys
export type OnboardingState = {
  completed: boolean;
  tourDismissed: boolean;
  lastSeen?: string;
};

export type KeyboardShortcuts = Record<
  string,
  {
    key: string;
    modifiers?: ('ctrl' | 'shift' | 'alt' | 'meta')[];
  }
>;

export type RecentDocument = {
  id: string;
  title: string;
  path: string;
  lastAccessed: string;
};

// Pre-configured storage instances for common use cases
export const onboardingStorage =
  new LocalStorageWrapper<OnboardingState>('vault_onboarding');
export const shortcutsStorage = new LocalStorageWrapper<KeyboardShortcuts>(
  'lattice-custom-shortcuts'
);
export const recentDocsStorage = new LocalStorageWrapper<RecentDocument[]>(
  'lattice:recentDocuments'
);

// Secure credential stores
export const openaiApiKeyStorage = new SecureCredentialStore('openai_api_key');
export const anthropicApiKeyStorage = new SecureCredentialStore(
  'anthropic_api_key'
);
