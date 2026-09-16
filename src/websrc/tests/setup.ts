import '@testing-library/jest-dom';
import { cleanup } from '@testing-library/react';
import { afterEach, vi } from 'vitest';

import * as apiMock from '../lib/__mocks__/api';

// Node can expose its own experimental storage globals. Tests must use the
// jsdom window's origin-scoped storage, just like the renderer does.
const testWindow = (globalThis as typeof globalThis & { jsdom: { window: Window } }).jsdom.window;
Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: testWindow.localStorage });
Object.defineProperty(globalThis, 'sessionStorage', { configurable: true, value: testWindow.sessionStorage });

vi.mock('../lib/api', () => apiMock);
vi.mock('@/lib/api', () => apiMock);

afterEach(() => {
  cleanup();
});

global.matchMedia = global.matchMedia || function () {
  return {
    matches: false,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  };
};

const mockIPC = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockIPC,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(() => Promise.resolve()),
}));


vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(() => Promise.resolve(null)),
  save: vi.fn(() => Promise.resolve(null)),
}));

export { mockIPC };
