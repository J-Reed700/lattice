import { test as base, expect, Page } from '@playwright/test';
import type { BrowserContext } from '@playwright/test';

export interface TauriHandle {
  invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
}

export interface ExtendedPage extends Page {
  tauri: TauriHandle;
}

export const test = base.extend<{ tauriPage: ExtendedPage }>({
  tauriPage: async ({ page }, use) => {
    const tauriPage = page as ExtendedPage;

    await page.evaluate(() => {
      (window as any).__TAURI_INVOKE__ = (window as any).__TAURI_INVOKE__ || [];
    });

    tauriPage.tauri = {
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        return page.evaluate(
          async ({ cmd, args }) => {
            if (typeof (window as any).__TAURI__ !== 'undefined') {
              return (window as any).__TAURI__.core.invoke(cmd, args);
            }
            throw new Error('Tauri API not available');
          },
          { cmd, args }
        );
      },
    };

    await use(tauriPage);
  },
});

export { expect };

export async function waitForTauriReady(page: Page, timeout = 30000): Promise<void> {
  await page.waitForFunction(
    () => typeof (window as any).__TAURI__ !== 'undefined',
    { timeout }
  );
}

export async function waitForElementWithText(
  page: Page,
  selector: string,
  text: string,
  timeout = 10000
): Promise<void> {
  await page.waitForSelector(selector, { timeout });
  await page.waitForFunction(
    ({ selector, text }) => {
      const elements = document.querySelectorAll(selector);
      return Array.from(elements).some((el) =>
        el.textContent?.includes(text)
      );
    },
    { selector, text },
    { timeout }
  );
}

export async function clearTestData(page: ExtendedPage): Promise<void> {
  try {
    await page.evaluate(() => {
      localStorage.clear();
      sessionStorage.clear();
    });
  } catch (error) {
    console.warn('Failed to clear test data:', error);
  }
}

export function getTestFilePath(filename: string): string {
  const path = require('path');
  return path.resolve(__dirname, 'fixtures', filename);
}

export async function waitForIndexingComplete(
  page: ExtendedPage,
  timeout = 30000
): Promise<void> {
  const startTime = Date.now();

  while (Date.now() - startTime < timeout) {
    const progress = await page.tauri.invoke('get_index_progress');

    if (progress && typeof progress === 'object') {
      const progressObj = progress as { total: number; processed: number };
      if (progressObj.total > 0 && progressObj.processed >= progressObj.total) {
        return;
      }
    }

    await page.waitForTimeout(500);
  }

  throw new Error('Indexing did not complete within timeout');
}

export interface TestDocument {
  name: string;
  content: string;
  path?: string;
}

export const sampleDocuments: TestDocument[] = [
  {
    name: 'test-document.txt',
    content: 'This is a test document for search functionality. It contains sample text about artificial intelligence and machine learning.',
  },
  {
    name: 'sample-notes.md',
    content: '# Sample Notes\n\nThis is a markdown document with:\n- Lists\n- **Bold text**\n- *Italic text*\n\nGreat for testing markdown rendering.',
  },
  {
    name: 'code-sample.txt',
    content: 'function example() {\n  console.log("Testing code search");\n  return true;\n}',
  },
];
