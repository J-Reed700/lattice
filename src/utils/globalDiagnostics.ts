import { diagnostics, redact } from './diagnostics';
import { toast } from '../stores/toastStore';


let installed = false;
export function installGlobalDiagnostics(): () => void {
  if (installed) return () => {};
  installed = true;
  let lastNotice = 0;
  const report = (error: unknown, source: string) => {
    diagnostics.capture(error, source);
    if (Date.now() - lastNotice < 5000) return;
    lastNotice = Date.now();
    const latest = diagnostics.getSnapshot()[0];
    toast.error('An unexpected error occurred', {
      message: latest?.message ?? 'Open Settings → Logs for details.',
      duration: 8000,
    });
  };
  const onError = (event: ErrorEvent) => report(event.error ?? event.message, 'Uncaught exception');
  const onRejection = (event: PromiseRejectionEvent) => report(event.reason, 'Unhandled promise');
  const onHide = () => diagnostics.flush();
  const originalError = console.error;
  const originalWarn = console.warn;
  const wrap = (level: 'error' | 'warn', original: typeof console.error) => (...args: unknown[]) => {
    // The structured logger already records its own output.
    if (!(typeof args[0] === 'string' && /^[❌⚠️]/u.test(args[0]))) {
      const message = args.map(arg => {
        try { return arg instanceof Error ? arg.message : typeof arg === 'string' ? arg : JSON.stringify(arg); }
        catch { return '[Unserializable value]'; }
      }).join(' ');
      diagnostics.record(level, redact(message), 'Console', args);
    }
    original.apply(console, args);
  };
  console.error = wrap('error', originalError);
  console.warn = wrap('warn', originalWarn);
  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onRejection);
  window.addEventListener('pagehide', onHide);
  return () => {
    installed = false;
    console.error = originalError;
    console.warn = originalWarn;
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onRejection);
    window.removeEventListener('pagehide', onHide);
    diagnostics.flush();
  };
}
