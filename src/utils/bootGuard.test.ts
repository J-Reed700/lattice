import { readFileSync } from 'node:fs';
import path from 'node:path';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BOOT_FAILURE_STORAGE_KEY } from './globalDiagnostics';

const BOOT_GUARD_SOURCE = readFileSync(
  path.resolve(import.meta.dirname, '../../public/boot-guard.js'),
  'utf8',
);
const RETRY_KEY = 'lattice:boot-retry';

interface TauriBridgeMock {
  invoke: ReturnType<typeof vi.fn>;
}

function installBootGuard(reloadSpy: ReturnType<typeof vi.fn>, tauriBridge?: TauriBridgeMock) {
  const locationProxy = {
    href: window.location.href,
    reload: reloadSpy,
  };
  const windowProxy = new Proxy(window, {
    get(target, prop, receiver) {
      if (prop === 'location') return locationProxy;
      if (prop === '__TAURI_INTERNALS__' && tauriBridge) return tauriBridge;
      const value = Reflect.get(target, prop, receiver);
      return typeof value === 'function' ? value.bind(target) : value;
    },
    set(target, prop, value) {
      Reflect.set(target, prop, value);
      return true;
    },
  });

  const execute = new Function('window', BOOT_GUARD_SOURCE);
  execute(windowProxy);
}

function dispatchWindowError(error: Error) {
  const absorb = vi.fn();
  window.addEventListener('error', absorb);
  try {
    window.dispatchEvent(new ErrorEvent('error', { error }));
  } finally {
    window.removeEventListener('error', absorb);
  }
  return absorb;
}

describe('public/boot-guard.js', () => {
  let reloadSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    reloadSpy = vi.fn();
    sessionStorage.clear();
    document.documentElement.dataset.theme = 'dark';
    document.head.innerHTML = '<style id="lattice-boot-styles"></style>';
    document.body.innerHTML = '<div id="root"></div>';
  });

  afterEach(() => {
    window.__LATTICE_BOOT__?.disarm();
    delete window.__LATTICE_BOOT__;
    sessionStorage.clear();
    document.head.innerHTML = '';
    document.body.innerHTML = '';
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('disarms timers, listeners, boot stylesheet, and stale retry marker once React mounts', () => {
    sessionStorage.setItem(RETRY_KEY, String(Date.now()));
    const rafSpy = vi
      .spyOn(window, 'requestAnimationFrame')
      .mockImplementation((callback: FrameRequestCallback) => {
        callback(0);
        return 1;
      });
    installBootGuard(reloadSpy);

    expect(window.__LATTICE_BOOT__?.isMounted()).toBe(false);
    window.__LATTICE_BOOT__?.markMounted();
    expect(window.__LATTICE_BOOT__?.isMounted()).toBe(true);
    expect(rafSpy).toHaveBeenCalledTimes(1);
    expect(document.getElementById('lattice-boot-styles')).toBeNull();

    vi.advanceTimersByTime(15_000);

    const downstreamListener = dispatchWindowError(new Error('Post-mount error'));
    const rejection = new Event('unhandledrejection');
    Object.defineProperty(rejection, 'reason', { value: new Error('Post-mount rejection') });
    window.dispatchEvent(rejection);

    expect(downstreamListener).toHaveBeenCalledTimes(1);
    expect(document.querySelector('[data-boot-recovery="true"]')).toBeNull();
    expect(reloadSpy).not.toHaveBeenCalled();
    expect(sessionStorage.getItem(RETRY_KEY)).toBeNull();
  });

  it('renders recovery UI with overlay drag region, copy details, and host notification on fatal error', async () => {
    sessionStorage.setItem(RETRY_KEY, String(Date.now()));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    const tauriBridge: TauriBridgeMock = { invoke: vi.fn().mockResolvedValue(null) };
    installBootGuard(reloadSpy, tauriBridge);

    dispatchWindowError(new Error('Store initialization exploded'));

    const recovery = document.querySelector('[data-boot-recovery="true"]');
    expect(recovery).not.toBeNull();
    expect(recovery?.querySelector('[data-tauri-drag-region]')).not.toBeNull();
    expect(recovery?.textContent).toContain("Lattice couldn't load.");
    expect(recovery?.textContent).toContain('Store initialization exploded');
    expect(reloadSpy).not.toHaveBeenCalled();

    expect(tauriBridge.invoke).toHaveBeenCalledWith('plugin:event|emit', {
      event: 'lattice:boot-recovery',
      payload: expect.objectContaining({
        kind: 'runtime-error',
        detail: expect.stringContaining('Store initialization exploded'),
      }),
    });

    const buttons = recovery?.querySelectorAll('button') ?? [];
    expect(buttons).toHaveLength(2);
    const [reloadButton, copyButton] = buttons;

    copyButton.click();
    await Promise.resolve();
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText.mock.calls[0]?.[0]).toContain('Store initialization exploded');
    expect(copyButton.textContent).toBe('Copied');

    vi.advanceTimersByTime(2_000);
    expect(copyButton.textContent).toBe('Copy details');

    reloadButton.click();
    expect(sessionStorage.getItem(RETRY_KEY)).toBeNull();
    expect(reloadSpy).toHaveBeenCalledTimes(1);
  });

  it('persists telemetry, notifies host, and auto-reloads at most once on script load failure', () => {
    const tauriBridge: TauriBridgeMock = { invoke: vi.fn().mockResolvedValue(null) };
    installBootGuard(reloadSpy, tauriBridge);

    const script = document.createElement('script');
    script.src = 'http://localhost:5173/src/main.tsx';
    document.body.appendChild(script);
    script.dispatchEvent(new Event('error'));

    expect(reloadSpy).not.toHaveBeenCalled();
    expect(sessionStorage.getItem(RETRY_KEY)).not.toBeNull();

    const storedTelemetry = JSON.parse(
      sessionStorage.getItem(BOOT_FAILURE_STORAGE_KEY) ?? '{}',
    ) as Record<string, string>;
    expect(storedTelemetry.kind).toBe('resource-error');
    expect(storedTelemetry.detail).toContain('/src/main.tsx');
    expect(tauriBridge.invoke).toHaveBeenCalledWith('plugin:event|emit', {
      event: 'lattice:boot-auto-retry',
      payload: expect.objectContaining({
        kind: 'resource-error',
      }),
    });

    vi.advanceTimersByTime(350);
    expect(reloadSpy).toHaveBeenCalledTimes(1);
    expect(document.querySelector('[data-boot-recovery="true"]')).toBeNull();

    // Simulate the reloaded page failing again within the 30s retry window.
    installBootGuard(reloadSpy, tauriBridge);
    script.dispatchEvent(new Event('error'));
    vi.advanceTimersByTime(1_000);

    expect(reloadSpy).toHaveBeenCalledTimes(1);
    const recovery = document.querySelector('[data-boot-recovery="true"]');
    expect(recovery).not.toBeNull();
    expect(recovery?.textContent).toContain('Failed to load application script');
    expect(recovery?.textContent).toContain('/src/main.tsx');
  });

  it('ignores non-boot asset failures and triggers watchdog recovery after 10s of empty root', () => {
    installBootGuard(reloadSpy);

    const img = document.createElement('img');
    img.src = 'http://localhost:5173/missing.png';
    document.body.appendChild(img);
    img.dispatchEvent(new Event('error'));

    expect(document.querySelector('[data-boot-recovery="true"]')).toBeNull();
    expect(reloadSpy).not.toHaveBeenCalled();

    vi.advanceTimersByTime(10_000);
    vi.advanceTimersByTime(350);
    expect(reloadSpy).toHaveBeenCalledTimes(1);

    installBootGuard(reloadSpy);
    vi.advanceTimersByTime(10_000);
    expect(reloadSpy).toHaveBeenCalledTimes(1);

    const recovery = document.querySelector('[data-boot-recovery="true"]');
    expect(recovery).not.toBeNull();
    expect(recovery?.textContent).toContain('did not finish loading within 10 seconds');
  });

  it('disarms silently if #root already contains content when the watchdog fires', () => {
    installBootGuard(reloadSpy);

    const root = document.getElementById('root');
    const appNode = document.createElement('main');
    appNode.textContent = 'Rendered app';
    root?.appendChild(appNode);

    vi.advanceTimersByTime(15_000);

    expect(reloadSpy).not.toHaveBeenCalled();
    expect(document.querySelector('[data-boot-recovery="true"]')).toBeNull();
    expect(root?.textContent).toBe('Rendered app');
  });

  it('auto-reloads once on a dynamic import rejection and shows recovery on a non-transient rejection', () => {
    installBootGuard(reloadSpy);

    const transientRejection = new Event('unhandledrejection');
    Object.defineProperty(transientRejection, 'reason', {
      value: new TypeError('Failed to fetch dynamically imported module: /src/App.tsx'),
    });
    window.dispatchEvent(transientRejection);

    vi.advanceTimersByTime(350);
    expect(reloadSpy).toHaveBeenCalledTimes(1);

    sessionStorage.clear();
    installBootGuard(reloadSpy);
    const fatalRejection = new Event('unhandledrejection');
    Object.defineProperty(fatalRejection, 'reason', {
      value: new Error('Invalid bootstrap configuration'),
    });
    window.dispatchEvent(fatalRejection);

    vi.advanceTimersByTime(1_000);
    expect(reloadSpy).toHaveBeenCalledTimes(1);
    const recovery = document.querySelector('[data-boot-recovery="true"]');
    expect(recovery).not.toBeNull();
    expect(recovery?.textContent).toContain('Invalid bootstrap configuration');
  });

  it('markLoaded detaches error listeners but keeps the deadline armed', () => {
    installBootGuard(reloadSpy);
    window.__LATTICE_BOOT__?.markLoaded();

    const downstream = dispatchWindowError(new Error('Late error'));
    const rejection = new Event('unhandledrejection');
    Object.defineProperty(rejection, 'reason', { value: new Error('Late rejection') });
    window.dispatchEvent(rejection);

    expect(downstream).toHaveBeenCalledTimes(1);
    expect(document.querySelector('[data-boot-recovery="true"]')).toBeNull();
    expect(reloadSpy).not.toHaveBeenCalled();

    vi.advanceTimersByTime(10_000);
    vi.advanceTimersByTime(350);
    expect(reloadSpy).toHaveBeenCalledTimes(1);
    const telemetry = JSON.parse(sessionStorage.getItem(BOOT_FAILURE_STORAGE_KEY) ?? '{}') as Record<
      string,
      string
    >;
    expect(telemetry.kind).toBe('boot-timeout');
    expect(telemetry.detail).toContain('first screen');
  });

  it('markLoaded cancels a pending transient auto-reload and re-arms the deadline', () => {
    installBootGuard(reloadSpy);
    const script = document.createElement('script');
    script.src = 'http://localhost:5173/src/main.tsx';
    document.body.appendChild(script);
    script.dispatchEvent(new Event('error'));

    window.__LATTICE_BOOT__?.markLoaded();
    vi.advanceTimersByTime(350);
    expect(reloadSpy).not.toHaveBeenCalled();

    // The deadline now guards the first render, and the one retry is spent,
    // so a root that stays blank gets the recovery screen, not another reload.
    vi.advanceTimersByTime(10_000);
    expect(reloadSpy).not.toHaveBeenCalled();
    expect(document.querySelector('[data-boot-recovery="true"]')?.textContent).toContain(
      'first screen did not render',
    );
  });

  it('never arms the empty-root deadline under the dev server, but still retries resource errors', () => {
    const devScript = document.createElement('script');
    devScript.dataset.dev = 'true';
    Object.defineProperty(document, 'currentScript', { configurable: true, get: () => devScript });
    try {
      installBootGuard(reloadSpy);
    } finally {
      delete (document as unknown as Record<string, unknown>).currentScript;
    }

    vi.advanceTimersByTime(60_000);
    expect(reloadSpy).not.toHaveBeenCalled();
    expect(document.querySelector('[data-boot-recovery="true"]')).toBeNull();

    const script = document.createElement('script');
    script.src = 'http://localhost:5173/src/main.tsx';
    document.body.appendChild(script);
    script.dispatchEvent(new Event('error'));
    vi.advanceTimersByTime(350);
    expect(reloadSpy).toHaveBeenCalledTimes(1);
  });

  it('treats a bare "Load failed" rejection as non-transient', () => {
    installBootGuard(reloadSpy);
    const rejection = new Event('unhandledrejection');
    Object.defineProperty(rejection, 'reason', { value: new TypeError('Load failed') });
    window.dispatchEvent(rejection);

    vi.advanceTimersByTime(1_000);
    expect(reloadSpy).not.toHaveBeenCalled();
    expect(document.querySelector('[data-boot-recovery="true"]')).not.toBeNull();
  });

  it('markLoaded clears a deferred pre-DOM failure and re-arms the boot deadline', () => {
    Object.defineProperty(document, 'readyState', {
      configurable: true,
      get: () => 'loading',
    });
    try {
      installBootGuard(reloadSpy);
      dispatchWindowError(new Error('Early pre-DOM failure'));
      expect(document.querySelector('[data-boot-recovery="true"]')).toBeNull();

      window.__LATTICE_BOOT__?.markLoaded();
      document.dispatchEvent(new Event('DOMContentLoaded'));
      expect(document.querySelector('[data-boot-recovery="true"]')).toBeNull();

      vi.advanceTimersByTime(10_000);
      vi.advanceTimersByTime(350);
      expect(reloadSpy).toHaveBeenCalledTimes(1);
    } finally {
      delete (document as unknown as Record<string, unknown>).readyState;
    }
  });

  it('formats undefined rejection reasons cleanly instead of rendering "undefined"', () => {
    installBootGuard(reloadSpy);
    const rejection = new Event('unhandledrejection');
    Object.defineProperty(rejection, 'reason', { value: undefined });
    window.dispatchEvent(rejection);

    const recovery = document.querySelector('[data-boot-recovery="true"]');
    expect(recovery).not.toBeNull();
    expect(recovery?.textContent).toContain('Unhandled startup rejection');
    expect(recovery?.textContent).not.toContain('undefined');
  });
});
