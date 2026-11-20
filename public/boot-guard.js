// Pre-mount WebView supervisor (`script-src 'self'`, zero external deps).
//
// Pairs with the native `renderer_watchdog` supervisor and works in two stages:
//   1. Until the entry module finishes evaluating (`markLoaded`), it catches
//      boot asset failures, top-level exceptions and unhandled rejections.
//      Transient ones auto-reload once; the rest show a recovery screen.
//   2. Until React's first commit (`markMounted`), only the 10s empty-`#root`
//      deadline stays armed; React's boundaries own errors from here on.
//   Failures persist to `sessionStorage` for `installGlobalDiagnostics()` and
//   are reported over Tauri's event bridge. Under the Vite dev server the
//   deadline is never armed, since a cold start has no fair deadline.
(() => {
  // Vite replaces %DEV% in index.html; a null currentScript (tests) means a packaged build.
  const DEV_SERVER = document.currentScript?.dataset.dev === 'true';
  const BOOT_TIMEOUT_MS = 10000;
  const RETRY_WINDOW_MS = 30000;
  const RETRY_DELAY_MS = 350;
  const RETRY_KEY = 'lattice:boot-retry';
  const FAILURE_LOG_KEY = 'lattice:boot-last-failure';
  const STYLE_ELEMENT_ID = 'lattice-boot-styles';
  const MODULE_ERROR_PATTERN =
    /Failed to fetch dynamically imported module|Importing a module script failed|error loading dynamically imported module|Outdated Optimize Dep/i;

  if (window.__LATTICE_BOOT__ && typeof window.__LATTICE_BOOT__.disarm === 'function') {
    window.__LATTICE_BOOT__.disarm();
  }

  let loaded = false;
  let mounted = false;
  let failed = false;
  let pendingFailure = null;
  let watchdogId = null;
  let retryTimerId = null;

  function readRetryTimestamp() {
    try {
      const raw = sessionStorage.getItem(RETRY_KEY);
      if (!raw) return null;
      const parsed = Number(raw);
      return Number.isFinite(parsed) ? parsed : null;
    } catch {
      return null;
    }
  }

  function writeRetryTimestamp(now) {
    try {
      sessionStorage.setItem(RETRY_KEY, String(now));
      return true;
    } catch {
      return false;
    }
  }

  function clearRetryTimestamp() {
    try {
      sessionStorage.removeItem(RETRY_KEY);
    } catch {
      // Storage can be unavailable in restricted contexts.
    }
  }

  function persistFailureTelemetry(failure) {
    try {
      sessionStorage.setItem(
        FAILURE_LOG_KEY,
        JSON.stringify({
          kind: failure.kind,
          detail: failure.detail,
          url: window.location.href,
          occurredAt: new Date().toISOString(),
        }),
      );
    } catch {
      // Never let telemetry persistence block recovery.
    }
  }

  function emitHostEvent(event, failure) {
    try {
      const internals = window.__TAURI_INTERNALS__;
      if (internals && typeof internals.invoke === 'function') {
        Promise.resolve(
          internals.invoke('plugin:event|emit', {
            event,
            payload: {
              kind: failure.kind,
              detail: failure.detail,
              url: window.location.href,
            },
          }),
        ).catch(() => {});
      }
    } catch {
      // Plain browser / test context without Tauri bridge.
    }
  }

  function clearTimers() {
    if (watchdogId !== null) {
      clearTimeout(watchdogId);
      watchdogId = null;
    }
    if (retryTimerId !== null) {
      clearTimeout(retryTimerId);
      retryTimerId = null;
    }
  }

  function detachListeners() {
    window.removeEventListener('error', onError, true);
    window.removeEventListener('unhandledrejection', onRejection);
    document.removeEventListener('DOMContentLoaded', onDomReady);
  }

  function disarm() {
    clearTimers();
    detachListeners();
  }

  function buildDiagnosticReport(failure) {
    return [
      `Kind: ${failure.kind}`,
      `Detail: ${failure.detail}`,
      `Timestamp: ${new Date().toISOString()}`,
      `URL: ${window.location.href}`,
      `User Agent: ${navigator.userAgent}`,
    ].join('\n');
  }

  function renderRecovery(failure) {
    if (mounted) return;
    const container = document.getElementById('root') || document.body;
    if (!container) {
      pendingFailure = failure;
      return;
    }
    pendingFailure = null;

    const shell = document.createElement('div');
    shell.setAttribute('data-boot-recovery', 'true');
    shell.setAttribute('role', 'alert');

    // Preserve window dragging under macOS's `Overlay` titlebar style.
    const dragRegion = document.createElement('div');
    dragRegion.className = 'lattice-boot-drag';
    dragRegion.setAttribute('data-tauri-drag-region', '');
    shell.appendChild(dragRegion);

    const card = document.createElement('div');
    card.className = 'lattice-boot-card';

    const heading = document.createElement('h1');
    heading.className = 'lattice-boot-heading';
    heading.textContent = "Lattice couldn't load.";

    const message = document.createElement('p');
    message.className = 'lattice-boot-body';
    message.textContent =
      'Something stopped the window from starting up. Reloading usually fixes it.';

    const actions = document.createElement('div');
    actions.className = 'lattice-boot-actions';

    const reloadButton = document.createElement('button');
    reloadButton.type = 'button';
    reloadButton.className = 'lattice-boot-btn';
    reloadButton.textContent = 'Reload';
    reloadButton.addEventListener('click', () => {
      clearRetryTimestamp();
      window.location.reload();
    });
    actions.appendChild(reloadButton);

    const copyButton = document.createElement('button');
    copyButton.type = 'button';
    copyButton.className = 'lattice-boot-btn-ghost';
    copyButton.textContent = 'Copy details';
    copyButton.addEventListener('click', () => {
      if (!navigator.clipboard || typeof navigator.clipboard.writeText !== 'function') return;
      Promise.resolve(navigator.clipboard.writeText(buildDiagnosticReport(failure)))
        .then(() => {
          copyButton.textContent = 'Copied';
          setTimeout(() => {
            copyButton.textContent = 'Copy details';
          }, 2000);
        })
        .catch(() => {});
    });
    actions.appendChild(copyButton);

    card.appendChild(heading);
    card.appendChild(message);
    card.appendChild(actions);

    if (failure.detail) {
      const details = document.createElement('details');
      details.className = 'lattice-boot-details';

      const summary = document.createElement('summary');
      summary.className = 'lattice-boot-summary';
      summary.textContent = 'Show details';

      const trace = document.createElement('pre');
      trace.className = 'lattice-boot-trace';
      trace.textContent = failure.detail;

      details.appendChild(summary);
      details.appendChild(trace);
      card.appendChild(details);
    }

    shell.appendChild(card);
    container.replaceChildren(shell);
    emitHostEvent('lattice:boot-recovery', failure);
  }

  function handleBootFailure(kind, detail, isTransient) {
    if (mounted || failed) return;
    failed = true;
    clearTimers();
    detachListeners();

    const failure = { kind, detail };
    persistFailureTelemetry(failure);

    if (isTransient) {
      const now = Date.now();
      const lastRetry = readRetryTimestamp();
      const canAutoRetry = lastRetry === null || now - lastRetry > RETRY_WINDOW_MS;
      if (canAutoRetry && writeRetryTimestamp(now)) {
        emitHostEvent('lattice:boot-auto-retry', failure);
        retryTimerId = setTimeout(() => {
          retryTimerId = null;
          if (!mounted) {
            window.location.reload();
          }
        }, RETRY_DELAY_MS);
        return;
      }
    }

    if (document.readyState === 'loading') {
      pendingFailure = failure;
      document.addEventListener('DOMContentLoaded', onDomReady);
    } else {
      renderRecovery(failure);
    }
  }

  function onDomReady() {
    document.removeEventListener('DOMContentLoaded', onDomReady);
    if (!mounted && pendingFailure !== null) {
      renderRecovery(pendingFailure);
    }
  }

  function formatError(value) {
    if (value instanceof Error) {
      return value.stack || `${value.name}: ${value.message}`;
    }
    if (typeof value === 'string' && value.trim() !== '') {
      return value;
    }
    try {
      const serialized = JSON.stringify(value);
      return serialized !== undefined ? serialized : String(value);
    } catch {
      return String(value);
    }
  }

  function isBootResourceTarget(target) {
    if (!target || typeof target !== 'object' || !('tagName' in target)) return false;
    const tag = String(target.tagName).toUpperCase();
    if (tag === 'SCRIPT') return true;
    if (tag === 'LINK') {
      const rel = String(target.rel || '').toLowerCase();
      return rel === 'modulepreload' || rel === 'stylesheet';
    }
    return false;
  }

  function onError(event) {
    if (mounted || failed) return;

    const target = event && event.target;
    const isWindowTarget = !target || target === window || target.window === target;
    if (!isWindowTarget) {
      if (isBootResourceTarget(target)) {
        const resource = target.src || target.href || target.tagName;
        handleBootFailure('resource-error', `Failed to load application script: ${resource}`, true);
      }
      return;
    }

    const detail = formatError(
      (event && (event.error || event.message)) || 'Unknown startup error',
    );
    handleBootFailure('runtime-error', detail, MODULE_ERROR_PATTERN.test(detail));
  }

  function onRejection(event) {
    if (mounted || failed) return;
    const reason = event && event.reason !== undefined ? event.reason : 'Unhandled startup rejection';
    const detail = formatError(reason);
    handleBootFailure('unhandled-rejection', detail, MODULE_ERROR_PATTERN.test(detail));
  }

  window.addEventListener('error', onError, true);
  window.addEventListener('unhandledrejection', onRejection);

  function armDeadline() {
    // No deadline under the dev server: a Vite cold start has no fair one.
    if (DEV_SERVER) return;
    watchdogId = setTimeout(() => {
      watchdogId = null;
      if (mounted || failed) return;
      const root = document.getElementById('root');
      if (root && root.childElementCount > 0) {
        disarm();
        return;
      }
      handleBootFailure(
        'boot-timeout',
        loaded
          ? `The app's code loaded, but its first screen did not render within ${BOOT_TIMEOUT_MS / 1000} seconds.`
          : `The app's code did not finish loading within ${BOOT_TIMEOUT_MS / 1000} seconds.`,
        true,
      );
    }, BOOT_TIMEOUT_MS);
  }

  armDeadline();

  window.__LATTICE_BOOT__ = {
    markLoaded() {
      if (loaded || mounted) return;
      loaded = true;
      detachListeners();
      if (retryTimerId !== null || pendingFailure !== null) {
        // The entry ran, so any earlier pre-DOM reload or queued failure is superseded;
        // re-arm the render deadline so a failed React mount still surfaces recovery UI.
        if (retryTimerId !== null) {
          clearTimeout(retryTimerId);
          retryTimerId = null;
        }
        pendingFailure = null;
        failed = false;
        armDeadline();
      }
    },
    markMounted() {
      if (mounted) return;
      mounted = true;
      disarm();
      const finalizeBoot = () => {
        clearRetryTimestamp();
        const bootStyle = document.getElementById(STYLE_ELEMENT_ID);
        if (bootStyle && bootStyle.parentNode) {
          bootStyle.parentNode.removeChild(bootStyle);
        }
      };
      if (typeof requestAnimationFrame === 'function') {
        requestAnimationFrame(finalizeBoot);
      } else {
        finalizeBoot();
      }
    },
    isMounted() {
      return mounted;
    },
    disarm,
  };
})();
