/** Coalesce frequent UI updates to one animation frame, with a hidden-tab fallback. */
export function createFrameBatcher(callback: () => void, fallbackMs = 100) {
  let frame: number | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const flush = () => {
    if (frame !== null && typeof cancelAnimationFrame === 'function') cancelAnimationFrame(frame);
    if (timer !== null) clearTimeout(timer);
    frame = null;
    timer = null;
    callback();
  };
  const schedule = () => {
    if (frame === null && typeof requestAnimationFrame === 'function') {
      frame = requestAnimationFrame(flush);
    }
    if (timer === null) timer = setTimeout(flush, fallbackMs);
  };
  const cancel = () => {
    if (frame !== null && typeof cancelAnimationFrame === 'function') cancelAnimationFrame(frame);
    if (timer !== null) clearTimeout(timer);
    frame = null;
    timer = null;
  };
  return { schedule, flush, cancel };
}
