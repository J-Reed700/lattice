import { Page, Locator } from '@playwright/test';
import { ExtendedPage } from '../setup';

export class BasePage {
  constructor(protected page: Page) {}

  async goto(url?: string): Promise<void> {
    if (url) {
      await this.page.goto(url);
    } else {
      await this.page.goto('/');
    }
  }

  async waitForLoad(): Promise<void> {
    await this.page.waitForLoadState('domcontentloaded');
  }

  async click(selector: string): Promise<void> {
    await this.page.click(selector);
  }

  async fill(selector: string, value: string): Promise<void> {
    await this.page.fill(selector, value);
  }

  async getText(selector: string): Promise<string> {
    return this.page.textContent(selector) || '';
  }

  async isVisible(selector: string): Promise<boolean> {
    return this.page.isVisible(selector);
  }

  async waitForSelector(selector: string, timeout = 10000): Promise<void> {
    await this.page.waitForSelector(selector, { timeout });
  }

  locator(selector: string): Locator {
    return this.page.locator(selector);
  }

  getByRole(role: Parameters<Page['getByRole']>[0], options?: Parameters<Page['getByRole']>[1]): Locator {
    return this.page.getByRole(role, options);
  }

  getByText(text: string | RegExp): Locator {
    return this.page.getByText(text);
  }

  getByPlaceholder(text: string | RegExp): Locator {
    return this.page.getByPlaceholder(text);
  }

  async screenshot(name: string): Promise<void> {
    await this.page.screenshot({ path: `e2e-results/screenshots/${name}.png` });
  }
}
